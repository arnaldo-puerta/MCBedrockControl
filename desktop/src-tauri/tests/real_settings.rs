//! Renames a real world and boots the server on it. Point `MCBC_REAL_SERVER`
//! at a disposable copy of a server folder, then run:
//! `cargo test --test real_settings -- --ignored --nocapture`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mcbc_lib::bds::supervisor::{Event, ServerState, Sink, Supervisor};
use mcbc_lib::bds::{level_dat, settings, worlds};

struct Discard;

impl Sink for Discard {
    fn emit(&self, _event: Event) {}
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs MCBC_REAL_SERVER pointing at a disposable server copy"]
async fn renamed_world_still_loads() {
    let server = PathBuf::from(std::env::var("MCBC_REAL_SERVER").expect("MCBC_REAL_SERVER"));
    let mut current = settings::read(&server);
    println!("antes: servidor={:?} mundo={:?} modo={}", current.server_name, current.world_name, current.gamemode);

    settings::rename_world(&server, "Mundo de prueba ñ").unwrap();
    current.server_name = "Servidor de prueba".into();
    current.gamemode = "creative".into();
    current.world_name = Some("Mundo de prueba ñ".into());
    settings::write(&server, &current).unwrap();

    let after = settings::read(&server);
    println!("después: servidor={:?} mundo={:?} modo={}", after.server_name, after.world_name, after.gamemode);
    assert_eq!(after.world_name.as_deref(), Some("Mundo de prueba ñ"));

    let supervisor = Supervisor::new(Arc::new(Discard));
    supervisor.start(&server).await.unwrap();
    for _ in 0..600 {
        if supervisor.state() != ServerState::Starting {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    for line in supervisor.snapshot().history.iter().filter(|l| {
        ["Level Name", "Game mode", "Server started", "ERROR", "corrupt"].iter().any(|w| l.text.contains(w))
    }) {
        println!("{}", line.text);
    }
    assert_eq!(supervisor.state(), ServerState::Running);
    supervisor.stop().await;

    // The server rewrites level.dat on shutdown; the new name must survive it.
    let folder = worlds::active(&server).unwrap();
    let data = std::fs::read(worlds::worlds_dir(&server).join(folder).join("level.dat")).unwrap();
    let name = level_dat::read_name(&data).unwrap();
    println!("nombre guardado por el servidor al cerrar: {name:?}");
    assert_eq!(name, "Mundo de prueba ñ");
}
