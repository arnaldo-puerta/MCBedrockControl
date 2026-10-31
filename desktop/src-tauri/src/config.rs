//! Persistent settings, stored in the per-user app config directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub server_path: Option<PathBuf>,
    pub server_version: Option<String>,
}

impl Config {
    /// A missing or unreadable file falls back to defaults so the setup
    /// screen can recover from it.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write then rename, so a crash can't leave a half-written config.
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&temp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("mcbc-config-{}", std::process::id()));
        let path = dir.join("nested").join("config.json");
        let config = Config {
            server_path: Some(PathBuf::from(r"C:\Servidores\Bedrock")),
            server_version: Some("1.26.61.1".into()),
        };

        config.save(&path).unwrap();
        assert_eq!(Config::load(&path), config);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let path = std::env::temp_dir().join(format!("mcbc-corrupt-{}.json", std::process::id()));
        std::fs::write(&path, "{ no es json").unwrap();
        assert_eq!(Config::load(&path), Config::default());
        std::fs::remove_file(path).unwrap();
    }
}
