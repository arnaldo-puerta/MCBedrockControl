//! Stand-in for `bedrock_server.exe`, for tests and for trying the UI.
//! Speaks just enough of the BDS console: `stop`, `crash`, `hang`, `flood`,
//! `join <name>`, `leave <name>` and `save hold|query|resume`, in the real
//! server's format.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn log(message: &str) {
    println!("[2026-10-31 12:00:00:000 INFO] {message}");
    std::io::stdout().flush().unwrap();
}

fn error(message: &str) {
    println!("[2026-10-31 12:00:00:000 ERROR] {message}");
    std::io::stdout().flush().unwrap();
}

fn level_name() -> String {
    std::fs::read_to_string("server.properties")
        .unwrap_or_default()
        .lines()
        .find_map(|line| line.strip_prefix("level-name=").map(str::to_owned))
        .unwrap_or_else(|| "Bedrock level".to_owned())
}

fn files(dir: &Path, base: &Path, out: &mut Vec<(PathBuf, u64)>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, base, out);
        } else {
            let len = entry.metadata().unwrap().len();
            out.push((path.strip_prefix(base).unwrap().to_owned(), len));
        }
    }
}

fn main() {
    println!("NO LOG FILE! - setting up server logging...");
    log("Version: 1.26.61.1");
    std::thread::sleep(Duration::from_millis(200));
    log("Server started.");

    // None: not held. Some(n): held, with n queries left before it is ready.
    let mut hold: Option<u32> = None;

    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        match line.split_once(' ').unwrap_or((&line, "")) {
            ("stop", _) => {
                log("Server stop requested.");
                std::thread::sleep(Duration::from_millis(100));
                println!("Quit correctly");
                return;
            }
            ("crash", _) => std::process::exit(3),
            ("hang", _) => loop {
                std::thread::sleep(Duration::from_secs(3600));
            },
            ("join", name) => log(&format!("Player connected: {name}, xuid: 2535")),
            ("leave", name) => log(&format!("Player disconnected: {name}, xuid: 2535, pfid: abc")),
            ("flood", _) => (0..20_000).for_each(|i| log(&format!("linea {i}"))),
            ("save", "hold") if hold.is_some() => error("The command is already running"),
            ("save", "hold") => {
                hold = Some(1);
                log("Saving...");
            }
            ("save", "query") => match hold {
                None => error("A previous save has not been completed."),
                Some(0) => {
                    let level = level_name();
                    let mut list = Vec::new();
                    files(&Path::new("worlds").join(&level), Path::new("worlds"), &mut list);
                    let list: Vec<String> = list
                        .iter()
                        .map(|(p, len)| format!("{}:{len}", p.to_string_lossy().replace('\\', "/")))
                        .collect();
                    log("Data saved. Files are now ready to be copied.");
                    println!("{}", list.join(", "));
                    std::io::stdout().flush().unwrap();
                    // Data past the reported length must never reach a backup.
                    let level_dat = Path::new("worlds").join(&level).join("level.dat");
                    let mut file = std::fs::OpenOptions::new().append(true).open(level_dat).unwrap();
                    file.write_all(b"-sin-guardar").unwrap();
                }
                Some(n) => hold = Some(n - 1),
            },
            ("save", "resume") if hold.is_none() => error("A previous save has not been completed."),
            ("save", "resume") => {
                hold = None;
                log("Changes to the world are resumed.");
            }
            (other, _) => log(&format!("Unknown command: {other}")),
        }
    }
}
