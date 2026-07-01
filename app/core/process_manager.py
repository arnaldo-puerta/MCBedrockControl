from __future__ import annotations

from enum import Enum, auto

from PySide6.QtCore import QObject, QProcess, Signal


class ServerState(Enum):
    STOPPED = auto()
    STARTING = auto()
    RUNNING = auto()
    STOPPING = auto()
    ERROR = auto()


class ProcessManager(QObject):
    output_received = Signal(str)
    error_received = Signal(str)
    server_started = Signal()
    server_stopped = Signal()
    state_changed = Signal(object)

    STOP_TIMEOUT_MS = 30000

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._process: QProcess = QProcess(self)
        self._process.setProcessChannelMode(QProcess.SeparateChannels)
        self._state: ServerState = ServerState.STOPPED
        self._executable: str = ""
        self._working_dir: str = ""
        self._restart_pending: bool = False

        self._process.started.connect(self._on_started)
        self._process.finished.connect(self._on_finished)
        self._process.errorOccurred.connect(self._on_error)
        self._process.readyReadStandardOutput.connect(self._read_stdout)
        self._process.readyReadStandardError.connect(self._read_stderr)

    @property
    def state(self) -> ServerState:
        return self._state

    @property
    def is_running(self) -> bool:
        return self._state in (ServerState.RUNNING, ServerState.STARTING)

    def configure(self, executable: str, working_dir: str) -> None:
        self._executable = executable
        self._working_dir = working_dir

    def start(self) -> None:
        if self.is_running:
            return
        if not self._executable:
            return

        self._set_state(ServerState.STARTING)
        self._process.setWorkingDirectory(self._working_dir)
        self._process.start(self._executable)

    def stop(self) -> None:
        if self._state != ServerState.RUNNING:
            return

        self._set_state(ServerState.STOPPING)
        self._write("stop")
        if not self._process.waitForFinished(self.STOP_TIMEOUT_MS):
            self._process.kill()
            self._process.waitForFinished(5000)
        if self._state == ServerState.STOPPING:
            self._set_state(ServerState.STOPPED)

    def restart(self) -> None:
        if self._state == ServerState.STOPPED:
            self.start()
        elif self._state == ServerState.RUNNING:
            self._restart_pending = True
            self.stop()

    def send_command(self, command: str) -> None:
        if self._state == ServerState.RUNNING:
            self._write(command)

    def _write(self, data: str) -> None:
        self._process.write(f"{data}\n".encode("utf-8"))

    def _set_state(self, state: ServerState) -> None:
        self._state = state
        self.state_changed.emit(state)

    def _read_stdout(self) -> None:
        raw = self._process.readAllStandardOutput().data()
        self.output_received.emit(raw.decode("utf-8", errors="replace"))

    def _read_stderr(self) -> None:
        raw = self._process.readAllStandardError().data()
        self.error_received.emit(raw.decode("utf-8", errors="replace"))

    def _on_started(self) -> None:
        self._set_state(ServerState.RUNNING)
        self.server_started.emit()

    def _on_finished(self, exit_code: int, exit_status: QProcess.ExitStatus) -> None:
        was_running = self._state != ServerState.STOPPED
        self._set_state(ServerState.STOPPED)
        self.server_stopped.emit()

        if self._restart_pending:
            self._restart_pending = False
            self.start()

    def _on_error(self, error: QProcess.ProcessError) -> None:
        if self._state != ServerState.STOPPING:
            self._set_state(ServerState.ERROR)
