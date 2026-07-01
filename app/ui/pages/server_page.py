from __future__ import annotations

import re
from datetime import datetime, timedelta

from PySide6.QtCore import QTimer, Qt
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from app.core.process_manager import ProcessManager, ServerState
from app.core.server_manager import ServerManager
from app.core.server_metadata import get_version as get_server_version
from app.core.icons import Icons, BUTTON_SIZE, readonly_banner, title_row
from app.ui.widgets import CircularProgress

try:
    import psutil
except ImportError:
    psutil = None


_STATE_LABELS: dict[ServerState, str] = {
    ServerState.STOPPED: "Detenido",
    ServerState.STARTING: "Iniciando...",
    ServerState.RUNNING: "En ejecuci\u00f3n",
    ServerState.STOPPING: "Deteniendo...",
    ServerState.ERROR: "Error",
}

_PLAYERS_RE = re.compile(
    r"There are (\d+)/(\d+) players online", re.IGNORECASE
)


def _fmt_time(seconds: float) -> str:
    hours = int(seconds // 3600)
    minutes = int((seconds % 3600) // 60)
    secs = int(seconds % 60)
    return f"{hours:02d}:{minutes:02d}:{secs:02d}"


class ServerPage(QWidget):
    def __init__(
        self, process_manager: ProcessManager,
        server_manager: ServerManager,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._pm = process_manager
        self._sm = server_manager
        self._proc: psutil.Process | None = None
        self._proc_pid: int = 0
        self._start_time: datetime | None = None
        self._recent_events: list[str] = []

        self._status_dot = QLabel()
        self._status_text = QLabel()
        self._uptime_value = QLabel("\u2014")
        self._start_btn = QPushButton()
        self._stop_btn = QPushButton()
        self._restart_btn = QPushButton()
        self._cpu_gauge = CircularProgress("CPU")
        self._ram_gauge = CircularProgress("RAM")
        self._version_label = QLabel()
        self._port_label = QLabel()
        self._world_label = QLabel()
        self._players_label = QLabel()
        self._activity_content = QWidget()

        self._refresh_timer = QTimer(self)
        self._refresh_timer.setInterval(1000)
        self._refresh_timer.timeout.connect(self._refresh_stats)

        self._uptime_timer = QTimer(self)
        self._uptime_timer.setInterval(1000)
        self._uptime_timer.timeout.connect(self._update_uptime)

        self._players_timer = QTimer(self)
        self._players_timer.setInterval(5000)
        self._players_timer.timeout.connect(self._poll_players)

        self._current_players: int = 0
        self._max_players: int = 0

        self._pm.state_changed.connect(self._on_state_changed)
        self._pm.server_started.connect(self._on_server_started)
        self._pm.server_stopped.connect(self._on_server_stopped)
        self._pm.output_received.connect(self._on_output_received)

        self._server_state: ServerState = ServerState.STOPPED

        self._setup_ui()
        self._refresh_server_name()
        self._refresh_quick_info()
        self._on_state_changed(self._pm.state)

    def _make_info_card(self, label: str, value_label: QLabel) -> QFrame:
        card = QFrame()
        card.setObjectName("card")
        layout = QVBoxLayout(card)
        layout.setContentsMargins(12, 10, 12, 10)
        layout.setSpacing(4)
        layout.setAlignment(Qt.AlignCenter)

        lbl = QLabel(label)
        lbl.setObjectName("fieldLabel")
        lbl.setAlignment(Qt.AlignCenter)

        value_label.setObjectName("quickInfoValue")
        value_label.setAlignment(Qt.AlignCenter)
        value_label.setText("\u2014")

        layout.addWidget(lbl)
        layout.addWidget(value_label)
        return card

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.NoFrame)
        scroll.setObjectName("pageScroll")

        content = QWidget()
        content_layout = QVBoxLayout(content)
        content_layout.setContentsMargins(24, 20, 24, 20)
        content_layout.setSpacing(14)

        # ── Title ──────────────────────────────────────
        self._title_row = title_row(
            Icons.SERVER, "Servidor",
            "Controla el servidor dedicado de Minecraft Bedrock.",
        )
        # Find the pageTitle QLabel inside the row to update text later
        self._title_label = self._title_row.findChild(QLabel, "pageTitle")
        content_layout.addWidget(self._title_row)
        content_layout.addSpacing(10)

        # ── Status card (with uptime) ──────────────────
        status_card = QFrame()
        status_card.setObjectName("cardHighlight")
        status_card_layout = QHBoxLayout(status_card)
        status_card_layout.setContentsMargins(20, 16, 20, 16)
        status_card_layout.setSpacing(12)

        self._status_dot.setObjectName("statusDot")
        self._status_dot.setFixedSize(14, 14)
        status_card_layout.addWidget(self._status_dot)

        self._status_text.setObjectName("statusText")
        status_card_layout.addWidget(self._status_text, 1)

        uptime_frame = QFrame()
        uptime_frame.setObjectName("uptimeFrame")
        uptime_layout = QVBoxLayout(uptime_frame)
        uptime_layout.setContentsMargins(0, 0, 0, 0)
        uptime_layout.setSpacing(0)
        uptime_layout.setAlignment(Qt.AlignRight | Qt.AlignVCenter)

        uptime_lbl = QLabel("Tiempo activo")
        uptime_lbl.setObjectName("fieldLabel")
        uptime_layout.addWidget(uptime_lbl)

        self._uptime_value.setObjectName("uptimeValue")
        self._uptime_value.setAlignment(Qt.AlignRight)
        uptime_layout.addWidget(self._uptime_value)

        status_card_layout.addWidget(uptime_frame)
        content_layout.addWidget(status_card)

        # ── Buttons card ─────────────────────────────
        btn_card = QFrame()
        btn_card.setObjectName("card")
        btn_card_layout = QHBoxLayout(btn_card)
        btn_card_layout.setContentsMargins(16, 12, 16, 12)
        btn_card_layout.setSpacing(8)

        self._start_btn = QPushButton("Iniciar")
        self._start_btn.setIcon(Icons.START)
        self._start_btn.setIconSize(BUTTON_SIZE)
        self._start_btn.setObjectName("btnPrimary")
        self._start_btn.clicked.connect(self._pm.start)

        self._stop_btn = QPushButton("Detener")
        self._stop_btn.setIcon(Icons.STOP)
        self._stop_btn.setIconSize(BUTTON_SIZE)
        self._stop_btn.setObjectName("btnDanger")
        self._stop_btn.clicked.connect(self._pm.stop)

        self._restart_btn = QPushButton("Reiniciar")
        self._restart_btn.setIcon(Icons.RESTART)
        self._restart_btn.setIconSize(BUTTON_SIZE)
        self._restart_btn.setObjectName("btnWarning")
        self._restart_btn.clicked.connect(self._pm.restart)

        btn_card_layout.addWidget(self._start_btn, 1)
        btn_card_layout.addWidget(self._stop_btn, 1)
        btn_card_layout.addWidget(self._restart_btn, 1)

        content_layout.addWidget(btn_card)

        # ── World info card ──────────────────────────
        self._world_info_card = QFrame()
        self._world_info_card.setObjectName("infoBanner")
        self._world_info_card.setVisible(False)
        wi_layout = QHBoxLayout(self._world_info_card)
        wi_layout.setContentsMargins(16, 12, 16, 12)
        wi_layout.setSpacing(12)

        wi_icon = QLabel()
        wi_icon.setPixmap(Icons.WORLDS.pixmap(28, 28))
        wi_icon.setFixedSize(28, 28)
        wi_layout.addWidget(wi_icon)

        wi_text = QLabel(
            "A\u00fan no existe un mundo.\n"
            "Inicia el servidor por primera vez o importa un mundo "
            "desde el m\u00f3dulo Mundos."
        )
        wi_text.setObjectName("bannerText")
        wi_text.setWordWrap(True)
        wi_layout.addWidget(wi_text, 1)

        content_layout.addWidget(self._world_info_card)

        # ── Quick info cards ──────────────────────────
        info_row = QHBoxLayout()
        info_row.setSpacing(10)

        info_row.addWidget(
            self._make_info_card("Versi\u00f3n", self._version_label), 1
        )
        info_row.addWidget(
            self._make_info_card("Puerto", self._port_label), 1
        )
        info_row.addWidget(
            self._make_info_card("Mundo", self._world_label), 1
        )
        info_row.addWidget(
            self._make_info_card("Jugadores", self._players_label), 1
        )

        content_layout.addLayout(info_row)

        # ── Gauges + Uptime card ──────────────────────
        gauge_row = QHBoxLayout()
        gauge_row.setSpacing(10)

        gauge_card = QFrame()
        gauge_card.setObjectName("card")
        gauge_card_layout = QHBoxLayout(gauge_card)
        gauge_card_layout.setContentsMargins(12, 8, 12, 8)
        gauge_card_layout.setSpacing(16)

        self._cpu_gauge.setMinimumSize(100, 115)
        gauge_card_layout.addWidget(self._cpu_gauge, 1, Qt.AlignCenter)

        self._ram_gauge.setMinimumSize(100, 115)
        gauge_card_layout.addWidget(self._ram_gauge, 1, Qt.AlignCenter)

        gauge_row.addWidget(gauge_card, 3)

        # Uptime card
        uptime_card = QFrame()
        uptime_card.setObjectName("card")
        uptime_card_layout = QVBoxLayout(uptime_card)
        uptime_card_layout.setContentsMargins(12, 10, 12, 10)
        uptime_card_layout.setAlignment(Qt.AlignCenter)

        uptime_title = QLabel("Tiempo activo")
        uptime_title.setObjectName("fieldLabel")
        uptime_title.setAlignment(Qt.AlignCenter)

        self._uptime_card_value = QLabel("\u2014")
        self._uptime_card_value.setObjectName("uptimeValue")
        self._uptime_card_value.setAlignment(Qt.AlignCenter)

        uptime_card_layout.addWidget(uptime_title)
        uptime_card_layout.addWidget(self._uptime_card_value)

        gauge_row.addWidget(uptime_card, 1)

        content_layout.addLayout(gauge_row)

        # ── Recent activity ───────────────────────────
        activity_card = QFrame()
        activity_card.setObjectName("card")
        activity_card_layout = QVBoxLayout(activity_card)
        activity_card_layout.setContentsMargins(16, 12, 16, 12)
        activity_card_layout.setSpacing(6)

        activity_title = QLabel("Actividad reciente")
        activity_title.setObjectName("sectionTitle")
        activity_card_layout.addWidget(activity_title)

        self._activity_content.setObjectName("activityContent")
        self._activity_content.setAttribute(Qt.WA_StyledBackground, False)
        self._activity_layout_inner = QVBoxLayout(self._activity_content)
        self._activity_layout_inner.setContentsMargins(0, 0, 0, 0)
        self._activity_layout_inner.setSpacing(2)

        # Empty state placeholder
        self._activity_empty = QLabel(
            "No hay eventos recientes."
        )
        self._activity_empty.setObjectName("emptyLabel")
        self._activity_empty.setAlignment(Qt.AlignCenter)
        self._activity_layout_inner.addWidget(self._activity_empty)

        activity_card_layout.addWidget(self._activity_content, 1)
        content_layout.addWidget(activity_card)

        # ── Spacer ───────────────────────────────────
        content_layout.addStretch()

        scroll.setWidget(content)
        layout.addWidget(scroll, 1)

    # ── Server name ─────────────────────────────────

    def _refresh_server_name(self) -> None:
        name = self._sm.server_name
        if not name:
            name = "Servidor sin nombre"
        if self._title_label:
            self._title_label.setText(name)

    # ── Quick info ───────────────────────────────────

    def _refresh_quick_info(self) -> None:
        props = self._sm.read_properties()

        self._version_label.setText(get_server_version())
        self._port_label.setText(
            props.get("server-port", "\u2014")
        )
        world = props.get("level-name", "").strip()
        self._world_label.setText(world if world else "\u2014")
        try:
            self._max_players = int(props.get("max-players", "0"))
        except (ValueError, TypeError):
            self._max_players = 0
        self._update_players_label()
        self._update_world_info()

    def _update_world_info(self) -> None:
        show = (
            self._server_state == ServerState.STOPPED
            and not self._sm.world_exists
        )
        self._world_info_card.setVisible(show)

    # ── Recent activity ─────────────────────────────

    def _on_output_received(self, text: str) -> None:
        for line in text.splitlines():
            clean = line.strip()
            if not clean:
                continue
            self._add_event(clean)
            self._parse_player_count(clean)

    def _add_event(self, text: str) -> None:
        self._recent_events.append(text)
        if len(self._recent_events) > 20:
            self._recent_events.pop(0)
        self._update_activity_ui()

    def _update_activity_ui(self) -> None:
        while self._activity_layout_inner.count():
            item = self._activity_layout_inner.takeAt(0)
            w = item.widget()
            if w:
                w.deleteLater()

        if not self._recent_events:
            self._activity_layout_inner.addWidget(self._activity_empty)
            return

        for evt in self._recent_events:
            lbl = QLabel(evt)
            lbl.setObjectName("activityItem")
            lbl.setWordWrap(True)
            self._activity_layout_inner.addWidget(lbl)

        self._activity_layout_inner.addStretch()

    # ── Uptime ──────────────────────────────────────

    def _update_uptime(self) -> None:
        if self._start_time is None:
            self._uptime_value.setText("\u2014")
            self._uptime_card_value.setText("\u2014")
            return
        elapsed = (datetime.now() - self._start_time).total_seconds()
        fmt = _fmt_time(elapsed)
        self._uptime_value.setText(fmt)
        self._uptime_card_value.setText(fmt)

    # ── Players ─────────────────────────────────────

    def _poll_players(self) -> None:
        self._pm.send_command("list")

    def _parse_player_count(self, line: str) -> None:
        m = _PLAYERS_RE.search(line)
        if not m:
            return
        self._current_players = int(m.group(1))
        self._max_players = int(m.group(2))
        self._update_players_label()

    def _update_players_label(self) -> None:
        self._players_label.setText(
            f"{self._current_players} / {self._max_players}"
        )

    # ── State management ────────────────────────────

    def _on_state_changed(self, state: ServerState) -> None:
        self._server_state = state
        text = _STATE_LABELS.get(state, "Desconocido")
        self._status_text.setText(text)

        if state == ServerState.STOPPED:
            color = "#888888"
            self._start_btn.setEnabled(True)
            self._stop_btn.setEnabled(False)
            self._restart_btn.setEnabled(False)
        elif state == ServerState.STARTING:
            color = "#d4a843"
            self._start_btn.setEnabled(False)
            self._stop_btn.setEnabled(False)
            self._restart_btn.setEnabled(False)
        elif state == ServerState.RUNNING:
            color = "#7bc96f"
            self._start_btn.setEnabled(False)
            self._stop_btn.setEnabled(True)
            self._restart_btn.setEnabled(True)
        elif state == ServerState.STOPPING:
            color = "#d4a843"
            self._start_btn.setEnabled(False)
            self._stop_btn.setEnabled(False)
            self._restart_btn.setEnabled(False)
        elif state == ServerState.ERROR:
            color = "#c94f4f"
            self._start_btn.setEnabled(True)
            self._stop_btn.setEnabled(False)
            self._restart_btn.setEnabled(False)
        else:
            color = "#888888"
            self._start_btn.setEnabled(True)
            self._stop_btn.setEnabled(False)
            self._restart_btn.setEnabled(False)

        self._status_dot.setStyleSheet(
            f"background-color: {color};"
            " border-radius: 7px;"
            " border: 2px solid #1a1a1a;"
        )

        self._update_world_info()

    def _on_server_started(self) -> None:
        self._proc = None
        self._proc_pid = 0
        self._start_time = datetime.now()
        self._refresh_timer.start()
        self._uptime_timer.start()
        self._players_timer.start()
        self._current_players = 0
        self._update_players_label()
        self._add_event("Servidor iniciado")

    def _on_server_stopped(self) -> None:
        self._refresh_timer.stop()
        self._uptime_timer.stop()
        self._players_timer.stop()
        self._start_time = None
        self._proc = None
        self._proc_pid = 0
        self._cpu_gauge.set_value(0)
        self._ram_gauge.set_value(0)
        self._ram_gauge.set_unit("%")
        self._current_players = 0
        self._update_players_label()
        self._update_uptime()
        self._add_event("Servidor detenido")

    # ── Stats refresh ────────────────────────────────

    def _refresh_stats(self) -> None:
        if not psutil:
            self._cpu_gauge.set_fallback("\u2014")
            self._ram_gauge.set_fallback("\u2014")
            return

        pid = self._pm._process.processId()
        if not pid:
            self._cpu_gauge.set_value(0)
            self._ram_gauge.set_value(0)
            return

        if pid != self._proc_pid:
            try:
                self._proc = psutil.Process(pid)
                self._proc_pid = pid
                self._proc.cpu_percent(interval=0)
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                self._cpu_gauge.set_fallback("\u2014")
                self._ram_gauge.set_fallback("\u2014")
                return

        if self._proc is None:
            return

        try:
            cpu = self._proc.cpu_percent(interval=0)
            mem_info = self._proc.memory_info()
            mem_total = psutil.virtual_memory().total
            mem_pct = (mem_info.rss / mem_total) * 100

            self._cpu_gauge.set_value(cpu)
            self._ram_gauge.set_value(mem_pct)
            self._ram_gauge.set_unit("%")

        except (psutil.NoSuchProcess, psutil.AccessDenied):
            self._cpu_gauge.set_fallback("\u2014")
            self._ram_gauge.set_fallback("\u2014")
