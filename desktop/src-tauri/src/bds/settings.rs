//! The everyday options from `server.properties`, with friendly names and
//! the same limits the file's own comments give.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{level_dat, properties, worlds};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSettings {
    pub server_name: String,
    /// None when the server has no world yet.
    pub world_name: Option<String>,
    pub gamemode: String,
    pub force_gamemode: bool,
    pub difficulty: String,
    pub allow_cheats: bool,
    pub default_permission: String,
    pub max_players: u32,
    pub idle_timeout_minutes: u32,
    pub view_distance: u32,
    pub tick_distance: u32,
    pub online_mode: bool,
    pub port: u32,
}

const GAMEMODES: [&str; 3] = ["survival", "creative", "adventure"];
const DIFFICULTIES: [&str; 4] = ["peaceful", "easy", "normal", "hard"];
const PERMISSIONS: [&str; 3] = ["visitor", "member", "operator"];
const MAX_NAME_CHARS: usize = 64;

fn world_dir(server_dir: &Path) -> Option<std::path::PathBuf> {
    let dir = worlds::worlds_dir(server_dir).join(worlds::active(server_dir)?);
    dir.join("level.dat").is_file().then_some(dir)
}

pub fn read(server_dir: &Path) -> ServerSettings {
    let text = std::fs::read_to_string(server_dir.join("server.properties")).unwrap_or_default();
    let props = properties::parse(&text);
    let text_of = |key: &str, default: &str| props.get(key).copied().unwrap_or(default).to_owned();
    let flag = |key: &str, default: bool| props.get(key).map_or(default, |v| *v == "true");
    let number = |key: &str, default: u32| props.get(key).and_then(|v| v.parse().ok()).unwrap_or(default);

    let world_name = world_dir(server_dir).map(|dir| {
        std::fs::read(dir.join("level.dat"))
            .ok()
            .and_then(|data| level_dat::read_name(&data).ok())
            .or_else(|| worlds::display_name(&dir))
            .unwrap_or_default()
    });

    ServerSettings {
        server_name: text_of("server-name", "Dedicated Server"),
        world_name,
        gamemode: text_of("gamemode", "survival"),
        force_gamemode: flag("force-gamemode", false),
        difficulty: text_of("difficulty", "easy"),
        allow_cheats: flag("allow-cheats", false),
        default_permission: text_of("default-player-permission-level", "member"),
        max_players: number("max-players", 10),
        idle_timeout_minutes: number("player-idle-timeout", 30),
        view_distance: number("view-distance", 32),
        tick_distance: number("tick-distance", 4),
        online_mode: flag("online-mode", true),
        port: number("server-port", 19132),
    }
}

fn clean_name<'a>(name: &'a str, what: &str) -> Result<&'a str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("Escribe un nombre para el {what}."));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("El nombre del {what} es demasiado largo."));
    }
    if name.contains(';') || name.chars().any(char::is_control) {
        return Err(format!("El nombre del {what} no puede tener «;» ni saltos de línea."));
    }
    Ok(name)
}

pub fn validate(settings: &ServerSettings) -> Result<(), String> {
    clean_name(&settings.server_name, "servidor")?;
    if let Some(world) = &settings.world_name {
        clean_name(world, "mundo")?;
    }
    let one_of = |value: &str, allowed: &[&str], what: &str| {
        if allowed.contains(&value) { Ok(()) } else { Err(format!("{what} no válido: {value}")) }
    };
    one_of(&settings.gamemode, &GAMEMODES, "Modo de juego")?;
    one_of(&settings.difficulty, &DIFFICULTIES, "Dificultad")?;
    one_of(&settings.default_permission, &PERMISSIONS, "Permiso")?;
    if settings.max_players == 0 {
        return Err("Tiene que poder entrar al menos 1 jugador.".to_owned());
    }
    if settings.view_distance < 5 {
        return Err("La distancia de visión tiene que ser 5 o más.".to_owned());
    }
    if !(4..=12).contains(&settings.tick_distance) {
        return Err("La distancia de simulación tiene que estar entre 4 y 12.".to_owned());
    }
    if !(1..=65535).contains(&settings.port) {
        return Err("El puerto tiene que estar entre 1 y 65535.".to_owned());
    }
    Ok(())
}

