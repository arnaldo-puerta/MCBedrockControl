//! Owns the BDS child process: lifecycle, console I/O, players and stats.

use std::collections::VecDeque;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, watch};

use super::EXECUTABLE;
use super::parser::{self, Level, ServerEvent};

const HISTORY_LINES: usize = 5000;
const LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(50);
const STATS_INTERVAL: Duration = Duration::from_secs(2);
const KILL_WAIT: Duration = Duration::from_secs(5);
const PIPE_DRAIN_WAIT: Duration = Duration::from_secs(2);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LogLine {
    pub seq: u64,
    pub level: Level,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Player {
    pub name: String,
    pub xuid: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub memory_percent: f32,
    pub uptime_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "lowercase")]
pub enum Event {
    State(ServerState),
    Logs(Vec<LogLine>),
    Players(Vec<Player>),
    Stats(Stats),
    Version(String),
}

pub trait Sink: Send + Sync + 'static {
    fn emit(&self, event: Event);
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Snapshot {
    pub state: ServerState,
    pub players: Vec<Player>,
    pub stats: Stats,
    pub history: Vec<LogLine>,
}

struct Inner {
    state: ServerState,
    players: Vec<Player>,
    stats: Stats,
    history: VecDeque<LogLine>,
    pending: Vec<LogLine>,
    next_seq: u64,
    stdin: Option<mpsc::UnboundedSender<String>>,
    watchers: Vec<mpsc::UnboundedSender<String>>,
    kill: Option<oneshot::Sender<()>>,
    exited: Option<watch::Receiver<bool>>,
}

struct Shared {
    sink: Arc<dyn Sink>,
    inner: Mutex<Inner>,
    // Serialises start/stop so a restart can never interleave with either.
    lifecycle: tokio::sync::Mutex<()>,
    stop_timeout: Duration,
}

#[derive(Clone)]
pub struct Supervisor {
    shared: Arc<Shared>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().expect("server state lock poisoned")
    }

    fn set_state(&self, state: ServerState) {
        self.lock().state = state;
        self.sink.emit(Event::State(state));
    }

    fn push_line(&self, inner: &mut Inner, text: String) {
        let line = LogLine { seq: inner.next_seq, level: parser::level(&text), text };
        inner.next_seq += 1;
        if inner.history.len() == HISTORY_LINES {
            inner.history.pop_front();
        }
        inner.history.push_back(line.clone());
        inner.pending.push(line);
    }

    fn ingest(&self, text: String) {
        let event = parser::parse(&text);
        {
            let mut inner = self.lock();
            // A dropped receiver means its watcher is done.
            inner.watchers.retain(|watcher| watcher.send(text.clone()).is_ok());
            self.push_line(&mut inner, text);
        }
        match event {
            Some(ServerEvent::Ready) => {
                if self.lock().state == ServerState::Starting {
                    self.set_state(ServerState::Running);
                }
            }
            Some(ServerEvent::Version(version)) => self.sink.emit(Event::Version(version)),
            Some(ServerEvent::PlayerJoined { name, xuid }) => self.update_players(|players| {
                players.retain(|p| p.name != name);
                players.push(Player { name, xuid });
            }),
            Some(ServerEvent::PlayerLeft { name }) => {
                self.update_players(|players| players.retain(|p| p.name != name));
            }
            None => {}
        }
    }

    fn update_players(&self, change: impl FnOnce(&mut Vec<Player>)) {
        let players = {
            let mut inner = self.lock();
            change(&mut inner.players);
            inner.players.clone()
        };
        self.sink.emit(Event::Players(players));
    }

    fn flush_logs(&self) {
        let lines = std::mem::take(&mut self.lock().pending);
        if !lines.is_empty() {
            self.sink.emit(Event::Logs(lines));
        }
    }

    fn on_exit(&self, status: std::io::Result<ExitStatus>) {
        let was_stopping = {
            let mut inner = self.lock();
            inner.stdin = None;
            inner.kill = None;
            inner.stats = Stats::default();
            inner.state == ServerState::Stopping
        };
        self.update_players(Vec::clear);
        self.sink.emit(Event::Stats(Stats::default()));

        let clean = status.is_ok_and(|s| s.success());
        self.set_state(if was_stopping || clean { ServerState::Stopped } else { ServerState::Crashed });
    }
}

