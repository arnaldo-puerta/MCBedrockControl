//! Turns BDS log lines into structured events.
//!
//! Lines look like `[2026-10-31 12:00:00:123 INFO] Server started.`, so both
//! the level and the message sit behind a timestamp.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
    Plain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerEvent {
    Ready,
    Version(String),
    PlayerJoined { name: String, xuid: String },
    PlayerLeft { name: String },
}

fn split_header(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('[')?;
    rest.split_once("] ")
}

pub fn level(line: &str) -> Level {
    let Some((header, _)) = split_header(line) else {
        return Level::Plain;
    };
    match header.rsplit(' ').next() {
        Some("INFO") => Level::Info,
        Some("WARN") => Level::Warn,
        Some("ERROR") => Level::Error,
        _ => Level::Plain,
    }
}

pub fn parse(line: &str) -> Option<ServerEvent> {
    let message = split_header(line).map_or(line, |(_, message)| message).trim();

    if message == "Server started." {
        return Some(ServerEvent::Ready);
    }
    if let Some(version) = message.strip_prefix("Version: ") {
        return Some(ServerEvent::Version(version.trim().to_owned()));
    }
    if let Some(rest) = message.strip_prefix("Player connected: ") {
        let (name, xuid) = rest.split_once(", xuid:")?;
        return Some(ServerEvent::PlayerJoined {
            name: name.to_owned(),
            xuid: xuid.trim().to_owned(),
        });
    }
    if let Some(rest) = message.strip_prefix("Player disconnected: ") {
        let (name, _) = rest.split_once(", xuid:")?;
        return Some(ServerEvent::PlayerLeft { name: name.to_owned() });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_level_behind_timestamp() {
        assert_eq!(level("[2026-10-31 12:00:00:123 INFO] Server started."), Level::Info);
        assert_eq!(level("[2026-10-31 12:00:00:123 WARN] Careful"), Level::Warn);
        assert_eq!(level("[2026-10-31 12:00:00:123 ERROR] Boom"), Level::Error);
        assert_eq!(level("NO LOG FILE! - setting up server logging..."), Level::Plain);
    }

    #[test]
    fn detects_ready_and_version() {
        assert_eq!(parse("[2026-10-31 12:00:00:123 INFO] Server started."), Some(ServerEvent::Ready));
        assert_eq!(
            parse("[2026-10-31 12:00:00:123 INFO] Version: 1.26.61.1"),
            Some(ServerEvent::Version("1.26.61.1".into())),
        );
    }

    #[test]
    fn tracks_players_with_spaces_in_their_names() {
        assert_eq!(
            parse("[2026-10-31 12:00:00:123 INFO] Player connected: Mi Gamertag, xuid: 2535"),
            Some(ServerEvent::PlayerJoined { name: "Mi Gamertag".into(), xuid: "2535".into() }),
        );
        assert_eq!(
            parse("[2026-10-31 12:00:00:123 INFO] Player disconnected: Mi Gamertag, xuid: 2535, pfid: abc"),
            Some(ServerEvent::PlayerLeft { name: "Mi Gamertag".into() }),
        );
    }

    #[test]
    fn ignores_everything_else() {
        assert_eq!(parse("[2026-10-31 12:00:00:123 INFO] Level Name: Mi mundo"), None);
        assert_eq!(parse(""), None);
    }
}
