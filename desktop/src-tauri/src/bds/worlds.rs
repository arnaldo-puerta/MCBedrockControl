//! The worlds under `worlds/` and which one `server.properties` loads.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::UNIX_EPOCH;

use serde::Serialize;

use super::{archive, properties};

const LEVEL_KEY: &str = "level-name";
const MAX_FOLDER_CHARS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldInfo {
    pub folder: String,
    pub name: String,
    pub size_bytes: u64,
    pub modified_secs: u64,
    pub active: bool,
}

pub fn worlds_dir(server_dir: &Path) -> PathBuf {
    server_dir.join("worlds")
}

pub fn active(server_dir: &Path) -> Option<String> {
    properties::read_value(server_dir, LEVEL_KEY).filter(|name| !name.is_empty())
}

/// Folder names come from the UI, so make sure they can't point elsewhere.
pub fn plain_name(name: &str) -> Result<&str, String> {
    let is_plain = !name.is_empty()
        && name != "."
        && name != ".."
        && Path::new(name).file_name().and_then(|n| n.to_str()) == Some(name);
    if is_plain { Ok(name) } else { Err(format!("Nombre no válido: {name}")) }
}

pub fn sanitize_folder(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control() && !r#"<>:"/\|?*"#.contains(*c))
        .take(MAX_FOLDER_CHARS)
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim();
    if cleaned.is_empty() { "Mundo".to_owned() } else { cleaned.to_owned() }
}

pub fn unique_folder(parent: &Path, base: &str) -> String {
    let base = sanitize_folder(base);
    if !parent.join(&base).exists() {
        return base;
    }
    (2..).map(|n| format!("{base} ({n})")).find(|name| !parent.join(name).exists()).expect("unbounded")
}

pub fn display_name(world_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(world_dir.join("levelname.txt")).ok()?;
    let name = text.trim_start_matches('\u{feff}').trim();
    (!name.is_empty()).then(|| name.to_owned())
}

pub fn list(server_dir: &Path) -> Result<Vec<WorldInfo>, String> {
    let dir = worlds_dir(server_dir);
    let active = active(server_dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("No se pudo leer la carpeta de mundos: {e}")),
    };

    let mut worlds = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.join("level.dat").is_file() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().into_owned();
        let files = archive::list_files(&path)?;
        let modified_secs = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        worlds.push(WorldInfo {
            name: display_name(&path).unwrap_or_else(|| folder.clone()),
            active: active.as_deref() == Some(folder.as_str()),
            size_bytes: files.iter().map(|(_, size)| size).sum(),
            modified_secs,
            folder,
        });
    }
    worlds.sort_by(|a, b| b.active.cmp(&a.active).then_with(|| a.name.cmp(&b.name)));
    Ok(worlds)
}

/// The folder holding `level.dat`: the archive root, or one level down for
/// archives that wrap the world in a folder.
pub fn find_world_root(unpacked: &Path) -> Option<PathBuf> {
    if unpacked.join("level.dat").is_file() {
        return Some(unpacked.to_owned());
    }
    let mut candidates = std::fs::read_dir(unpacked)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("level.dat").is_file());
    let only = candidates.next()?;
    candidates.next().is_none().then_some(only)
}

fn staging_dir(server_dir: &Path, label: &str) -> PathBuf {
    server_dir.join(format!(".mcbc-{label}-{}", std::process::id()))
}