impl Supervisor {
    pub fn new(sink: Arc<dyn Sink>) -> Self {
        Self::with_stop_timeout(sink, Duration::from_secs(30))
    }

    pub fn with_stop_timeout(sink: Arc<dyn Sink>, stop_timeout: Duration) -> Self {
        let inner = Inner {
            state: ServerState::Stopped,
            players: Vec::new(),
            stats: Stats::default(),
            history: VecDeque::with_capacity(HISTORY_LINES),
            pending: Vec::new(),
            next_seq: 0,
            stdin: None,
            watchers: Vec::new(),
            kill: None,
            exited: None,
        };
        let shared = Shared {
            sink,
            inner: Mutex::new(inner),
            lifecycle: tokio::sync::Mutex::new(()),
            stop_timeout,
        };
        Self { shared: Arc::new(shared) }
    }

    pub fn state(&self) -> ServerState {
        self.shared.lock().state
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state(),
            ServerState::Starting | ServerState::Running | ServerState::Stopping
        )
    }

    pub fn snapshot(&self) -> Snapshot {
        let inner = self.shared.lock();
        Snapshot {
            state: inner.state,
            players: inner.players.clone(),
            stats: inner.stats,
            history: inner.history.iter().cloned().collect(),
        }
    }

    pub async fn start(&self, server_dir: &Path) -> Result<(), String> {
        let _lifecycle = self.shared.lifecycle.lock().await;
        if self.is_active() {
            return Ok(());
        }

        let executable = server_dir.join(EXECUTABLE);
        let mut command = Command::new(&executable);
        command
            .current_dir(server_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);

        let mut child = command
            .spawn()
            .map_err(|e| format!("No se pudo iniciar {}: {e}", executable.display()))?;

        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let pid = child.id();

        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
        let (kill_tx, kill_rx) = oneshot::channel();
        let (exited_tx, exited_rx) = watch::channel(false);
        {
            let mut inner = self.shared.lock();
            inner.stdin = Some(stdin_tx);
            inner.kill = Some(kill_tx);
            inner.exited = Some(exited_rx.clone());
        }
        self.shared.set_state(ServerState::Starting);

        tokio::spawn(write_stdin(stdin, stdin_rx));
        let readers = [
            tokio::spawn(read_lines(stdout, self.shared.clone())),
            tokio::spawn(read_lines(stderr, self.shared.clone())),
        ];
        tokio::spawn(flush_logs(self.shared.clone(), exited_rx.clone()));
        if let Some(pid) = pid {
            tokio::spawn(sample_stats(self.shared.clone(), pid, exited_rx));
        }
        tokio::spawn(watch_exit(self.shared.clone(), child, kill_rx, readers, exited_tx));
        Ok(())
    }

    /// Asks the server to save and quit, killing it only if it ignores us.
    pub async fn stop(&self) {
        let _lifecycle = self.shared.lifecycle.lock().await;
        let (stdin, exited) = {
            let inner = self.shared.lock();
            if !matches!(inner.state, ServerState::Starting | ServerState::Running) {
                return;
            }
            (inner.stdin.clone(), inner.exited.clone())
        };
        let Some(mut exited) = exited else { return };

        self.shared.set_state(ServerState::Stopping);
        if let Some(stdin) = stdin {
            let _ = stdin.send("stop".to_owned());
        }

        let wait = tokio::time::timeout(self.shared.stop_timeout, exited.wait_for(|done| *done));
        if wait.await.is_err() {
            if let Some(kill) = self.shared.lock().kill.take() {
                let _ = kill.send(());
            }
            let _ = tokio::time::timeout(KILL_WAIT, exited.wait_for(|done| *done)).await;
        }
    }

    pub async fn restart(&self, server_dir: &Path) -> Result<(), String> {
        self.stop().await;
        self.start(server_dir).await
    }

    /// Every line the server prints from now on, until the receiver is dropped.
    pub fn watch_lines(&self) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.shared.lock().watchers.push(tx);
        rx
    }

    /// Like [`Self::send_command`], but not echoed to the console, for the
    /// app's own housekeeping commands.
    pub fn send_quietly(&self, command: &str) -> Result<(), String> {
        let inner = self.shared.lock();
        if inner.state != ServerState::Running {
            return Err("El servidor no está encendido.".to_owned());
        }
        let stdin = inner.stdin.clone().ok_or("El servidor no acepta comandos.")?;
        stdin
            .send(command.to_owned())
            .map_err(|_| "El servidor no acepta comandos.".to_owned())
    }

    pub fn send_command(&self, command: &str) -> Result<(), String> {
        let command = command.trim();
        if command.is_empty() {
            return Ok(());
        }
        let mut inner = self.shared.lock();
        if inner.state != ServerState::Running {
            return Err("El servidor no está encendido.".to_owned());
        }
        let stdin = inner.stdin.clone().ok_or("El servidor no acepta comandos.")?;
        stdin
            .send(command.to_owned())
            .map_err(|_| "El servidor no acepta comandos.".to_owned())?;
        self.shared.push_line(&mut inner, format!("> {command}"));
        Ok(())
    }
}