/// Writes every option in one go. Renaming the world is separate, since it
/// needs the server stopped and a backup first.
pub fn write(server_dir: &Path, settings: &ServerSettings) -> Result<(), String> {
    validate(settings)?;
    let bool_text = |value: bool| if value { "true" } else { "false" }.to_owned();
    let values = [
        ("server-name", settings.server_name.trim().to_owned()),
        ("gamemode", settings.gamemode.clone()),
        ("force-gamemode", bool_text(settings.force_gamemode)),
        ("difficulty", settings.difficulty.clone()),
        ("allow-cheats", bool_text(settings.allow_cheats)),
        ("default-player-permission-level", settings.default_permission.clone()),
        ("max-players", settings.max_players.to_string()),
        ("player-idle-timeout", settings.idle_timeout_minutes.to_string()),
        ("view-distance", settings.view_distance.to_string()),
        ("tick-distance", settings.tick_distance.to_string()),
        ("online-mode", bool_text(settings.online_mode)),
        ("server-port", settings.port.to_string()),
    ];
    properties::write_values(server_dir, &values)
}

pub fn rename_world(server_dir: &Path, name: &str) -> Result<(), String> {
    let name = clean_name(name, "mundo")?;
    let dir = world_dir(server_dir).ok_or("El servidor no tiene un mundo todavía.")?;
    level_dat::rename_world(&dir, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_server(name: &str, props: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mcbc-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("server.properties"), props).unwrap();
        dir
    }

    const PROPS: &str = "server-name=Dedicated Server\n# Used as the server name\ngamemode=survival\n\
                         difficulty=easy\nallow-cheats=false\nmax-players=10\nserver-port=19132\n\
                         tick-distance=4\nview-distance=32\nlevel-name=Bedrock level\n";

    #[test]
    fn reads_and_writes_while_keeping_comments_and_other_keys() {
        let dir = temp_server("roundtrip", PROPS);
        let mut settings = read(&dir);
        assert_eq!(settings.server_name, "Dedicated Server");
        assert_eq!(settings.world_name, None);
        assert!(settings.online_mode, "missing keys use the server's defaults");

        settings.server_name = "  Servidor de Arnaldo ".into();
        settings.gamemode = "creative".into();
        settings.difficulty = "hard".into();
        settings.max_players = 5;
        write(&dir, &settings).unwrap();

        let text = std::fs::read_to_string(dir.join("server.properties")).unwrap();
        assert!(text.contains("server-name=Servidor de Arnaldo\n# Used as the server name\n"));
        assert!(text.contains("level-name=Bedrock level"));
        let again = read(&dir);
        assert_eq!((again.gamemode.as_str(), again.difficulty.as_str(), again.max_players), ("creative", "hard", 5));
        assert_eq!(again.server_name, "Servidor de Arnaldo");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_values_the_server_would_not_accept() {
        let dir = temp_server("invalid", PROPS);
        let good = read(&dir);
        let cases: [(&str, fn(&mut ServerSettings)); 8] = [
            ("vacío", |s| s.server_name = "   ".into()),
            ("punto y coma", |s| s.server_name = "a;b".into()),
            ("modo", |s| s.gamemode = "spectator".into()),
            ("dificultad", |s| s.difficulty = "insane".into()),
            ("jugadores", |s| s.max_players = 0),
            ("visión", |s| s.view_distance = 4),
            ("simulación", |s| s.tick_distance = 13),
            ("puerto", |s| s.port = 70000),
        ];
        for (what, change) in cases {
            let mut settings = good.clone();
            change(&mut settings);
            assert!(write(&dir, &settings).is_err(), "{what}");
        }
        assert_eq!(std::fs::read_to_string(dir.join("server.properties")).unwrap(), PROPS);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
