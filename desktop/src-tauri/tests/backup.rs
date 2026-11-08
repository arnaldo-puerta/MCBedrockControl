//! Live backups against the process built from `examples/fake_bds.rs`,
//! which answers `save hold|query|resume` in the real server's format.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use mcbc_lib::bds::EXECUTABLE;
use mcbc_lib::bds::archive;
use mcbc_lib::bds::backup::{self, Reason};
use mcbc_lib::bds::supervisor::{Event, ServerState, Sink, Supervisor};

struct Discard;

impl Sink for Discard {
    fn emit(&self, _event: Event) {}
}

struct Server(PathBuf);

impl Server {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mcbc-backup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let world = dir.join("worlds").join("Mi mundo");
        std::fs::create_dir_all(world.join("db")).unwrap();
        std::fs::write(world.join("level.dat"), "nivel").unwrap();
        std::fs::write(world.join("db").join("CURRENT"), "MANIFEST-1").unwrap();
        std::fs::write(dir.join("server.properties"), "level-name=Mi mundo\n").unwrap();

        let debug = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_owned();
        std::fs::copy(debug.join("examples").join("fake_bds.exe"), dir.join(EXECUTABLE)).unwrap();
        Self(dir)
    }

    fn backups(&self) -> PathBuf {
        self.0.join("copias")
    }

    fn unpack(&self, file: &str) -> PathBuf {
        let out = self.0.join(format!("revisar-{file}"));
        archive::extract(&self.backups().join(file), &out, &AtomicBool::new(false), |_, _| {}).unwrap();
        out
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn running(server: &Server) -> Supervisor {
    let supervisor = Supervisor::new(Arc::new(Discard));
    supervisor.start(&server.0).await.unwrap();
    for _ in 0..500 {
        if supervisor.state() == ServerState::Running {
            return supervisor;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("server never became ready");
}

#[tokio::test(flavor = "multi_thread")]
async fn backs_up_a_stopped_world() {
    let server = Server::new("stopped");
    let supervisor = Supervisor::new(Arc::new(Discard));

    let info = backup::create(&supervisor, &server.0, &server.backups(), "Mi mundo", Reason::Manual, "2026-11-07 10-00-00")
        .await
        .unwrap();

    assert_eq!(info.world, "Mi mundo");
    let out = server.unpack(&info.file);
    assert_eq!(std::fs::read_to_string(out.join("level.dat")).unwrap(), "nivel");
    assert!(out.join("db").join("CURRENT").is_file());
}

#[tokio::test(flavor = "multi_thread")]
async fn live_backup_copies_only_what_the_server_reported_and_resumes() {
    let server = Server::new("live");
    let supervisor = running(&server).await;
    let mut lines = supervisor.watch_lines();

    let info = backup::create(&supervisor, &server.0, &server.backups(), "Mi mundo", Reason::Manual, "2026-11-07 10-00-00")
        .await
        .unwrap();

    // The server has to have resumed by the time the backup reports back,
    // or the world would stay frozen.
    let mut seen = Vec::new();
    while let Ok(line) = lines.try_recv() {
        seen.push(line);
    }
    assert!(seen.iter().any(|l| l.contains("Changes to the world are resumed.")), "{seen:?}");
    // Housekeeping commands must not clutter the console.
    let history = supervisor.snapshot().history;
    assert!(!history.iter().any(|l| l.text.starts_with("> save")));

    // The fake appends to level.dat right after reporting its length; only
    // the reported bytes may be copied.
    let world = server.0.join("worlds").join("Mi mundo");
    assert_eq!(std::fs::read_to_string(world.join("level.dat")).unwrap(), "nivel-sin-guardar");
    let out = server.unpack(&info.file);
    assert_eq!(std::fs::read_to_string(out.join("level.dat")).unwrap(), "nivel");
    supervisor.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_second_backup_in_the_same_second() {
    let server = Server::new("dup");
    let supervisor = Supervisor::new(Arc::new(Discard));
    let backups = server.backups();
    let create = || backup::create(&supervisor, &server.0, &backups, "Mi mundo", Reason::Manual, "2026-11-07 10-00-00");

    create().await.unwrap();
    assert!(create().await.unwrap_err().contains("mismo segundo"));
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_world_is_an_error() {
    let server = Server::new("missing");
    let supervisor = Supervisor::new(Arc::new(Discard));
    let error = backup::create(&supervisor, &server.0, &server.backups(), "No existe", Reason::Manual, "2026-11-07 10-00-00").await;
    assert!(error.unwrap_err().contains("No existe el mundo"));
}
