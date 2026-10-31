//! Reads and edits `allowlist.json`.
//!
//! The server fills in `xuid` when a player first joins, so entries are kept
//! as they are and only the one being changed is touched.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const FILE: &str = "allowlist.json";
const MAX_NAME_CHARS: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AllowEntry {
    #[serde(rename = "ignoresPlayerLimit", default)]
    pub ignores_player_limit: bool,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xuid: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn path(server_dir: &Path) -> PathBuf {
    server_dir.join(FILE)
}

/// Names end up inside console commands, so a quote or a line break here
/// could otherwise run a different command.
pub fn clean_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Escribe el Gamertag del jugador.".to_owned());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err("Ese Gamertag es demasiado largo.".to_owned());
    }
    if name.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        return Err("Ese Gamertag tiene caracteres no válidos.".to_owned());
    }
    Ok(name)
}

/// A missing file is an empty list, but an unreadable one is an error:
/// treating it as empty would erase it on the next save.
pub fn read(server_dir: &Path) -> Result<Vec<AllowEntry>, String> {
    let text = match std::fs::read_to_string(path(server_dir)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("No se pudo leer {FILE}: {e}")),
    };
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("{FILE} no es válido: {e}"))
}

fn write(server_dir: &Path, entries: &[AllowEntry]) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("No se pudo guardar {FILE}: {e}");
    let target = path(server_dir);
    let temp = target.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    std::fs::write(&temp, json + "\n").map_err(failed)?;
    std::fs::rename(&temp, &target).map_err(failed)
}

pub fn add(server_dir: &Path, name: &str) -> Result<(), String> {
    let name = clean_name(name)?;
    let mut entries = read(server_dir)?;
    if entries.iter().any(|entry| entry.name.eq_ignore_ascii_case(name)) {
        return Ok(());
    }
    entries.push(AllowEntry {
        ignores_player_limit: false,
        name: name.to_owned(),
        xuid: None,
        extra: Map::new(),
    });
    write(server_dir, &entries)
}

pub fn remove(server_dir: &Path, name: &str) -> Result<(), String> {
    let mut entries = read(server_dir)?;
    entries.retain(|entry| !entry.name.eq_ignore_ascii_case(name.trim()));
    write(server_dir, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("mcbc-allow-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const REAL_FILE: &str =
        r#"[{"ignoresPlayerLimit":false,"name":"AxelDoor5960","xuid":"2535471276377335"}]"#;

    #[test]
    fn adding_keeps_the_xuid_of_existing_players() {
        let dir = TempDir::new("keep");
        std::fs::write(dir.0.join(FILE), REAL_FILE).unwrap();

        add(&dir.0, "Mi Amigo").unwrap();

        let entries = read(&dir.0).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].xuid.as_deref(), Some("2535471276377335"));
        assert_eq!(entries[1].name, "Mi Amigo");
        assert_eq!(entries[1].xuid, None);
    }

    #[test]
    fn keeps_fields_this_app_does_not_know_about() {
        let dir = TempDir::new("extra");
        std::fs::write(dir.0.join(FILE), r#"[{"name":"Alex","futuro":7}]"#).unwrap();

        add(&dir.0, "Steve").unwrap();

        assert!(std::fs::read_to_string(dir.0.join(FILE)).unwrap().contains("\"futuro\": 7"));
    }

    #[test]
    fn adding_twice_ignores_case() {
        let dir = TempDir::new("dup");
        add(&dir.0, "Steve").unwrap();
        add(&dir.0, " steve ").unwrap();
        assert_eq!(read(&dir.0).unwrap().len(), 1);
    }

    #[test]
    fn removing_only_touches_that_player() {
        let dir = TempDir::new("remove");
        std::fs::write(dir.0.join(FILE), REAL_FILE).unwrap();
        add(&dir.0, "Steve").unwrap();

        remove(&dir.0, "steve").unwrap();

        let entries = read(&dir.0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "AxelDoor5960");
    }

    #[test]
    fn a_corrupt_file_is_never_overwritten() {
        let dir = TempDir::new("corrupt");
        std::fs::write(dir.0.join(FILE), "{ roto").unwrap();

        assert!(add(&dir.0, "Steve").is_err());
        assert_eq!(std::fs::read_to_string(dir.0.join(FILE)).unwrap(), "{ roto");
    }

    #[test]
    fn missing_file_is_an_empty_list() {
        let dir = TempDir::new("missing");
        assert!(read(&dir.0).unwrap().is_empty());
    }

    #[test]
    fn rejects_names_that_could_escape_a_command() {
        for name in ["", "   ", "a\" && stop", "linea\nstop", "barra\\", &"x".repeat(33)] {
            assert!(clean_name(name).is_err(), "{name:?}");
        }
        assert_eq!(clean_name("  Mi Amigo#1234 "), Ok("Mi Amigo#1234"));
    }
}
