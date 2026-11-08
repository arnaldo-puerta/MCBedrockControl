//! World backups as `.mcworld` files, which Minecraft itself can also open.
//!
//! A running server is backed up with `save hold`: it flushes the world and
//! reports how many bytes of each file are consistent, and we copy exactly
//! that before letting it write again with `save resume`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::mpsc;

use super::archive;
use super::supervisor::{ServerState, Supervisor};
use super::worlds;

pub const EXTENSION: &str = "mcworld";
const SEPARATOR: &str = " - ";
const KEEP_AUTOMATIC: usize = 10;
const READY_MARKER: &str = "Files are now ready to be copied.";
const RESUMED_MARKER: &str = "Changes to the world are resumed.";
const QUERY_INTERVAL: Duration = Duration::from_millis(250);
const QUERY_ATTEMPTS: u32 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    Manual,
    BeforeDelete,
    BeforeRestore,
    BeforeUpdate,
    BeforeRename,
}

impl Reason {
    fn slug(self) -> &'static str {
        match self {
            Reason::Manual => "manual",
            Reason::BeforeDelete => "antes de borrar",
            Reason::BeforeRestore => "antes de restaurar",
            Reason::BeforeUpdate => "antes de actualizar",
            Reason::BeforeRename => "antes de renombrar",
        }
    }

    fn from_slug(slug: &str) -> Option<Self> {
        [Reason::Manual, Reason::BeforeDelete, Reason::BeforeRestore, Reason::BeforeUpdate, Reason::BeforeRename]
            .into_iter()
            .find(|reason| reason.slug() == slug)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub file: String,
    pub world: String,
    pub reason: Reason,
    pub created: String,
    pub size_bytes: u64,
}

/// `<world> - <YYYY-MM-DD HH-MM-SS> - <reason>.mcworld`, readable in Explorer.
pub fn file_name(world: &str, reason: Reason, created: &str) -> String {
    format!("{world}{SEPARATOR}{created}{SEPARATOR}{}.{EXTENSION}", reason.slug())
}

fn parse_name(file: &str) -> Option<(String, Reason, String)> {
    let stem = file.strip_suffix(&format!(".{EXTENSION}"))?;
    let mut parts = stem.rsplitn(3, SEPARATOR);
    let reason = Reason::from_slug(parts.next()?)?;
    let created = parts.next()?.to_owned();
    let world = parts.next()?.to_owned();
    Some((world, reason, created))
}

pub fn list(backups_dir: &Path) -> Vec<BackupInfo> {
    let Ok(entries) = std::fs::read_dir(backups_dir) else {
        return Vec::new();
    };
    let mut backups: Vec<BackupInfo> = entries
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let (world, reason, created) = parse_name(&file)?;
            let size_bytes = entry.metadata().ok()?.len();
            Some(BackupInfo { file, world, reason, created, size_bytes })
        })
        .collect();
    backups.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| a.file.cmp(&b.file)));
    backups
}

/// Keeps the newest automatic backups; manual ones are only removed by hand.
pub fn prune(backups_dir: &Path) {
    let automatic = list(backups_dir).into_iter().filter(|b| b.reason != Reason::Manual);
    for old in automatic.skip(KEEP_AUTOMATIC) {
        let _ = std::fs::remove_file(backups_dir.join(old.file));
    }
}

pub fn path_of(backups_dir: &Path, file: &str) -> Result<PathBuf, String> {
    let file = worlds::plain_name(file)?;
    if parse_name(file).is_none() {
        return Err(format!("No es una copia de seguridad: {file}"));
    }
    let path = backups_dir.join(file);
    if path.is_file() { Ok(path) } else { Err(format!("No existe la copia {file}.")) }
}

/// Parses the line after the ready marker:
/// `Bedrock level/db/000008.ldb:598520, Bedrock level/level.dat:3056`.
/// Splitting on the world prefix, not on ", ", keeps world names that
/// contain commas intact.
pub fn parse_save_list(line: &str, folder: &str) -> Result<Vec<(PathBuf, u64)>, String> {
    let prefix = format!("{folder}/");
    let invalid = || format!("Respuesta inesperada del servidor: {line}");
    let rest = line.trim().strip_prefix(&prefix).ok_or_else(invalid)?;

    rest.split(&format!(", {prefix}"))
        .map(|item| {
            let (path, length) = item.rsplit_once(':').ok_or_else(invalid)?;
            let length = length.trim().parse().map_err(|_| invalid())?;
            let path = PathBuf::from(path);
            let safe = path.components().all(|c| matches!(c, std::path::Component::Normal(_)));
            if safe { Ok((path, length)) } else { Err(invalid()) }
        })
        .collect()
}

async fn next_line(lines: &mut mpsc::UnboundedReceiver<String>, wait: Duration) -> Option<String> {
    tokio::time::timeout(wait, lines.recv()).await.ok().flatten()
}

async fn wait_for_save(
    supervisor: &Supervisor,
    lines: &mut mpsc::UnboundedReceiver<String>,
    folder: &str,
) -> Result<Vec<(PathBuf, u64)>, String> {
    for _ in 0..QUERY_ATTEMPTS {
        supervisor.send_quietly("save query")?;
        while let Some(line) = next_line(lines, QUERY_INTERVAL).await {
            if line.contains(READY_MARKER) {
                let list = next_line(lines, Duration::from_secs(5))
                    .await
                    .ok_or("El servidor no envió la lista de archivos.")?;
                return parse_save_list(&list, folder);
            }
        }
    }
    Err("El servidor tardó demasiado en preparar la copia.".to_owned())
}

