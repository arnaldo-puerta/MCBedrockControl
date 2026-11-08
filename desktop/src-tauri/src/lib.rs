pub mod bds;
pub mod config;
pub mod connect;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use bds::EXECUTABLE;
use bds::allowlist::{self, AllowEntry};
use bds::backup::{self, BackupInfo, Reason};
use bds::installer;
use bds::{packs, settings, updater};
use bds::worlds::{self, WorldInfo};
use bds::properties::{self, ServerInfo};
use bds::supervisor::{Event, ServerState, Sink, Snapshot, Supervisor};
use config::Config;

const SERVER_EVENT: &str = "server-event";
const INSTALL_EVENT: &str = "install-progress";

struct AppState {
    supervisor: Supervisor,
    config: Mutex<Config>,
    config_path: PathBuf,
    closing: AtomicBool,
    installing: AtomicBool,
    cancel_install: Arc<AtomicBool>,
    backups_dir: PathBuf,
    // Backups, restores, imports and deletions run one at a time, and the
    // server can't be stopped mid-backup while its world is held.
    world_ops: tokio::sync::Mutex<()>,
}

impl AppState {
    fn server_path(&self) -> Result<PathBuf, String> {
        self.config
            .lock()
            .expect("config lock poisoned")
            .server_path
            .clone()
            .ok_or_else(|| "Selecciona primero la carpeta del servidor.".to_owned())
    }

    fn update_config(&self, change: impl FnOnce(&mut Config)) -> Result<(), String> {
        let mut config = self.config.lock().expect("config lock poisoned");
        change(&mut config);
        config
            .save(&self.config_path)
            .map_err(|e| format!("No se pudo guardar la configuración: {e}"))
    }
}

struct TauriSink(AppHandle);

