//! Access to the values of `server.properties` the app uses.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub world: String,
    pub port: String,
    pub max_players: String,
}

pub(crate) fn parse(text: &str) -> HashMap<&str, &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect()
}

pub fn read_info(server_dir: &Path) -> ServerInfo {
    let text = std::fs::read_to_string(server_dir.join("server.properties")).unwrap_or_default();
    let props = parse(&text);
    let get = |key: &str| props.get(key).copied().unwrap_or_default().to_owned();
    ServerInfo {
        name: get("server-name"),
        world: get("level-name"),
        port: get("server-port"),
        max_players: get("max-players"),
    }
}

pub fn read_value(server_dir: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(server_dir.join("server.properties")).ok()?;
    parse(&text).get(key).map(|value| (*value).to_owned())
}

/// Replaces one line and leaves comments and everything else exactly as is.
fn with_value(text: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut result = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let is_key = line.split_once('=').is_some_and(|(name, _)| name.trim() == key);
        if is_key && !found {
            found = true;
            let ending = if line.ends_with("\r\n") { "\r\n" } else { "\n" };
            result.push_str(&format!("{key}={value}{ending}"));
        } else {
            result.push_str(line);
        }
    }
    if !found {
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&format!("{key}={value}\n"));
    }
    result
}

pub fn write_value(server_dir: &Path, key: &str, value: &str) -> Result<(), String> {
    write_values(server_dir, &[(key, value.to_owned())])
}

/// Changes several values in one write, so a crash can't apply only some.
pub fn write_values(server_dir: &Path, values: &[(&str, String)]) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("No se pudo guardar server.properties: {e}");
    let target = server_dir.join("server.properties");
    let mut text = std::fs::read_to_string(&target).map_err(failed)?;
    for (key, value) in values {
        text = with_value(&text, key, value);
    }
    let temp = target.with_extension("properties.tmp");
    std::fs::write(&temp, text).map_err(failed)?;
    std::fs::rename(&temp, &target).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_one_value_and_preserves_the_rest() {
        let text = "# Lista\r\nallow-list=true\r\n# allow-list=comentario\r\nserver-port=19132\r\n";
        assert_eq!(
            with_value(text, "allow-list", "false"),
            "# Lista\r\nallow-list=false\r\n# allow-list=comentario\r\nserver-port=19132\r\n",
        );
    }

    #[test]
    fn appends_a_missing_key() {
        assert_eq!(with_value("a=1", "allow-list", "true"), "a=1\nallow-list=true\n");
    }

    #[test]
    fn skips_comments_and_keeps_values_with_equals() {
        let props = parse("# comment=1\nserver-name=Hola=Mundo\n\n  server-port = 19132 \n");
        assert_eq!(props.get("server-name"), Some(&"Hola=Mundo"));
        assert_eq!(props.get("server-port"), Some(&"19132"));
        assert_eq!(props.len(), 2);
    }

    #[test]
    fn missing_file_gives_empty_info() {
        assert_eq!(read_info(Path::new("no-existe")), ServerInfo::default());
    }
}