async fn snapshot_running(
    supervisor: &Supervisor,
    world_dir: &Path,
    folder: &str,
    destination: &Path,
) -> Result<(), String> {
    let mut lines = supervisor.watch_lines();
    supervisor.send_quietly("save hold")?;

    let result = async {
        let files = wait_for_save(supervisor, &mut lines, folder).await?;
        let (root, destination) = (world_dir.to_owned(), destination.to_owned());
        tokio::task::spawn_blocking(move || archive::write(&root, &files, &destination))
            .await
            .map_err(|e| format!("La copia se interrumpió: {e}"))?
    }
    .await;

    // Always let the world change again, even if the copy failed, and only
    // report back once the server confirms it.
    if supervisor.send_quietly("save resume").is_ok() {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while let Some(line) = next_line(&mut lines, deadline - tokio::time::Instant::now()).await {
            if line.contains(RESUMED_MARKER) {
                break;
            }
        }
    }
    result
}

/// Copies `folder` into `destination`, live if the server is running.
pub async fn snapshot(
    supervisor: &Supervisor,
    server_dir: &Path,
    folder: &str,
    destination: &Path,
) -> Result<(), String> {
    let folder = worlds::plain_name(folder)?;
    let world_dir = worlds::worlds_dir(server_dir).join(folder);
    if !world_dir.join("level.dat").is_file() {
        return Err(format!("No existe el mundo {folder}."));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("No se pudo crear la carpeta: {e}"))?;
    }

    // Only the loaded world is held by the server; others are safe to copy.
    let loaded = worlds::active(server_dir).as_deref() == Some(folder);
    match supervisor.state() {
        ServerState::Running if loaded => {
            snapshot_running(supervisor, &world_dir, folder, destination).await
        }
        ServerState::Starting | ServerState::Stopping if loaded => {
            Err("Espera a que el servidor termine de iniciar o detenerse.".to_owned())
        }
        _ => {
            let destination = destination.to_owned();
            tokio::task::spawn_blocking(move || {
                archive::write(&world_dir, &archive::list_files(&world_dir)?, &destination)
            })
            .await
            .map_err(|e| format!("La copia se interrumpió: {e}"))?
        }
    }
}

pub async fn create(
    supervisor: &Supervisor,
    server_dir: &Path,
    backups_dir: &Path,
    folder: &str,
    reason: Reason,
    created: &str,
) -> Result<BackupInfo, String> {
    let file = file_name(folder, reason, created);
    let destination = backups_dir.join(&file);
    if destination.exists() {
        return Err("Ya hay una copia de este mismo segundo. Espera un momento.".to_owned());
    }
    snapshot(supervisor, server_dir, folder, &destination).await?;
    if reason != Reason::Manual {
        prune(backups_dir);
    }
    list(backups_dir)
        .into_iter()
        .find(|backup| backup.file == file)
        .ok_or_else(|| "La copia no se guardó.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_its_own_file_names() {
        let file = file_name("Mi - mundo", Reason::BeforeDelete, "2026-11-07 18-56-47");
        assert_eq!(file, "Mi - mundo - 2026-11-07 18-56-47 - antes de borrar.mcworld");
        assert_eq!(
            parse_name(&file),
            Some(("Mi - mundo".into(), Reason::BeforeDelete, "2026-11-07 18-56-47".into())),
        );
        assert_eq!(parse_name("foto.png"), None);
        assert_eq!(parse_name("Mundo - 2026 - inventado.mcworld"), None);
    }

    #[test]
    fn parses_the_real_save_query_output() {
        let line = "Bedrock level/db/000008.ldb:598520, Bedrock level/db/CURRENT:16, \
                    Bedrock level/level.dat:3056, Bedrock level/levelname.txt:13\r";
        let files = parse_save_list(line, "Bedrock level").unwrap();
        assert_eq!(files.len(), 4);
        assert_eq!(files[0], (PathBuf::from("db/000008.ldb"), 598520));
        assert_eq!(files[3], (PathBuf::from("levelname.txt"), 13));
    }

    #[test]
    fn keeps_world_names_with_commas_together() {
        let files = parse_save_list("Casa, campo/level.dat:10, Casa, campo/db/CURRENT:16", "Casa, campo");
        assert_eq!(files.unwrap(), [("level.dat".into(), 10), ("db/CURRENT".into(), 16)]);
    }

    #[test]
    fn rejects_paths_outside_the_world() {
        assert!(parse_save_list("Bedrock level/../server.properties:10", "Bedrock level").is_err());
        assert!(parse_save_list("Otro/level.dat:10", "Bedrock level").is_err());
        assert!(parse_save_list("Bedrock level/level.dat:diez", "Bedrock level").is_err());
    }

    #[test]
    fn pruning_keeps_manual_copies_and_the_newest_automatic_ones() {
        let dir = std::env::temp_dir().join(format!("mcbc-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..12 {
            let created = format!("2026-11-07 10-00-{i:02}");
            std::fs::write(dir.join(file_name("M", Reason::BeforeDelete, &created)), "x").unwrap();
        }
        std::fs::write(dir.join(file_name("M", Reason::Manual, "2026-01-01 00-00-00")), "x").unwrap();

        prune(&dir);

        let left = list(&dir);
        assert_eq!(left.iter().filter(|b| b.reason == Reason::Manual).count(), 1);
        let automatic: Vec<_> = left.iter().filter(|b| b.reason != Reason::Manual).collect();
        assert_eq!(automatic.len(), 10);
        assert_eq!(automatic.last().unwrap().created, "2026-11-07 10-00-02");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