impl Sink for TauriSink {
    fn emit(&self, event: Event) {
        if let Event::Version(version) = &event {
            let state = self.0.state::<AppState>();
            let _ = state.update_config(|config| config.server_version = Some(version.clone()));
        }
        let _ = self.0.emit(SERVER_EVENT, event);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    server_path: Option<PathBuf>,
    version: Option<String>,
    info: ServerInfo,
    server: Snapshot,
}

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> AppSnapshot {
    let config = state.config.lock().expect("config lock poisoned").clone();
    let info = config
        .server_path
        .as_deref()
        .map(properties::read_info)
        .unwrap_or_default();
    AppSnapshot {
        server_path: config.server_path,
        version: config.server_version,
        info,
        server: state.supervisor.snapshot(),
    }
}

#[tauri::command]
fn set_server_path(path: PathBuf, state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    if state.supervisor.is_active() {
        return Err("Detén el servidor antes de cambiar de carpeta.".to_owned());
    }
    if !path.join(EXECUTABLE).is_file() {
        return Err(format!("Esa carpeta no contiene {EXECUTABLE}."));
    }
    state.update_config(|config| {
        config.server_path = Some(path);
        config.server_version = None;
    })?;
    Ok(get_snapshot(state))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Playit {
    port: String,
    status: connect::PlayitStatus,
}

#[tauri::command]
async fn get_playit(state: State<'_, AppState>) -> Result<Playit, String> {
    let port = properties::read_value(&state.server_path()?, "server-port")
        .filter(|port| !port.is_empty())
        .unwrap_or_else(|| "19132".to_owned());
    Ok(Playit { port, status: connect::playit().await })
}

#[tauri::command]
async fn install_playit() -> Result<connect::PlayitStatus, String> {
    connect::install_playit().await
}

#[derive(Serialize)]
struct PlayitLink {
    code: String,
    url: String,
}

#[tauri::command]
async fn start_playit_link(replace: bool) -> Result<PlayitLink, String> {
    let code = connect::start_link(replace).await?;
    Ok(PlayitLink { url: connect::claim_url(&code), code })
}

#[tauri::command]
async fn finish_playit_link(code: String) -> Result<connect::PlayitStatus, String> {
    connect::finish_link(&code).await
}

#[tauri::command]
fn playit_tunnels_url() -> &'static str {
    connect::tunnels_url()
}

#[tauri::command]
async fn set_playit_running(running: bool) -> Result<connect::PlayitStatus, String> {
    connect::set_playit_running(running).await
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H-%M-%S").to_string()
}

fn require_stopped(state: &AppState) -> Result<(), String> {
    if state.supervisor.is_active() {
        return Err("Detén el servidor primero.".to_owned());
    }
    Ok(())
}

fn require_no_world_op(state: &AppState) -> Result<(), String> {
    if state.world_ops.try_lock().is_err() {
        return Err("Espera a que termine la copia o el cambio de mundos en curso.".to_owned());
    }
    Ok(())
}

#[tauri::command]
async fn check_update(state: State<'_, AppState>) -> Result<updater::UpdateCheck, String> {
    let current = state.config.lock().expect("config lock poisoned").server_version.clone();
    updater::check(current).await
}

#[tauri::command]
async fn update_server(app: AppHandle, state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    require_stopped(&state)?;
    let server_dir = state.server_path()?;
    if state.installing.swap(true, Ordering::SeqCst) {
        return Err("Ya hay una instalación o actualización en curso.".to_owned());
    }
    state.cancel_install.store(false, Ordering::SeqCst);

    let result = async {
        let _ops = state.world_ops.lock().await;
        if let Some(world) = worlds::active(&server_dir)
            .filter(|world| worlds::worlds_dir(&server_dir).join(world).join("level.dat").is_file())
        {
            backup::create(&state.supervisor, &server_dir, &state.backups_dir, &world, Reason::BeforeUpdate, &now())
                .await?;
        }
        let reporter = app.clone();
        updater::update(&server_dir, state.cancel_install.clone(), move |progress| {
            let _ = reporter.emit(INSTALL_EVENT, progress);
        })
        .await
    }
    .await;
    state.installing.store(false, Ordering::SeqCst);

    let version = result?;
    state.update_config(|config| config.server_version = Some(version))?;
    Ok(get_snapshot(state))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Packs {
    packs: Vec<packs::PackInfo>,
    texturepack_required: bool,
    has_world: bool,
}

fn read_packs(server_dir: &Path) -> Result<Packs, String> {
    Ok(Packs {
        packs: packs::list(server_dir)?,
        texturepack_required: properties::read_value(server_dir, "texturepack-required").as_deref() == Some("true"),
        has_world: worlds::active(server_dir)
            .is_some_and(|world| worlds::worlds_dir(server_dir).join(world).join("level.dat").is_file()),
    })
}

#[tauri::command]
fn list_packs(state: State<'_, AppState>) -> Result<Packs, String> {
    read_packs(&state.server_path()?)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<settings::ServerSettings, String> {
    Ok(settings::read(&state.server_path()?))
}

#[tauri::command]
async fn save_settings(settings: settings::ServerSettings, state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let server_dir = state.server_path()?;
    settings::validate(&settings)?;

    let current = settings::read(&server_dir).world_name;
    let new_world_name = settings.world_name.as_deref().map(str::trim);
    if new_world_name.is_some() && new_world_name != current.as_deref() {
        require_stopped(&state)?;
        let _ops = state.world_ops.lock().await;
        let folder = worlds::active(&server_dir).ok_or("El servidor no tiene un mundo todavía.")?;
        backup::create(&state.supervisor, &server_dir, &state.backups_dir, &folder, Reason::BeforeRename, &now())
            .await?;
        settings::rename_world(&server_dir, new_world_name.unwrap_or_default())?;
    }
    settings::write(&server_dir, &settings)?;
    Ok(get_snapshot(state))
}

#[tauri::command]
async fn import_packs(path: PathBuf, state: State<'_, AppState>) -> Result<Packs, String> {
    let server_dir = state.server_path()?;
    let dir = server_dir.clone();
    tauri::async_runtime::spawn_blocking(move || packs::import(&dir, &path))
        .await
        .map_err(|e| e.to_string())??;
    read_packs(&server_dir)
}

#[tauri::command]
fn set_pack_enabled(
    kind: packs::PackKind,
    uuid: String,
    version: Vec<u64>,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<Packs, String> {
    let server_dir = state.server_path()?;
    packs::set_enabled(&server_dir, kind, &uuid, &version, enabled)?;
    read_packs(&server_dir)
}

#[tauri::command]
fn remove_pack(kind: packs::PackKind, folder: String, state: State<'_, AppState>) -> Result<Packs, String> {
    let server_dir = state.server_path()?;
    packs::remove(&server_dir, kind, &folder)?;
    read_packs(&server_dir)
}

#[tauri::command]
fn set_texturepack_required(required: bool, state: State<'_, AppState>) -> Result<Packs, String> {
    let server_dir = state.server_path()?;
    properties::write_value(&server_dir, "texturepack-required", if required { "true" } else { "false" })?;
    read_packs(&server_dir)
}

#[tauri::command]
fn list_worlds(state: State<'_, AppState>) -> Result<Vec<WorldInfo>, String> {
    worlds::list(&state.server_path()?)
}

#[tauri::command]
async fn import_world(path: PathBuf, state: State<'_, AppState>) -> Result<Vec<WorldInfo>, String> {
    let server_dir = state.server_path()?;
    let _ops = state.world_ops.lock().await;
    let dir = server_dir.clone();
    tauri::async_runtime::spawn_blocking(move || worlds::unpack(&dir, &path, None, false))
        .await
        .map_err(|e| e.to_string())??;
    worlds::list(&server_dir)
}

#[tauri::command]
async fn export_world(folder: String, destination: PathBuf, state: State<'_, AppState>) -> Result<(), String> {
    let server_dir = state.server_path()?;
    let _ops = state.world_ops.lock().await;
    backup::snapshot(&state.supervisor, &server_dir, &folder, &destination).await
}

#[tauri::command]
fn set_active_world(folder: String, state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    require_stopped(&state)?;
    require_no_world_op(&state)?;
    worlds::set_active(&state.server_path()?, &folder)?;
    Ok(get_snapshot(state))
}

#[tauri::command]
async fn delete_world(folder: String, state: State<'_, AppState>) -> Result<Vec<WorldInfo>, String> {
    let server_dir = state.server_path()?;
    let _ops = state.world_ops.lock().await;
    backup::create(&state.supervisor, &server_dir, &state.backups_dir, &folder, Reason::BeforeDelete, &now()).await?;
    worlds::delete(&server_dir, &folder)?;
    worlds::list(&server_dir)
}

#[derive(Serialize)]
struct Backups {
    dir: PathBuf,
    backups: Vec<BackupInfo>,
}

#[tauri::command]
fn list_backups(state: State<'_, AppState>) -> Backups {
    Backups { dir: state.backups_dir.clone(), backups: backup::list(&state.backups_dir) }
}

#[tauri::command]
async fn create_backup(state: State<'_, AppState>) -> Result<Backups, String> {
    let server_dir = state.server_path()?;
    let folder = worlds::active(&server_dir).ok_or("El servidor no tiene un mundo activo.")?;
    let _ops = state.world_ops.lock().await;
    backup::create(&state.supervisor, &server_dir, &state.backups_dir, &folder, Reason::Manual, &now()).await?;
    Ok(Backups { dir: state.backups_dir.clone(), backups: backup::list(&state.backups_dir) })
}

#[tauri::command]
async fn restore_backup(file: String, state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    require_stopped(&state)?;
    let server_dir = state.server_path()?;
    let _ops = state.world_ops.lock().await;
    let archive = backup::path_of(&state.backups_dir, &file)?;
    let info = backup::list(&state.backups_dir)
        .into_iter()
        .find(|b| b.file == file)
        .ok_or("No se encontró la copia.")?;

    // Restoring replaces the world, so keep what's there now first.
    if worlds::worlds_dir(&server_dir).join(&info.world).join("level.dat").is_file() {
        backup::create(&state.supervisor, &server_dir, &state.backups_dir, &info.world, Reason::BeforeRestore, &now())
            .await?;
    }
    let dir = server_dir.clone();
    let folder = info.world.clone();
    tauri::async_runtime::spawn_blocking(move || worlds::unpack(&dir, &archive, Some(&folder), true))
        .await
        .map_err(|e| e.to_string())??;
    worlds::set_active(&server_dir, &info.world)?;
    drop(_ops);
    Ok(get_snapshot(state))
}

#[tauri::command]
fn delete_backup(file: String, state: State<'_, AppState>) -> Result<Backups, String> {
    let path = backup::path_of(&state.backups_dir, &file)?;
    std::fs::remove_file(path).map_err(|e| format!("No se pudo borrar la copia: {e}"))?;
    Ok(list_backups(state))
}

#[derive(Serialize)]
struct AllowList {
    enabled: bool,
    entries: Vec<AllowEntry>,
}

fn read_allow_list(server_dir: &Path) -> Result<AllowList, String> {
    Ok(AllowList {
        enabled: properties::read_value(server_dir, "allow-list").as_deref() == Some("true"),
        entries: allowlist::read(server_dir)?,
    })
}

/// Saves a change to disk and, if the server is up, tells it to apply it now.
fn change_allow_list(
    state: &AppState,
    change: impl FnOnce(&Path) -> Result<(), String>,
    live_command: &str,
) -> Result<AllowList, String> {
    let server_dir = state.server_path()?;
    let server_state = state.supervisor.state();
    if matches!(server_state, ServerState::Starting | ServerState::Stopping) {
        return Err("Espera a que el servidor termine de iniciar o detenerse.".to_owned());
    }
    change(&server_dir)?;
    if server_state == ServerState::Running {
        state.supervisor.send_command(live_command)?;
    }
    read_allow_list(&server_dir)
}

#[tauri::command]
fn get_allow_list(state: State<'_, AppState>) -> Result<AllowList, String> {
    read_allow_list(&state.server_path()?)
}

#[tauri::command]
fn add_allowed_player(name: String, state: State<'_, AppState>) -> Result<AllowList, String> {
    change_allow_list(&state, |dir| allowlist::add(dir, &name), "allowlist reload")
}

#[tauri::command]
fn remove_allowed_player(name: String, state: State<'_, AppState>) -> Result<AllowList, String> {
    change_allow_list(&state, |dir| allowlist::remove(dir, &name), "allowlist reload")
}

#[tauri::command]
fn set_allow_list_enabled(enabled: bool, state: State<'_, AppState>) -> Result<AllowList, String> {
    let (value, command) = if enabled { ("true", "allowlist on") } else { ("false", "allowlist off") };
    change_allow_list(&state, |dir| properties::write_value(dir, "allow-list", value), command)
}

#[tauri::command]
fn kick_player(name: String, state: State<'_, AppState>) -> Result<(), String> {
    let name = allowlist::clean_name(&name)?;
    state.supervisor.send_command(&format!("kick \"{name}\""))
}

#[tauri::command]
fn default_install_path(app: AppHandle) -> Result<PathBuf, String> {
    let data = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    Ok(data.join("Servidor Bedrock"))
}

#[tauri::command]
async fn install_server(
    path: PathBuf,
    eula_accepted: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSnapshot, String> {
    if !eula_accepted {
        return Err("Debes aceptar el EULA de Minecraft para instalar el servidor.".to_owned());
    }
    if state.supervisor.is_active() {
        return Err("Detén el servidor antes de instalar otro.".to_owned());
    }
    if state.installing.swap(true, Ordering::SeqCst) {
        return Err("Ya hay una instalación en curso.".to_owned());
    }
    state.cancel_install.store(false, Ordering::SeqCst);

    let reporter = app.clone();
    let result = installer::install(&path, state.cancel_install.clone(), move |progress| {
        let _ = reporter.emit(INSTALL_EVENT, progress);
    })
    .await;
    state.installing.store(false, Ordering::SeqCst);

    let version = result?;
    state.update_config(|config| {
        config.server_path = Some(path);
        config.server_version = Some(version);
    })?;
    Ok(get_snapshot(state))
}

#[tauri::command]
fn cancel_install(state: State<'_, AppState>) {
    state.cancel_install.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn start_server(state: State<'_, AppState>) -> Result<(), String> {
    if state.installing.load(Ordering::SeqCst) {
        return Err("Espera a que termine la instalación o actualización.".to_owned());
    }
    state.supervisor.start(&state.server_path()?).await
}

#[tauri::command]
async fn stop_server(state: State<'_, AppState>) -> Result<(), String> {
    require_no_world_op(&state)?;
    state.supervisor.stop().await;
    Ok(())
}

#[tauri::command]
async fn restart_server(state: State<'_, AppState>) -> Result<(), String> {
    require_no_world_op(&state)?;
    state.supervisor.restart(&state.server_path()?).await
}

#[tauri::command]
fn send_command(command: String, state: State<'_, AppState>) -> Result<(), String> {
    state.supervisor.send_command(&command)
}

fn config_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join(Path::new("config.json")))
}

/// Never let the window close on a live server: quitting without `stop`
/// would lose whatever the world hasn't saved yet.
fn stop_then_exit(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.closing.swap(true, Ordering::SeqCst) {
        return;
    }
    let supervisor = state.supervisor.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        supervisor.stop().await;
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let config_path = config_path(&handle)?;
            let backups_dir = handle.path().app_local_data_dir()?.join("Copias de seguridad");
            app.manage(AppState {
                supervisor: Supervisor::new(Arc::new(TauriSink(handle))),
                config: Mutex::new(Config::load(&config_path)),
                config_path,
                closing: AtomicBool::new(false),
                installing: AtomicBool::new(false),
                cancel_install: Arc::new(AtomicBool::new(false)),
                backups_dir,
                world_ops: tokio::sync::Mutex::new(()),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if app.state::<AppState>().supervisor.is_active() {
                    api.prevent_close();
                    stop_then_exit(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_server_path,
            default_install_path,
            install_server,
            cancel_install,
            check_update,
            update_server,
            list_packs,
            import_packs,
            set_pack_enabled,
            remove_pack,
            set_texturepack_required,
            get_settings,
            save_settings,
            get_allow_list,
            add_allowed_player,
            remove_allowed_player,
            set_allow_list_enabled,
            kick_player,
            list_worlds,
            import_world,
            export_world,
            set_active_world,
            delete_world,
            list_backups,
            create_backup,
            restore_backup,
            delete_backup,
            get_playit,
            install_playit,
            start_playit_link,
            finish_playit_link,
            playit_tunnels_url,
            set_playit_running,
            start_server,
            stop_server,
            restart_server,
            send_command,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
