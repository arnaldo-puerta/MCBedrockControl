//! Updates and adds a pack to a real server. Point `MCBC_REAL_SERVER` at a
//! disposable copy of a server folder (it gets replaced), then run:
//! `cargo test --test real_update -- --ignored --nocapture`

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use mcbc_lib::bds::supervisor::{Event, ServerState, Sink, Supervisor};
use mcbc_lib::bds::{packs, updater, worlds};

struct Discard;

impl Sink for Discard {
    fn emit(&self, _event: Event) {}
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads the real server; needs MCBC_REAL_SERVER pointing at a disposable copy"]
async fn update_keeps_everything_and_the_server_boots_with_a_new_pack() {
    let server = PathBuf::from(std::env::var("MCBC_REAL_SERVER").expect("MCBC_REAL_SERVER"));
    let world = worlds::active(&server).expect("active world");
    let level_dat = std::fs::read(worlds::worlds_dir(&server).join(&world).join("level.dat")).unwrap();
    let allowlist = std::fs::read_to_string(server.join("allowlist.json")).unwrap();

    // A pack of the player's own, which the update must carry over.
    let own = server.join("resource_packs").join("Mis texturas");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("manifest.json"), manifest("Mis texturas", "bbbbbbbb-0000-0000-0000-000000000001", "resources")).unwrap();

    let version = updater::update(&server, Arc::new(AtomicBool::new(false)), |_| {}).await.unwrap();
    println!("actualizado a {version}");

    assert_eq!(std::fs::read(worlds::worlds_dir(&server).join(&world).join("level.dat")).unwrap(), level_dat);
    assert_eq!(std::fs::read_to_string(server.join("allowlist.json")).unwrap(), allowlist);
    assert!(std::fs::read_to_string(server.join("server.properties")).unwrap().contains("server-port=19150"));
    assert!(own.join("manifest.json").is_file(), "the player's pack was lost");

    // Now add a pack the way the app does.
    let file = server.join("prueba.mcpack");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(manifest("Pack de prueba", "bbbbbbbb-0000-0000-0000-000000000002", "resources").as_bytes()).unwrap();
    zip.finish().unwrap();
    let installed = packs::import(&server, &file).unwrap();
    assert!(installed[0].enabled);
    println!("packs del jugador: {:?}", packs::list(&server).unwrap().iter().map(|p| (&p.name, p.enabled)).collect::<Vec<_>>());

    let supervisor = Supervisor::new(Arc::new(Discard));
    supervisor.start(&server).await.unwrap();
    for _ in 0..600 {
        if supervisor.state() != ServerState::Starting {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let history = supervisor.snapshot().history;
    for line in history.iter().filter(|l| {
        ["Version", "Level Name", "Server started", "pack", "Pack", "ERROR"].iter().any(|w| l.text.contains(w))
    }) {
        println!("{}", line.text);
    }
    assert_eq!(supervisor.state(), ServerState::Running);
    supervisor.stop().await;
}

/// A valid manifest: the module needs its own well-formed uuid.
fn manifest(name: &str, uuid: &str, module: &str) -> String {
    let module_uuid = format!("cccccccc{}", &uuid[8..]);
    format!(
        r#"{{"format_version":2,"header":{{"name":"{name}","description":"prueba","uuid":"{uuid}","version":[1,0,0],"min_engine_version":[1,26,0]}},"modules":[{{"type":"{module}","uuid":"{module_uuid}","version":[1,0,0]}}]}}"#
    )
}
