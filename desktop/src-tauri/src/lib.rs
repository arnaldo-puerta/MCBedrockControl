pub mod bds;
pub mod config;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use bds::EXECUTABLE;
use bds::allowlist::{self, AllowEntry};
use bds::installer;
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
    state.supervisor.start(&state.server_path()?).await
}

#[tauri::command]
async fn stop_server(state: State<'_, AppState>) -> Result<(), String> {
    state.supervisor.stop().await;
    Ok(())
}

#[tauri::command]
async fn restart_server(state: State<'_, AppState>) -> Result<(), String> {
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
            app.manage(AppState {
                supervisor: Supervisor::new(Arc::new(TauriSink(handle))),
                config: Mutex::new(Config::load(&config_path)),
                config_path,
                closing: AtomicBool::new(false),
                installing: AtomicBool::new(false),
                cancel_install: Arc::new(AtomicBool::new(false)),
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
            get_allow_list,
            add_allowed_player,
            remove_allowed_player,
            set_allow_list_enabled,
            kick_player,
            start_server,
            stop_server,
            restart_server,
            send_command,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
