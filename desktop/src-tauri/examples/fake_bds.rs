//! Stand-in for `bedrock_server.exe`, for tests and for trying the UI.
//! Speaks just enough of the BDS console: `stop`, `crash`, `hang`, `flood`,
//! `join <name>` and `leave <name>`.

use std::io::{BufRead, Write};
use std::time::Duration;

fn log(message: &str) {
    println!("[2026-10-31 12:00:00:000 INFO] {message}");
    std::io::stdout().flush().unwrap();
}

fn main() {
    println!("NO LOG FILE! - setting up server logging...");
    log("Version: 1.26.61.1");
    std::thread::sleep(Duration::from_millis(200));
    log("Server started.");

    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        match line.split_once(' ').unwrap_or((&line, "")) {
            ("stop", _) => {
                log("Server stop requested.");
                std::thread::sleep(Duration::from_millis(100));
                log("Quit correctly");
                return;
            }
            ("crash", _) => std::process::exit(3),
            ("hang", _) => loop {
                std::thread::sleep(Duration::from_secs(3600));
            },
            ("join", name) => log(&format!("Player connected: {name}, xuid: 2535")),
            ("leave", name) => log(&format!("Player disconnected: {name}, xuid: 2535, pfid: abc")),
            ("flood", _) => (0..20_000).for_each(|i| log(&format!("linea {i}"))),
            (other, _) => log(&format!("Unknown command: {other}")),
        }
    }
}