async fn write_stdin(mut stdin: ChildStdin, mut commands: mpsc::UnboundedReceiver<String>) {
    while let Some(command) = commands.recv().await {
        let line = format!("{command}\n");
        if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
            break;
        }
    }
}

async fn read_lines(stream: impl AsyncRead + Unpin, shared: Arc<Shared>) {
    let mut reader = BufReader::new(stream);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buffer);
                shared.ingest(text.trim_end_matches(['\r', '\n']).to_owned());
            }
        }
    }
}

async fn flush_logs(shared: Arc<Shared>, mut exited: watch::Receiver<bool>) {
    let mut ticker = tokio::time::interval(LOG_FLUSH_INTERVAL);
    loop {
        tokio::select! {
            _ = ticker.tick() => shared.flush_logs(),
            _ = exited.wait_for(|done| *done) => {
                shared.flush_logs();
                break;
            }
        }
    }
}

async fn sample_stats(shared: Arc<Shared>, pid: u32, mut exited: watch::Receiver<bool>) {
    let started = Instant::now();
    let pid = Pid::from_u32(pid);
    let cores = std::thread::available_parallelism().map_or(1, usize::from) as f32;
    let refresh = ProcessRefreshKind::nothing().with_cpu().with_memory();

    let mut system = System::new();
    system.refresh_memory();
    let total_memory = system.total_memory().max(1);

    let mut ticker = tokio::time::interval(STATS_INTERVAL);
    loop {
        tokio::select! {
            _ = exited.wait_for(|done| *done) => break,
            _ = ticker.tick() => {}
        }
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh);
        let Some(process) = system.process(pid) else { continue };

        let memory = process.memory();
        let stats = Stats {
            cpu_percent: process.cpu_usage() / cores,
            memory_mb: memory / (1024 * 1024),
            memory_percent: memory as f32 / total_memory as f32 * 100.0,
            uptime_secs: started.elapsed().as_secs(),
        };
        shared.lock().stats = stats;
        shared.sink.emit(Event::Stats(stats));
    }
}

async fn watch_exit(
    shared: Arc<Shared>,
    mut child: Child,
    kill: oneshot::Receiver<()>,
    readers: [tokio::task::JoinHandle<()>; 2],
    exited: watch::Sender<bool>,
) {
    let status = tokio::select! {
        status = child.wait() => status,
        _ = kill => {
            let _ = child.kill().await;
            child.wait().await
        }
    };
    // Let the last lines through, but a grandchild holding the pipes open
    // must not keep us from reporting the exit.
    for reader in readers {
        let _ = tokio::time::timeout(PIPE_DRAIN_WAIT, reader).await;
    }
    shared.on_exit(status);
    let _ = exited.send(true);
}
