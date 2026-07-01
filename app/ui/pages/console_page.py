from __future__ import annotations

import json
from pathlib import Path

from PySide6.QtCore import QEvent, Qt, Signal
from PySide6.QtGui import QColor, QFont, QKeyEvent, QTextCharFormat, QTextCursor
from PySide6.QtWidgets import (
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPlainTextEdit,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QSplitter,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from app.core.process_manager import ProcessManager, ServerState
from app.core.icons import Icons, BUTTON_SIZE, title_row

_LINE_COLORS: dict[str, QColor] = {
    "[INFO]": QColor("#7bc96f"),
    "[WARN]": QColor("#d4a843"),
    "[ERROR]": QColor("#c94f4f"),
}
_DEFAULT_COLOR = QColor("#d0d0d0")


from app.core.paths import resource_path


def _load_commands() -> dict[str, list[dict[str, str]]]:
    path = resource_path() / "resources" / "commands.json"
    if path.is_file():
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    return {}


class CommandItem(QFrame):
    inserted = Signal(str)

    def __init__(
        self, command: str, syntax: str, description: str,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._syntax = syntax
        self.setObjectName("cmdItem")
        self.setCursor(Qt.PointingHandCursor)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 4, 8, 4)
        layout.setSpacing(0)

        name = QLabel(command)
        name.setObjectName("cmdItemName")
        layout.addWidget(name)

        desc = QLabel(description)
        desc.setObjectName("cmdItemDesc")
        desc.setWordWrap(True)
        layout.addWidget(desc)

    def mouseDoubleClickEvent(self, event: object) -> None:
        self.inserted.emit(self._syntax)


class CategorySection(QWidget):
    inserted = Signal(str)

    def __init__(
        self, category: str, commands: list[dict[str, str]],
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._commands: list[CommandItem] = []

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(1)

        header = QLabel(category)
        header.setObjectName("cmdCategory")
        layout.addWidget(header)

        for cmd in commands:
            item = CommandItem(
                cmd["command"], cmd["syntax"], cmd["description"],
            )
            item.inserted.connect(self.inserted.emit)
            self._commands.append(item)
            layout.addWidget(item)

        layout.addSpacing(4)

    def filter(self, text: str) -> bool:
        lowered = text.lower()
        visible = False
        for item in self._commands:
            cmd_text = item._syntax.lower()
            match = not text or lowered in cmd_text
            item.setVisible(match)
            if match:
                visible = True
        self.setVisible(visible)
        return visible


class CommandPanel(QFrame):
    insert_requested = Signal(str)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._sections: list[CategorySection] = []
        self._setup_ui()

    def _setup_ui(self) -> None:
        self.setObjectName("commandPanel")
        self.setFixedWidth(260)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        self._search = QLineEdit()
        self._search.setObjectName("cmdSearch")
        self._search.setPlaceholderText("Buscar comando...")
        self._search.textChanged.connect(self._on_search)
        layout.addWidget(self._search)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.NoFrame)
        scroll.setObjectName("cmdScroll")

        content = QWidget()
        content.setObjectName("cmdContent")
        content_layout = QVBoxLayout(content)
        content_layout.setContentsMargins(8, 8, 8, 8)
        content_layout.setSpacing(0)

        data = _load_commands()
        for category, commands in data.items():
            section = CategorySection(category, commands)
            section.inserted.connect(self._forward_insert)
            self._sections.append(section)
            content_layout.addWidget(section)

        content_layout.addStretch()

        scroll.setWidget(content)
        layout.addWidget(scroll, 1)

    def _on_search(self, text: str) -> None:
        for section in self._sections:
            section.filter(text)

    def _forward_insert(self, syntax: str) -> None:
        self.insert_requested.emit(syntax)


class ConsolePage(QWidget):
    def __init__(
        self, process_manager: ProcessManager,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._pm = process_manager
        self._command_history: list[str] = []
        self._history_index: int = -1
        self._auto_scroll: bool = True
        self._output_buffer: str = ""

        self._output: QPlainTextEdit | None = None
        self._input: QLineEdit | None = None
        self._send_btn: QPushButton | None = None
        self._search_input: QLineEdit | None = None
        self._clear_btn: QPushButton | None = None
        self._save_btn: QPushButton | None = None
        self._stopped_overlay: QLabel | None = None

        self._pm.output_received.connect(self._append_output)
        self._pm.error_received.connect(self._append_output)
        self._pm.state_changed.connect(self._on_state_changed)

        self._setup_ui()
        self._on_state_changed(self._pm.state)

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.NoFrame)
        scroll.setObjectName("pageScroll")

        content = QWidget()
        content.setObjectName("consoleContent")
        content_layout = QVBoxLayout(content)
        content_layout.setContentsMargins(24, 20, 24, 20)
        content_layout.setSpacing(14)

        # ── Title ────────────────────────────────────
        content_layout.addWidget(title_row(
            Icons.CONSOLE, "Consola",
            "Administra la consola y ejecuta comandos del servidor.",
        ))

        # ── Toolbar ───────────────────────────────────
        toolbar = QFrame()
        toolbar.setObjectName("toolbar")
        toolbar_layout = QHBoxLayout(toolbar)
        toolbar_layout.setContentsMargins(0, 0, 0, 0)
        toolbar_layout.setSpacing(8)

        self._search_input = QLineEdit()
        self._search_input.setObjectName("consoleSearch")
        self._search_input.setPlaceholderText("Buscar en salida...")
        self._search_input.textChanged.connect(self._highlight_search)
        toolbar_layout.addWidget(self._search_input)

        self._clear_btn = QPushButton("Limpiar")
        self._clear_btn.setIcon(Icons.RELOAD)
        self._clear_btn.setIconSize(BUTTON_SIZE)
        self._clear_btn.setObjectName("btnSecondary")
        self._clear_btn.clicked.connect(self._clear_console)
        toolbar_layout.addWidget(self._clear_btn)

        self._save_btn = QPushButton("Guardar log")
        self._save_btn.setIcon(Icons.SAVE)
        self._save_btn.setIconSize(BUTTON_SIZE)
        self._save_btn.setObjectName("btnSecondary")
        self._save_btn.clicked.connect(self._save_log)
        toolbar_layout.addWidget(self._save_btn)

        content_layout.addWidget(toolbar)

        # ── Splitter: output | command panel ─────────
        splitter = QSplitter(Qt.Horizontal)
        splitter.setObjectName("consoleSplitter")

        # Output area
        output_frame = QFrame()
        output_frame.setObjectName("consoleOutputFrame")

        output_frame_layout = QVBoxLayout(output_frame)
        output_frame_layout.setContentsMargins(0, 0, 0, 0)
        output_frame_layout.setSpacing(0)

        self._output = QPlainTextEdit()
        self._output.setReadOnly(True)
        self._output.setObjectName("consoleOutput")
        self._output.setUndoRedoEnabled(False)
        self._output.setVerticalScrollBarPolicy(Qt.ScrollBarAsNeeded)

        sb = self._output.verticalScrollBar()
        sb.valueChanged.connect(self._on_scroll_changed)

        output_frame_layout.addWidget(self._output)

        self._stopped_overlay = QLabel("Servidor detenido")
        self._stopped_overlay.setObjectName("consoleStoppedOverlay")
        self._stopped_overlay.setAlignment(Qt.AlignCenter)
        self._stopped_overlay.setVisible(False)
        output_frame_layout.addWidget(self._stopped_overlay)

        splitter.addWidget(output_frame)

        # Command panel
        self._cmd_panel = CommandPanel()
        self._cmd_panel.insert_requested.connect(self._on_command_insert)
        splitter.addWidget(self._cmd_panel)

        splitter.setStretchFactor(0, 3)
        splitter.setStretchFactor(1, 1)

        content_layout.addWidget(splitter, 1)

        # ── Input area ────────────────────────────────
        input_frame = QFrame()
        input_frame.setObjectName("consoleInputFrame")
        input_layout = QHBoxLayout(input_frame)
        input_layout.setContentsMargins(0, 0, 0, 0)
        input_layout.setSpacing(8)

        self._input = QLineEdit()
        self._input.setObjectName("consoleInput")
        self._input.setPlaceholderText("Escribir comando...")
        self._input.returnPressed.connect(self._send_command)
        self._input.installEventFilter(self)
        input_layout.addWidget(self._input, 1)

        self._send_btn = QPushButton("Enviar")
        self._send_btn.setIcon(Icons.START)
        self._send_btn.setIconSize(BUTTON_SIZE)
        self._send_btn.setObjectName("btnPrimary")
        self._send_btn.clicked.connect(self._send_command)
        input_layout.addWidget(self._send_btn)

        content_layout.addWidget(input_frame)

        scroll.setWidget(content)
        layout.addWidget(scroll, 1)

    # ── Event filter for history navigation ───────────

    def eventFilter(self, obj: QWidget, event: QEvent) -> bool:
        if obj is self._input and event.type() == QEvent.KeyPress:
            key_event: QKeyEvent = event  # type: ignore[annotation-unchecked]
            if key_event.key() == Qt.Key_Up:
                self._history_up()
                return True
            if key_event.key() == Qt.Key_Down:
                self._history_down()
                return True
        return super().eventFilter(obj, event)

    def _history_up(self) -> None:
        if not self._command_history:
            return
        if self._history_index < 0:
            self._history_index = len(self._command_history) - 1
        elif self._history_index > 0:
            self._history_index -= 1
        self._input.setText(self._command_history[self._history_index])

    def _history_down(self) -> None:
        if self._history_index < 0:
            return
        self._history_index += 1
        if self._history_index >= len(self._command_history):
            self._history_index = -1
            self._input.clear()
        else:
            self._input.setText(self._command_history[self._history_index])

    # ── Output with color highlighting ────────────────

    def _append_output(self, text: str) -> None:
        self._output_buffer += text
        while "\n" in self._output_buffer:
            line, self._output_buffer = self._output_buffer.split("\n", 1)
            self._append_line(line)

    def _append_line(self, line: str) -> None:
        cursor = self._output.textCursor()
        cursor.movePosition(QTextCursor.End)

        color = _DEFAULT_COLOR
        for prefix, c in _LINE_COLORS.items():
            if line.startswith(prefix):
                color = c
                break

        fmt = QTextCharFormat()
        fmt.setForeground(color)
        cursor.setCharFormat(fmt)
        cursor.insertText(line + "\n")

        if self._auto_scroll:
            self._output.ensureCursorVisible()

    # ── Auto-scroll ───────────────────────────────────

    def _on_scroll_changed(self, value: int) -> None:
        sb = self._output.verticalScrollBar()
        self._auto_scroll = value >= sb.maximum() - 5

    # ── Search ────────────────────────────────────────

    def _highlight_search(self, text: str) -> None:
        selections: list = []
        if text:
            fmt = QTextCharFormat()
            fmt.setBackground(Qt.yellow)
            fmt.setForeground(Qt.black)

            cursor = QTextCursor(self._output.document())
            while True:
                cursor = self._output.document().find(text, cursor)
                if cursor.isNull():
                    break
                sel = QTextEdit.ExtraSelection()
                sel.format = fmt
                sel.cursor = cursor
                selections.append(sel)
        self._output.setExtraSelections(selections)

    # ── Clear ─────────────────────────────────────────

    def _clear_console(self) -> None:
        self._output.clear()

    # ── Save log ──────────────────────────────────────

    def _save_log(self) -> None:
        path, _ = QFileDialog.getSaveFileName(
            self, "Guardar log", str(Path.home() / "server_log.txt"),
            "Archivos de texto (*.txt);;Todos los archivos (*)",
        )
        if path:
            with open(path, "w", encoding="utf-8") as f:
                f.write(self._output.toPlainText())

    # ── Send command ──────────────────────────────────

    def _send_command(self) -> None:
        text = self._input.text().strip()
        if not text:
            return
        self._command_history.append(text)
        self._history_index = -1
        self._pm.send_command(text)
        self._input.clear()

    def _on_command_insert(self, syntax: str) -> None:
        self._input.setText(syntax)
        self._input.setFocus()

    # ── State management ──────────────────────────────

    def _on_state_changed(self, state: ServerState) -> None:
        enabled = state == ServerState.RUNNING
        self._input.setEnabled(enabled)
        self._send_btn.setEnabled(enabled)
        self._stopped_overlay.setVisible(not enabled)
        if enabled:
            self._input.setPlaceholderText("Escribir comando...")
        else:
            self._input.setPlaceholderText("")
            self._output.ensureCursorVisible()
