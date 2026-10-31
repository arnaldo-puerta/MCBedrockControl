//! Uses the real network and the real server, so it only runs on demand:
//! `cargo test --test real_install -- --ignored --nocapture`

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use mcbc_lib::bds::EXECUTABLE;
use mcbc_lib::bds::installer::{self, Phase, Progress};
use mcbc_lib::bds::supervisor::{Event, ServerState, Sink, Supervisor};

struct Discard;

impl Sink for Discard {
    fn emit(&self, _event: Event) {}
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir() -> TempDir {
    let dir = std::env::temp_dir().join(format!("mcbc-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads the real server"]
async fn cancelling_leaves_nothing_behind() {
    let dir = temp_dir();
    let target = dir.0.join("servidor");
    let cancel = Arc::new(AtomicBool::new(false));

    let flag = cancel.clone();
    let error = installer::install(&target, cancel, move |progress: Progress| {
        if progress.done > 1024 * 1024 {
            flag.store(true, Ordering::SeqCst);
        }
    })
    .await
    .unwrap_err();

    assert_eq!(error, "Instalación cancelada.");
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads and runs the real server"]
async fn installs_and_runs_the_official_server() {
    let dir = temp_dir();
    let target = dir.0.join("servidor");
    let phases = Arc::new(std::sync::Mutex::new(Vec::new()));

    let seen = phases.clone();
    let version = installer::install(&target, Arc::new(AtomicBool::new(false)), move |progress| {
        let mut seen = seen.lock().unwrap();
        if seen.last() != Some(&progress.phase) {
            seen.push(progress.phase);
        }
    })
    .await
    .unwrap();

    println!("instalada la versión {version}");
    assert_eq!(*phases.lock().unwrap(), [Phase::Download, Phase::Extract]);
    assert!(target.join(EXECUTABLE).is_file());
    assert!(target.join("server.properties").is_file());
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1, "staging folder left behind");

    let supervisor = Supervisor::new(Arc::new(Discard));
    supervisor.start(&target).await.unwrap();
    for _ in 0..600 {
        if supervisor.state() != ServerState::Starting {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let history = supervisor.snapshot().history;
    for line in history.iter().rev().take(12).rev() {
        println!("{}", line.text);
    }
    assert_eq!(supervisor.state(), ServerState::Running);

    supervisor.stop().await;
    assert_eq!(supervisor.state(), ServerState::Stopped);
    assert!(supervisor.snapshot().history.iter().any(|l| l.text.contains("Quit correctly")));
}
