//! Updates the server in place while keeping worlds, settings and packs.
//!
//! The new version is unpacked beside the current one, the player's files
//! are carried over, and only then are the folders swapped. Worlds are moved
//! rather than copied, and moved back if anything after that fails.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use super::installer::{self, Progress};
use super::properties;

/// Files the player edits; the new release's defaults would undo their work.
const KEEP_FILES: [&str; 4] = ["allowlist.json", "permissions.json", "packetlimitconfig.json", "eula.txt"];
/// Folders that are entirely the player's.
const KEEP_DIRS: [&str; 5] = [
    "config",
    "development_behavior_packs",
    "development_resource_packs",
    "development_skin_packs",
    "premium_cache",
];
/// Folders shipped with built-in content where the player may add their own.
const MERGE_DIRS: [&str; 3] = ["resource_packs", "behavior_packs", "world_templates"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub current: Option<String>,
    pub latest: String,
    pub available: bool,
}

fn version_parts(version: &str) -> Vec<u32> {
    version.trim().split('.').map(|part| part.parse().unwrap_or(0)).collect()
}

/// Compares dotted versions numerically, so 1.26.100 is newer than 1.26.61.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    version_parts(candidate) > version_parts(current)
}

pub async fn check(current: Option<String>) -> Result<UpdateCheck, String> {
    let release = installer::latest_release(&installer::client()?).await?;
    let available = current.as_deref().is_none_or(|current| is_newer(&release.version, current));
    Ok(UpdateCheck { current, latest: release.version, available })
}

