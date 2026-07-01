from __future__ import annotations

import json
from pathlib import Path

from PySide6.QtCore import QTimer, Qt
from PySide6.QtWidgets import (
    QCheckBox,
    QDialog,
    QFrame,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QStackedWidget,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from app.core.process_manager import ProcessManager, ServerState
from app.core.server_manager import ServerManager
from app.core.icons import Icons, BUTTON_SIZE, readonly_banner, title_row


class _EmptyTable(QWidget):
    def __init__(self, text: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setAlignment(Qt.AlignCenter)
        label = QLabel(text)
        label.setObjectName("tableEmpty")
        label.setAlignment(Qt.AlignCenter)
        label.setWordWrap(True)
        layout.addWidget(label)


class _AddPlayerDialog(QDialog):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("Agregar jugador")
        self.setObjectName("addPlayerDialog")
        self.setFixedSize(360, 180)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(20, 20, 20, 20)
        layout.setSpacing(12)

        title = QLabel("Agregar jugador a la lista de permitidos")
        title.setObjectName("dialogTitle")
        layout.addWidget(title)

        self._input = QLineEdit()
        self._input.setObjectName("dialogInput")
        self._input.setPlaceholderText("Nombre del jugador")
        layout.addWidget(self._input)

        btn_row = QHBoxLayout()
        btn_row.setSpacing(8)

        cancel = QPushButton("Cancelar")
        cancel.setIcon(Icons.CANCEL)
        cancel.setIconSize(BUTTON_SIZE)
        cancel.setObjectName("btnSecondary")
        cancel.clicked.connect(self.reject)
        btn_row.addWidget(cancel)

        confirm = QPushButton("Agregar")
        confirm.setIcon(Icons.ADD)
        confirm.setIconSize(BUTTON_SIZE)
        confirm.setObjectName("btnPrimary")
        confirm.clicked.connect(self.accept)
        btn_row.addWidget(confirm)

        layout.addLayout(btn_row)
        self._input.returnPressed.connect(self.accept)

    def player_name(self) -> str:
        return self._input.text().strip()


class PlayersPage(QWidget):
    def __init__(
        self, process_manager: ProcessManager,
        server_manager: ServerManager,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._pm = process_manager
        self._sm = server_manager
        self._connected_players: list[str] = []
        self._server_state: ServerState = ServerState.STOPPED
        self._loading: bool = True

        self._connected_table: QTableWidget | None = None
        self._connected_stack: QStackedWidget | None = None

        self._allow_toggle: QCheckBox | None = None
        self._allow_toggle_label: QLabel | None = None
        self._allow_table: QTableWidget | None = None
        self._allow_stack: QStackedWidget | None = None
        self._add_btn: QPushButton | None = None
        self._remove_btn: QPushButton | None = None

        self._refresh_timer = QTimer(self)
        self._refresh_timer.setInterval(5000)
        self._refresh_timer.timeout.connect(self._refresh_connected)

        self._pm.output_received.connect(self._on_output)
        self._pm.state_changed.connect(self._on_state_changed)

        self._setup_ui()
        self._load_allowlist()
        self._loading = False
        self._on_state_changed(self._pm.state)
        self._update_allow_button_states()

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

        content_layout.addWidget(title_row(
            Icons.PLAYERS, "Jugadores",
            "Administra los jugadores conectados y la lista de permitidos.",
        ))

        # ── Read-only banner ──────────────────────────
        self._read_only_banner = readonly_banner(
            "Det\u00e9n el servidor para administrar la lista de permitidos."
        )
        content_layout.addWidget(self._read_only_banner)

        # ── Connected players ────────────────────────
        conn_card = QFrame()
        conn_card.setObjectName("card")
        conn_layout = QVBoxLayout(conn_card)
        conn_layout.setContentsMargins(16, 12, 16, 12)
        conn_layout.setSpacing(8)

        conn_header = QLabel("Jugadores conectados")
        conn_header.setObjectName("sectionTitle")
        conn_layout.addWidget(conn_header)

        self._connected_stack = QStackedWidget()
        self._connected_stack.setObjectName("tableStack")

        self._connected_table = QTableWidget(0, 1)
        self._connected_table.setObjectName("dataTable")
        self._connected_table.setHorizontalHeaderLabels(["Nombre"])
        self._connected_table.horizontalHeader().setStretchLastSection(True)
        self._connected_table.verticalHeader().setVisible(False)
        self._connected_table.setSelectionBehavior(
            QTableWidget.SelectRows
        )
        self._connected_table.setEditTriggers(
            QTableWidget.NoEditTriggers
        )
        self._connected_stack.addWidget(self._connected_table)

        conn_empty = _EmptyTable("No hay jugadores conectados.")
        self._connected_stack.addWidget(conn_empty)

        conn_layout.addWidget(self._connected_stack, 1)
        content_layout.addWidget(conn_card)

        # ── Allow list ───────────────────────────────
        allow_card = QFrame()
        allow_card.setObjectName("card")
        allow_layout = QVBoxLayout(allow_card)
        allow_layout.setContentsMargins(16, 12, 16, 12)
        allow_layout.setSpacing(8)

        allow_header = QVBoxLayout()
        allow_header.setSpacing(2)

        header_row = QHBoxLayout()
        header_row.setSpacing(8)

        allow_title = QLabel("Lista de permitidos")
        allow_title.setObjectName("sectionTitle")
        header_row.addWidget(allow_title)
        header_row.addStretch()

        self._allow_toggle = QCheckBox()
        self._allow_toggle.setObjectName("toggleSwitch")
        self._allow_toggle.toggled.connect(self._on_toggle_changed)
        header_row.addWidget(self._allow_toggle)

        allow_header.addLayout(header_row)

        status_row = QHBoxLayout()
        status_row.setSpacing(0)
        status_row.addStretch()
        self._allow_toggle_label = QLabel("Desactivada")
        self._allow_toggle_label.setObjectName("toggleStatus")
        status_row.addWidget(self._allow_toggle_label)

        allow_header.addLayout(status_row)
        allow_layout.addLayout(allow_header)

        self._allow_stack = QStackedWidget()
        self._allow_stack.setObjectName("tableStack")

        self._allow_table = QTableWidget(0, 1)
        self._allow_table.setObjectName("dataTable")
        self._allow_table.setHorizontalHeaderLabels(["Nombre"])
        self._allow_table.horizontalHeader().setStretchLastSection(True)
        self._allow_table.verticalHeader().setVisible(False)
        self._allow_table.setSelectionBehavior(
            QTableWidget.SelectRows
        )
        self._allow_table.setEditTriggers(
            QTableWidget.NoEditTriggers
        )
        self._allow_stack.addWidget(self._allow_table)

        allow_empty = _EmptyTable("No hay jugadores en la lista de permitidos.")
        self._allow_stack.addWidget(allow_empty)

        allow_layout.addWidget(self._allow_stack, 1)

        btn_row = QHBoxLayout()
        btn_row.setSpacing(8)

        self._add_btn = QPushButton("Agregar")
        self._add_btn.setIcon(Icons.ADD)
        self._add_btn.setIconSize(BUTTON_SIZE)
        self._add_btn.setObjectName("btnPrimary")
        self._add_btn.clicked.connect(self._add_player)
        btn_row.addWidget(self._add_btn)

        self._remove_btn = QPushButton("Eliminar")
        self._remove_btn.setIcon(Icons.DELETE)
        self._remove_btn.setIconSize(BUTTON_SIZE)
        self._remove_btn.setObjectName("btnDanger")
        self._remove_btn.clicked.connect(self._remove_player)
        btn_row.addWidget(self._remove_btn)

        btn_row.addStretch()
        allow_layout.addLayout(btn_row)

        content_layout.addWidget(allow_card)
        content_layout.addStretch()

        scroll.setWidget(content)
        layout.addWidget(scroll, 1)

    # ── Connected players ────────────────────────────

    def _refresh_connected(self) -> None:
        if self._server_state == ServerState.RUNNING:
            self._pm.send_command("list")

    def _on_output(self, text: str) -> None:
        players: list[str] | None = None
        pending: bool = False

        for line in text.splitlines():
            idx = line.find("players online:")
            if idx != -1:
                rest = line[idx + len("players online:"):].strip()
                if rest:
                    players = [p.strip() for p in rest.split(",")]
                else:
                    players = []
                    pending = True
                continue
            if pending:
                names = [p.strip() for p in line.split(",")]
                if names:
                    players = [n for n in names if n]
                pending = False

        if players is not None:
            self._connected_players = players
            self._update_connected_table()

    def _update_connected_table(self) -> None:
        table = self._connected_table
        if not table:
            return

        table.setRowCount(len(self._connected_players))
        for i, name in enumerate(self._connected_players):
            table.setItem(i, 0, QTableWidgetItem(name))

        if self._connected_stack:
            self._connected_stack.setCurrentIndex(
                0 if self._connected_players else 1
            )

    # ── Allow list ───────────────────────────────────

    @property
    def _allowlist_path(self) -> Path:
        return self._sm.server_path / "allowlist.json"

    def _load_allowlist(self) -> None:
        props = self._sm.read_properties()
        allow_enabled = props.get("allow-list", "").lower() == "true"
        if self._allow_toggle:
            self._allow_toggle.setChecked(allow_enabled)
        self._update_toggle_label(allow_enabled)

        entries: list[dict[str, str]] = []
        path = self._allowlist_path
        if path.is_file():
            try:
                entries = json.loads(path.read_text(encoding="utf-8"))
            except (json.JSONDecodeError, OSError):
                entries = []

        if not isinstance(entries, list):
            entries = []

        names = [e.get("name", "") for e in entries if isinstance(e, dict)]
        self._update_allow_table(names)

    def _save_allowlist(self, names: list[str]) -> None:
        entries = [{"name": n} for n in names]
        self._allowlist_path.write_text(
            json.dumps(entries, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        self._update_allow_table(names)

    def _update_allow_table(self, names: list[str]) -> None:
        table = self._allow_table
        if not table:
            return

        table.setRowCount(len(names))
        for i, name in enumerate(names):
            table.setItem(i, 0, QTableWidgetItem(name))

        if self._allow_stack:
            self._allow_stack.setCurrentIndex(0 if names else 1)

    def _update_toggle_label(self, checked: bool) -> None:
        if self._allow_toggle_label:
            self._allow_toggle_label.setText(
                "Activada" if checked else "Desactivada"
            )

    def _on_toggle_changed(self, checked: bool) -> None:
        if self._loading:
            return
        value = "true" if checked else "false"
        self._sm.write_properties({"allow-list": value})
        self._update_toggle_label(checked)
        self._update_allow_button_states()

    def _is_allow_editable(self) -> bool:
        if self._server_state == ServerState.RUNNING:
            return False
        if not self._allow_toggle or not self._allow_toggle.isChecked():
            return False
        return True

    def _update_allow_button_states(self) -> None:
        editable = self._is_allow_editable()
        if self._allow_toggle:
            self._allow_toggle.setEnabled(
                self._server_state != ServerState.RUNNING
            )
        if self._add_btn:
            self._add_btn.setEnabled(editable)
        if self._remove_btn:
            self._remove_btn.setEnabled(editable)

    def _add_player(self) -> None:
        dialog = _AddPlayerDialog(self)
        if dialog.exec() != _AddPlayerDialog.Accepted:
            return
        name = dialog.player_name()
        if not name:
            return

        table = self._allow_table
        if not table:
            return
        names: list[str] = []
        for row in range(table.rowCount()):
            item = table.item(row, 0)
            if item:
                names.append(item.text())

        if name in names:
            return

        names.append(name)
        self._save_allowlist(names)

    def _remove_player(self) -> None:
        table = self._allow_table
        if not table:
            return
        row = table.currentRow()
        if row < 0:
            return

        names: list[str] = []
        for r in range(table.rowCount()):
            if r != row:
                item = table.item(r, 0)
                if item:
                    names.append(item.text())

        self._save_allowlist(names)

    # ── State management ─────────────────────────────

    def _on_state_changed(self, state: ServerState) -> None:
        running = state == ServerState.RUNNING
        self._server_state = state

        self._read_only_banner.setVisible(
            running or state == ServerState.STARTING
        )

        # Connected players — always active (read-only info)
        if running:
            self._refresh_timer.start()
            self._pm.send_command("list")
        else:
            self._refresh_timer.stop()
            self._connected_players = []
            self._update_connected_table()

        # Allow-list section — disabled while server is running
        allow_enabled = not running
        self._allow_toggle.setEnabled(allow_enabled)
        self._add_btn.setEnabled(
            allow_enabled and self._allow_toggle.isChecked()
        )
        self._remove_btn.setEnabled(
            allow_enabled and self._allow_toggle.isChecked()
        )
        self._allow_table.setEnabled(allow_enabled)

        self._update_allow_button_states()
