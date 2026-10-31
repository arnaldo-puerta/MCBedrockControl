//! Drives the supervisor against the real process built from `examples/fake_bds.rs`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mcbc_lib::bds::EXECUTABLE;
use mcbc_lib::bds::supervisor::{Event, ServerState, Sink, Supervisor};

#[derive(Default)]
struct Recorder(Mutex<Vec<Event>>);

impl Sink for Recorder {
    fn emit(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

impl Recorder {
    fn states(&self) -> Vec<ServerState> {
        let events = self.0.lock().unwrap();
        events
            .iter()
            .filter_map(|e| match e {
                Event::State(state) => Some(*state),
                _ => None,
            })
            .collect()
    }
}

struct ServerDir(PathBuf);

impl ServerDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mcbc-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // target/debug/deps/supervisor-*.exe -> target/debug/examples/fake_bds.exe
        let debug = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_owned();
        std::fs::copy(debug.join("examples").join("fake_bds.exe"), dir.join(EXECUTABLE)).unwrap();
        Self(dir)
    }
}

impl Drop for ServerDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn until(what: &str, condition: impl Fn() -> bool) {
    for _ in 0..500 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

async fn started(name: &str, stop_timeout: Duration) -> (Supervisor, Arc<Recorder>, ServerDir) {
    let dir = ServerDir::new(name);
    let recorder = Arc::new(Recorder::default());
    let supervisor = Supervisor::with_stop_timeout(recorder.clone(), stop_timeout);
    supervisor.start(&dir.0).await.unwrap();
    until("running", || supervisor.state() == ServerState::Running).await;
    (supervisor, recorder, dir)
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_running_only_once_the_server_is_ready() {
    let (supervisor, recorder, _dir) = started("ready", Duration::from_secs(5)).await;
    assert_eq!(recorder.states(), [ServerState::Starting, ServerState::Running]);
    assert!(supervisor.snapshot().history.iter().any(|l| l.text.contains("Server started.")));
    supervisor.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stops_gracefully_and_lets_the_final_logs_through() {
    let (supervisor, recorder, _dir) = started("graceful", Duration::from_secs(5)).await;
    supervisor.stop().await;

    assert_eq!(supervisor.state(), ServerState::Stopped);
    assert_eq!(recorder.states().last(), Some(&ServerState::Stopped));
    assert!(supervisor.snapshot().history.iter().any(|l| l.text.contains("Quit correctly")));
}

#[tokio::test(flavor = "multi_thread")]
async fn kills_a_server_that_ignores_stop() {
    let (supervisor, _recorder, _dir) = started("hang", Duration::from_millis(300)).await;
    supervisor.send_command("hang").unwrap();
    supervisor.stop().await;
    assert_eq!(supervisor.state(), ServerState::Stopped);
}

#[tokio::test(flavor = "multi_thread")]
async fn flags_an_unexpected_exit_as_a_crash() {
    let (supervisor, _recorder, _dir) = started("crash", Duration::from_secs(5)).await;
    supervisor.send_command("crash").unwrap();
    until("crash", || supervisor.state() == ServerState::Crashed).await;
    assert!(!supervisor.is_active());
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_typed_in_the_console_is_not_a_crash() {
    let (supervisor, _recorder, _dir) = started("console-stop", Duration::from_secs(5)).await;
    supervisor.send_command("stop").unwrap();
    until("stopped", || supervisor.state() == ServerState::Stopped).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn tracks_players_without_polling_and_clears_them_on_exit() {
    let (supervisor, _recorder, _dir) = started("players", Duration::from_secs(5)).await;

    supervisor.send_command("join Mi Gamertag").unwrap();
    supervisor.send_command("join Alex").unwrap();
    supervisor.send_command("leave Alex").unwrap();
    until("one player left", || {
        let players = supervisor.snapshot().players;
        players.len() == 1 && players[0].name == "Mi Gamertag"
    })
    .await;

    supervisor.stop().await;
    assert!(supervisor.snapshot().players.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn restarts_with_a_fresh_process() {
    let (supervisor, recorder, dir) = started("restart", Duration::from_secs(5)).await;
    supervisor.restart(&dir.0).await.unwrap();
    until("running again", || supervisor.state() == ServerState::Running).await;

    assert_eq!(
        recorder.states(),
        [
            ServerState::Starting,
            ServerState::Running,
            ServerState::Stopping,
            ServerState::Stopped,
            ServerState::Starting,
            ServerState::Running,
        ],
    );
    supervisor.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn log_history_stays_bounded_and_output_is_batched() {
    let (supervisor, recorder, _dir) = started("flood", Duration::from_secs(5)).await;
    supervisor.send_command("flood").unwrap();
    until("flood to finish", || {
        supervisor.snapshot().history.last().is_some_and(|l| l.text.ends_with("linea 19999"))
    })
    .await;
    assert_eq!(supervisor.snapshot().history.len(), 5000);

    tokio::time::sleep(Duration::from_millis(100)).await;
    let batches = {
        let events = recorder.0.lock().unwrap();
        events.iter().filter(|e| matches!(e, Event::Logs(_))).count()
    };
    assert!(batches < 2000, "expected batching, got {batches} log events");
    supervisor.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_executable_is_an_error_and_leaves_it_stopped() {
    let recorder = Arc::new(Recorder::default());
    let supervisor = Supervisor::new(recorder);
    let error = supervisor.start(&PathBuf::from("no-existe")).await.unwrap_err();

    assert!(error.contains("No se pudo iniciar"));
    assert_eq!(supervisor.state(), ServerState::Stopped);
}

#[tokio::test(flavor = "multi_thread")]
async fn commands_are_rejected_while_stopped() {
    let supervisor = Supervisor::new(Arc::new(Recorder::default()));
    assert!(supervisor.send_command("say hola").is_err());
}