/// Keeps every value the player set and adds keys new to this release with
/// Mojang's defaults, so new options show up without losing old choices.
pub fn merge_properties(players: &str, release_defaults: &str) -> String {
    let theirs = properties::parse(players);
    let mut merged = players.to_owned();
    let mut added = false;
    for line in release_defaults.lines() {
        let Some((key, _)) = line.split_once('=') else { continue };
        let key = key.trim();
        if line.trim_start().starts_with('#') || theirs.contains_key(key) {
            continue;
        }
        if !added {
            if !merged.is_empty() && !merged.ends_with('\n') {
                merged.push('\n');
            }
            added = true;
        }
        merged.push_str(line.trim_end());
        merged.push('\n');
    }
    merged
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Copies the player's files from the running install into the new one.
/// Worlds are handled separately because they are moved, not copied.
pub fn carry_over(old: &Path, new: &Path) -> Result<(), String> {
    let failed = |what: &str, e: std::io::Error| format!("No se pudo conservar {what}: {e}");

    let props = old.join("server.properties");
    if props.is_file() {
        let theirs = std::fs::read_to_string(&props).map_err(|e| failed("server.properties", e))?;
        let defaults = std::fs::read_to_string(new.join("server.properties")).unwrap_or_default();
        std::fs::write(new.join("server.properties"), merge_properties(&theirs, &defaults))
            .map_err(|e| failed("server.properties", e))?;
    }
    for file in KEEP_FILES {
        if old.join(file).is_file() {
            std::fs::copy(old.join(file), new.join(file)).map_err(|e| failed(file, e))?;
        }
    }
    for dir in KEEP_DIRS {
        if old.join(dir).is_dir() {
            let _ = std::fs::remove_dir_all(new.join(dir));
            copy_dir(&old.join(dir), &new.join(dir)).map_err(|e| failed(dir, e))?;
        }
    }
    // Built-in packs come fresh from the release; anything the release
    // doesn't ship is the player's and comes along.
    for dir in MERGE_DIRS {
        let Ok(entries) = std::fs::read_dir(old.join(dir)) else { continue };
        for entry in entries.flatten() {
            let target = new.join(dir).join(entry.file_name());
            if target.exists() {
                continue;
            }
            let copied = if entry.path().is_dir() {
                copy_dir(&entry.path(), &target)
            } else {
                std::fs::copy(entry.path(), &target).map(drop)
            };
            copied.map_err(|e| failed(&entry.file_name().to_string_lossy(), e))?;
        }
    }
    Ok(())
}

fn sibling(server_dir: &Path, suffix: &str) -> PathBuf {
    let mut name = server_dir.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    server_dir.with_file_name(name)
}

/// Moves worlds into the new install and swaps the folders. On failure the
/// worlds go back and the current install is left as it was.
pub fn swap_in(server_dir: &Path, new: &Path) -> Result<(), String> {
    let worlds = server_dir.join("worlds");
    let moved_worlds = worlds.is_dir();
    if moved_worlds {
        std::fs::rename(&worlds, new.join("worlds"))
            .map_err(|e| format!("No se pudieron mover los mundos: {e}. No se cambió nada."))?;
    }

    let previous = sibling(server_dir, ".anterior");
    let _ = std::fs::remove_dir_all(&previous);
    let swapped = std::fs::rename(server_dir, &previous).and_then(|()| {
        std::fs::rename(new, server_dir).inspect_err(|_| {
            let _ = std::fs::rename(&previous, server_dir);
        })
    });

    if let Err(e) = swapped {
        if moved_worlds {
            let _ = std::fs::rename(new.join("worlds"), &worlds);
        }
        return Err(format!(
            "No se pudo cambiar la versión: {e}. ¿Hay algún archivo del servidor abierto? No se cambió nada."
        ));
    }
    let _ = std::fs::remove_dir_all(&previous);
    Ok(())
}

/// Downloads the latest release and installs it over `server_dir`.
/// Returns the new version.
pub async fn update(
    server_dir: &Path,
    cancel: Arc<AtomicBool>,
    report: impl Fn(Progress) + Send + Sync + 'static,
) -> Result<String, String> {
    let staging = sibling(server_dir, ".actualizando");
    let _ = tokio::fs::remove_dir_all(&staging).await;
    tokio::fs::create_dir_all(&staging)
        .await
        .map_err(|e| format!("No se puede escribir junto al servidor: {e}"))?;

    let result = async {
        let client = installer::client()?;
        let release = installer::latest_release(&client).await?;
        let archive_path = staging.join("server.zip");
        installer::download(&client, release.url, &archive_path, &cancel, &report).await?;

        let new = staging.join("server");
        let server_dir = server_dir.to_owned();
        tokio::task::spawn_blocking(move || {
            installer::extract(&archive_path, &new, &cancel, report)?;
            carry_over(&server_dir, &new)?;
            swap_in(&server_dir, &new)
        })
        .await
        .map_err(|e| format!("La actualización se interrumpió: {e}"))??;
        Ok(release.version)
    }
    .await;

    let _ = tokio::fs::remove_dir_all(&staging).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("mcbc-update-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self, path: &str, text: &str) {
            let path = self.0.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }

        fn read(&self, path: &str) -> String {
            std::fs::read_to_string(self.0.join(path)).unwrap()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// An old install with the player's changes, and a fresh release beside it.
    fn installs(name: &str) -> (TempDir, PathBuf, PathBuf) {
        let root = TempDir::new(name);
        root.file("Servidor/bedrock_server.exe", "viejo");
        root.file("Servidor/server.properties", "# mio\nserver-name=Mi servidor\nallow-list=true\n");
        root.file("Servidor/allowlist.json", r#"[{"name":"AxelDoor5960"}]"#);
        root.file("Servidor/worlds/Mi mundo/level.dat", "nivel");
        root.file("Servidor/resource_packs/vanilla/manifest.json", "vanilla viejo");
        root.file("Servidor/resource_packs/Mis texturas/manifest.json", "mio");
        root.file("Servidor/behavior_packs/Mi addon/manifest.json", "mio");
        root.file("Servidor/config/default/permissions.json", "mis permisos");

        root.file("nuevo/bedrock_server.exe", "nuevo");
        root.file(
            "nuevo/server.properties",
            "server-name=Dedicated Server\nallow-list=false\n# nueva opción\nopcion-nueva=1\n",
        );
        root.file("nuevo/allowlist.json", "[]");
        root.file("nuevo/resource_packs/vanilla/manifest.json", "vanilla nuevo");
        root.file("nuevo/config/default/permissions.json", "[]");
        let (old, new) = (root.0.join("Servidor"), root.0.join("nuevo"));
        (root, old, new)
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("1.26.70.1", "1.26.61.1"));
        assert!(is_newer("1.26.100.1", "1.26.61.1"));
        assert!(is_newer("1.27.0.1", "1.26.99.9"));
        assert!(!is_newer("1.26.61.1", "1.26.61.1"));
        assert!(!is_newer("1.26.60.5", "1.26.61.1"));
    }

    #[test]
    fn keeps_player_values_and_adds_new_options() {
        let merged = merge_properties(
            "server-name=Mi servidor\nallow-list=true",
            "server-name=Dedicated Server\n# comentario\nallow-list=false\nopcion-nueva=1\n",
        );
        assert_eq!(merged, "server-name=Mi servidor\nallow-list=true\nopcion-nueva=1\n");
    }

    #[test]
    fn an_update_keeps_everything_the_player_made() {
        let (root, old, new) = installs("full");

        carry_over(&old, &new).unwrap();
        swap_in(&old, &new).unwrap();

        assert_eq!(root.read("Servidor/bedrock_server.exe"), "nuevo");
        let props = root.read("Servidor/server.properties");
        assert!(props.contains("server-name=Mi servidor") && props.contains("allow-list=true"));
        assert!(props.contains("opcion-nueva=1"));
        assert!(root.read("Servidor/allowlist.json").contains("AxelDoor5960"));
        assert_eq!(root.read("Servidor/worlds/Mi mundo/level.dat"), "nivel");
        assert_eq!(root.read("Servidor/resource_packs/vanilla/manifest.json"), "vanilla nuevo");
        assert_eq!(root.read("Servidor/resource_packs/Mis texturas/manifest.json"), "mio");
        assert_eq!(root.read("Servidor/behavior_packs/Mi addon/manifest.json"), "mio");
        assert_eq!(root.read("Servidor/config/default/permissions.json"), "mis permisos");
        assert!(!new.exists());
        assert!(!sibling(&old, ".anterior").exists());
    }

    #[test]
    fn a_failed_swap_puts_the_worlds_back() {
        let (root, old, new) = installs("rollback");
        carry_over(&old, &new).unwrap();

        // Holding a file open in the current install blocks renaming it.
        let _lock = std::fs::File::open(old.join("server.properties")).unwrap();
        let blocked = swap_in(&old, &new);

        if blocked.is_err() {
            assert_eq!(root.read("Servidor/worlds/Mi mundo/level.dat"), "nivel");
            assert_eq!(root.read("Servidor/bedrock_server.exe"), "viejo");
            assert!(!new.join("worlds").exists());
        } else {
            // Some filesystems allow the rename; then it simply succeeded.
            assert_eq!(root.read("Servidor/worlds/Mi mundo/level.dat"), "nivel");
        }
    }
}
