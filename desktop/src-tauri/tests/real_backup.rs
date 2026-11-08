//! Live backup against a real server. Point `MCBC_REAL_SERVER` at a
//! disposable copy of a server folder (it gets modified), then run:
//! `cargo test --test real_backup -- --ignored --nocapture`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mcbc_lib::bds::backup::{self, Reason};
use mcbc_lib::bds::supervisor::{Event, ServerState, Sink, Supervisor};
use mcbc_lib::bds::{archive, worlds};

struct Discard;

impl Sink for Discard {
    fn emit(&self, _event: Event) {}
}

async fn wait_running(supervisor: &Supervisor) {
    for _ in 0..600 {
        if supervisor.state() == ServerState::Running {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not start: {:?}", supervisor.state());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs MCBC_REAL_SERVER pointing at a disposable server copy"]
async fn live_backup_of_a_real_server_loads_back() {
    let server = PathBuf::from(std::env::var("MCBC_REAL_SERVER").expect("MCBC_REAL_SERVER"));
    let backups = server.join("copias-prueba");
    let folder = worlds::active(&server).expect("active world");
    let supervisor = Supervisor::new(Arc::new(Discard));

    supervisor.start(&server).await.unwrap();
    wait_running(&supervisor).await;
    let info = backup::create(&supervisor, &server, &backups, &folder, Reason::Manual, "2026-11-07 12-00-00")
        .await
        .unwrap();
    println!("copia en vivo: {} ({} bytes)", info.file, info.size_bytes);

    // The world must keep accepting changes after the backup.
    let mut lines = supervisor.watch_lines();
    supervisor.send_command("save hold").unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(5), lines.recv()).await.unwrap().unwrap();
    println!("tras la copia, save hold respondió: {reply}");
    assert!(reply.contains("Saving..."), "the world was left held");
    supervisor.send_command("save resume").unwrap();
    supervisor.stop().await;

    let out = server.join("revisar");
    archive::extract(&backups.join(&info.file), &out, &Default::default(), |_, _| {}).unwrap();
    assert!(out.join("level.dat").is_file());
    println!("archivos en la copia: {}", archive::list_files(&out).unwrap().len());

    // Load the backup as a separate world and boot the server on it.
    let restored = worlds::unpack(&server, &backups.join(&info.file), Some("Restaurado"), false).unwrap();
    worlds::set_active(&server, &restored).unwrap();
    supervisor.start(&server).await.unwrap();
    wait_running(&supervisor).await;
    let history = supervisor.snapshot().history;
    for line in history.iter().filter(|l| l.text.contains("Level Name") || l.text.contains("Server started")) {
        println!("{}", line.text);
    }
    supervisor.stop().await;
    assert_eq!(supervisor.state(), ServerState::Stopped);
}