/// Unpacks a `.mcworld` into `worlds/<folder>`, replacing that folder if
/// `replace` is set. Returns the folder name used.
pub fn unpack(
    server_dir: &Path,
    archive_path: &Path,
    folder: Option<&str>,
    replace: bool,
) -> Result<String, String> {
    let staging = staging_dir(server_dir, "import");
    let _ = std::fs::remove_dir_all(&staging);
    let result = (|| {
        archive::extract(archive_path, &staging, &AtomicBool::new(false), |_, _| {})?;
        let root = find_world_root(&staging)
            .ok_or("Ese archivo no contiene un mundo de Minecraft (falta level.dat).")?;

        let worlds = worlds_dir(server_dir);
        std::fs::create_dir_all(&worlds).map_err(|e| e.to_string())?;
        let folder = match folder {
            Some(folder) if replace => plain_name(folder)?.to_owned(),
            Some(folder) => unique_folder(&worlds, folder),
            None => {
                let fallback = archive_path.file_stem().unwrap_or_default().to_string_lossy();
                unique_folder(&worlds, &display_name(&root).unwrap_or_else(|| fallback.into_owned()))
            }
        };

        let target = worlds.join(&folder);
        if replace && target.exists() {
            std::fs::remove_dir_all(&target)
                .map_err(|e| format!("No se pudo reemplazar el mundo {folder}: {e}"))?;
        }
        std::fs::rename(&root, &target).map_err(|e| format!("No se pudo guardar el mundo: {e}"))?;
        Ok(folder)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

pub fn set_active(server_dir: &Path, folder: &str) -> Result<(), String> {
    let folder = plain_name(folder)?;
    if !worlds_dir(server_dir).join(folder).join("level.dat").is_file() {
        return Err(format!("No existe el mundo {folder}."));
    }
    properties::write_value(server_dir, LEVEL_KEY, folder)
}

pub fn delete(server_dir: &Path, folder: &str) -> Result<(), String> {
    let folder = plain_name(folder)?;
    if active(server_dir).as_deref() == Some(folder) {
        return Err("No puedes borrar el mundo activo. Activa otro primero.".to_owned());
    }
    std::fs::remove_dir_all(worlds_dir(server_dir).join(folder))
        .map_err(|e| format!("No se pudo borrar el mundo: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub struct TempServer(pub PathBuf);

    impl TempServer {
        pub fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("mcbc-worlds-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("server.properties"), "level-name=Bedrock level\n").unwrap();
            Self(dir)
        }

        pub fn world(&self, folder: &str, name: &str) -> PathBuf {
            let path = worlds_dir(&self.0).join(folder);
            std::fs::create_dir_all(path.join("db")).unwrap();
            std::fs::write(path.join("level.dat"), "nivel").unwrap();
            std::fs::write(path.join("levelname.txt"), name).unwrap();
            std::fs::write(path.join("db").join("CURRENT"), "MANIFEST-1").unwrap();
            path
        }
    }

    impl Drop for TempServer {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn mcworld(server: &TempServer, world: &Path, name: &str) -> PathBuf {
        let path = server.0.join(name);
        archive::write(world, &archive::list_files(world).unwrap(), &path).unwrap();
        path
    }

    #[test]
    fn lists_worlds_with_their_names_and_the_active_one_first() {
        let server = TempServer::new("list");
        server.world("Zeta", "Otro mundo");
        server.world("Bedrock level", "Mi mundo");
        std::fs::create_dir_all(worlds_dir(&server.0).join("no-es-un-mundo")).unwrap();

        let worlds = list(&server.0).unwrap();
        assert_eq!(worlds.len(), 2);
        assert_eq!(worlds[0].folder, "Bedrock level");
        assert_eq!(worlds[0].name, "Mi mundo");
        assert!(worlds[0].active);
        assert_eq!(worlds[0].size_bytes, 5 + 8 + 10);
        assert!(!worlds[1].active);
    }

    #[test]
    fn importing_never_overwrites_an_existing_world() {
        let server = TempServer::new("import");
        let existing = server.world("Mi mundo", "Mi mundo");
        let file = mcworld(&server, &existing, "copia.mcworld");

        let folder = unpack(&server.0, &file, None, false).unwrap();
        assert_eq!(folder, "Mi mundo (2)");
        assert!(worlds_dir(&server.0).join("Mi mundo (2)").join("level.dat").is_file());
        assert!(existing.join("level.dat").is_file());
        assert!(!staging_dir(&server.0, "import").exists());
    }

    #[test]
    fn imports_worlds_wrapped_in_a_folder() {
        let server = TempServer::new("wrapped");
        let source = server.0.join("fuera");
        let inner = source.join("Envuelto");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("level.dat"), "nivel").unwrap();
        let file = mcworld(&server, &source, "envuelto.mcworld");

        assert_eq!(unpack(&server.0, &file, None, false).unwrap(), "envuelto");
    }

    #[test]
    fn rejects_archives_without_a_world() {
        let server = TempServer::new("noworld");
        let source = server.0.join("vacio");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("readme.txt"), "hola").unwrap();
        let file = mcworld(&server, &source, "nada.mcworld");

        assert!(unpack(&server.0, &file, None, false).unwrap_err().contains("level.dat"));
        assert!(worlds_dir(&server.0).read_dir().map_or(true, |mut d| d.next().is_none()));
    }

    #[test]
    fn switches_the_active_world_and_protects_it_from_deletion() {
        let server = TempServer::new("active");
        server.world("Bedrock level", "A");
        server.world("Otro", "B");

        assert!(delete(&server.0, "Bedrock level").unwrap_err().contains("activo"));
        set_active(&server.0, "Otro").unwrap();
        assert_eq!(active(&server.0).as_deref(), Some("Otro"));
        delete(&server.0, "Bedrock level").unwrap();
        assert!(!worlds_dir(&server.0).join("Bedrock level").exists());
        assert!(set_active(&server.0, "No existe").is_err());
    }

    #[test]
    fn folder_names_cannot_escape_the_worlds_folder() {
        for name in ["", ".", "..", "../server", "a/b", "a\\b", "C:\\x"] {
            assert!(plain_name(name).is_err(), "{name:?}");
        }
        assert_eq!(sanitize_folder(" Mi: mundo?. "), "Mi mundo");
        assert_eq!(sanitize_folder("<>"), "Mundo");
    }
}
