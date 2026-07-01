from __future__ import annotations

from PySide6.QtCore import QTimer, Signal
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QFrame,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

from app.core.process_manager import ProcessManager, ServerState
from app.core.server_manager import ServerManager
from app.core.icons import Icons, BUTTON_SIZE, readonly_banner, title_row


class _OptionCard(QFrame):
    def __init__(
        self, label: str, description: str,
        control: QWidget, parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._control = control
        self.setObjectName("card")

        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 12, 16, 12)
        layout.setSpacing(4)

        name = QLabel(label)
        name.setObjectName("optionName")
        layout.addWidget(name)

        desc = QLabel(description)
        desc.setObjectName("optionDesc")
        desc.setWordWrap(True)
        layout.addWidget(desc)

        layout.addSpacing(4)
        layout.addWidget(control)

    def set_read_only(self, ro: bool) -> None:
        self._control.setEnabled(not ro)


class OptionsPage(QWidget):
    settings_saved = Signal()

    def __init__(
        self, process_manager: ProcessManager,
        server_manager: ServerManager,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._pm = process_manager
        self._sm = server_manager
        self._loading: bool = True
        self._cards: list[_OptionCard] = []
        self._original_values: dict[str, str] = {}

        self._read_only_label: QLabel | None = None
        self._save_btn: QPushButton | None = None
        self._revert_btn: QPushButton | None = None
        self._save_notification: QLabel | None = None

        self._setup_ui()
        self._load_values()
        self._loading = False
        self._on_state_changed(self._pm.state)

        self._pm.state_changed.connect(self._on_state_changed)

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
            Icons.OPTIONS, "Opciones",
            "Configura las propiedades del servidor Minecraft Bedrock.",
        ))

        self._read_only_label = readonly_banner(
            "Det\u00e9n el servidor para modificar la configuraci\u00f3n."
        )
        content_layout.addWidget(self._read_only_label)

        self._build_cards(content_layout)

        # ── Button row ────────────────────────────────
        btn_row = QHBoxLayout()
        btn_row.setSpacing(8)

        self._save_btn = QPushButton("Guardar cambios")
        self._save_btn.setIcon(Icons.SAVE)
        self._save_btn.setIconSize(BUTTON_SIZE)
        self._save_btn.setObjectName("btnPrimary")
        self._save_btn.setEnabled(False)
        self._save_btn.clicked.connect(self._save_changes)
        btn_row.addWidget(self._save_btn)

        self._revert_btn = QPushButton("Restablecer")
        self._revert_btn.setIcon(Icons.CANCEL)
        self._revert_btn.setIconSize(BUTTON_SIZE)
        self._revert_btn.setObjectName("btnDanger")
        self._revert_btn.setEnabled(False)
        self._revert_btn.clicked.connect(self._revert_changes)
        btn_row.addWidget(self._revert_btn)

        self._save_notification = QLabel("")
        self._save_notification.setObjectName("optionSaveNotif")
        self._save_notification.setVisible(False)
        btn_row.addWidget(self._save_notification, 1)

        content_layout.addLayout(btn_row)

        content_layout.addStretch()
        scroll.setWidget(content)
        layout.addWidget(scroll, 1)

    def _build_cards(self, container: QVBoxLayout) -> None:
        cards: list[tuple[str, str, QWidget, str]] = [
            (
                "Nombre del servidor",
                "Es el nombre que ver\u00e1n los jugadores en la lista de servidores.",
                self._make_text("server-name"),
                "server-name",
            ),
            (
                "M\u00e1ximo de jugadores",
                "Cantidad m\u00e1xima de jugadores permitidos.",
                self._make_spin("max-players", 1, 30),
                "max-players",
            ),
            (
                "Modo de juego",
                "Modo predeterminado para los nuevos jugadores.",
                self._make_combo("gamemode", [
                    ("Survival", "survival"),
                    ("Creative", "creative"),
                    ("Adventure", "adventure"),
                ]),
                "gamemode",
            ),
            (
                "Dificultad",
                "Nivel de dificultad del servidor.",
                self._make_combo("difficulty", [
                    ("Peaceful", "peaceful"),
                    ("Easy", "easy"),
                    ("Normal", "normal"),
                    ("Hard", "hard"),
                ]),
                "difficulty",
            ),
            (
                "Servidor craqueado",
                "Si est\u00e1 desactivado, solo podr\u00e1n conectarse jugadores "
                "con cuentas premium de Minecraft. "
                "Si est\u00e1 activado, cualquier persona podr\u00e1 conectarse "
                "sin necesidad de una cuenta oficial.",
                self._make_bool("online-mode"),
                "online-mode",
            ),
            (
                "Permitir trucos",
                "Permite el uso de comandos como /gamemode, /tp, "
                "/give, etc.",
                self._make_bool("allow-cheats"),
                "allow-cheats",
            ),
            (
                "Forzar modo de juego",
                "Si est\u00e1 activado, los jugadores siempre tendr\u00e1n "
                "el modo de juego predeterminado del servidor "
                "al reconectarse.",
                self._make_bool("force-gamemode"),
                "force-gamemode",
            ),
            (
                "Paquetes de recursos requeridos",
                "Si est\u00e1 activado, los jugadores deber\u00e1n "
                "descargar y usar el paquete de recursos "
                "del servidor para poder conectarse.",
                self._make_bool("texturepack-required"),
                "texturepack-required",
            ),
        ]

        for label, desc, control, prop in cards:
            card = _OptionCard(label, desc, control)
            control.setProperty("prop_key", prop)
            self._cards.append(card)
            container.addWidget(card)

    # ── Factory helpers ──────────────────────────────

    def _make_text(self, prop: str) -> QLineEdit:
        inp = QLineEdit()
        inp.setObjectName("optionInput")
        inp.textEdited.connect(lambda: self._on_edit(inp))
        return inp

    def _make_spin(self, prop: str, min_val: int, max_val: int) -> QSpinBox:
        spin = QSpinBox()
        spin.setObjectName("optionSpin")
        spin.setRange(min_val, max_val)
        spin.valueChanged.connect(lambda: self._on_edit(spin))
        return spin

    def _make_combo(
        self, prop: str, items: list[tuple[str, str]],
    ) -> QComboBox:
        combo = QComboBox()
        combo.setObjectName("optionCombo")
        for display, _ in items:
            combo.addItem(display)
        combo.setProperty("_combo_values", [v for _, v in items])
        combo.currentIndexChanged.connect(lambda: self._on_edit(combo))
        return combo

    def _make_bool(self, prop: str) -> QCheckBox:
        cb = QCheckBox()
        cb.setObjectName("toggleSwitch")
        cb.toggled.connect(lambda: self._on_edit(cb))
        return cb

    # ── Load / Save ──────────────────────────────────

    def _load_values(self) -> None:
        props = self._sm.read_properties()
        self._original_values = dict(props)
        for card in self._cards:
            control = card._control
            prop = control.property("prop_key")
            raw = props.get(prop, "")
            self._set_control_value(control, raw)

    def _set_control_value(self, control: QWidget, raw: str) -> None:
        if isinstance(control, QLineEdit):
            control.setText(raw)
        elif isinstance(control, QSpinBox):
            try:
                control.setValue(int(raw))
            except ValueError:
                control.setValue(control.minimum())
        elif isinstance(control, QComboBox):
            values: list[str] = control.property("_combo_values")
            try:
                idx = values.index(raw)
            except ValueError:
                idx = 0
            control.setCurrentIndex(idx)
        elif isinstance(control, QCheckBox):
            control.setChecked(raw.lower() == "true")

    def _get_control_value(self, control: QWidget) -> str:
        if isinstance(control, QLineEdit):
            return control.text()
        elif isinstance(control, QSpinBox):
            return str(control.value())
        elif isinstance(control, QComboBox):
            values: list[str] = control.property("_combo_values")
            return values[control.currentIndex()]
        elif isinstance(control, QCheckBox):
            return "true" if control.isChecked() else "false"
        return ""

    def _on_edit(self, control: QWidget) -> None:
        if self._loading:
            return
        self._update_button_states()

    def _has_unsaved_changes(self) -> bool:
        for card in self._cards:
            control = card._control
            prop = control.property("prop_key")
            current = self._get_control_value(control)
            original = self._original_values.get(prop, "")
            if current != original:
                return True
        return False

    def _update_button_states(self) -> None:
        if not self._save_btn or not self._revert_btn:
            return
        running = self._pm.state == ServerState.RUNNING
        has_changes = self._has_unsaved_changes()
        self._save_btn.setEnabled(not running and has_changes)
        self._revert_btn.setEnabled(not running and has_changes)

    def _show_save_notification(self) -> None:
        self._save_notification.setText(
            "\u2705  Cambios guardados correctamente."
        )
        self._save_notification.setVisible(True)
        QTimer.singleShot(4000, lambda: self._save_notification.setVisible(False))

    def _save_changes(self) -> None:
        props: dict[str, str] = {}
        for card in self._cards:
            control = card._control
            prop = control.property("prop_key")
            props[prop] = self._get_control_value(control)
        self._sm.write_properties(props)
        self._original_values = dict(props)
        self._update_button_states()
        self._show_save_notification()
        self.settings_saved.emit()

    def _revert_changes(self) -> None:
        for card in self._cards:
            control = card._control
            prop = control.property("prop_key")
            original = self._original_values.get(prop, "")
            self._set_control_value(control, original)
        self._update_button_states()

    # ── Leave confirmation ───────────────────────────

    def confirm_leave(self) -> bool:
        if not self._has_unsaved_changes():
            return True
        if self._pm.state == ServerState.RUNNING:
            return True

        btn = QMessageBox.question(
            self, "Cambios sin guardar",
            "Hay cambios sin guardar.\n\n"
            "\u00bfDeseas guardar los cambios antes de continuar?",
            QMessageBox.Save | QMessageBox.Discard | QMessageBox.Cancel,
            QMessageBox.Cancel,
        )
        if btn == QMessageBox.Save:
            self._save_changes()
            return True
        elif btn == QMessageBox.Discard:
            self._revert_changes()
            return True
        return False

    # ── State management ─────────────────────────────

    def _on_state_changed(self, state: ServerState) -> None:
        if state == ServerState.RUNNING and self._has_unsaved_changes():
            self._save_changes()

        running = state == ServerState.RUNNING
        for card in self._cards:
            card.set_read_only(running)
        self._read_only_label.setVisible(
            running or state == ServerState.STARTING
        )
        if running:
            self._save_btn.setEnabled(False)
            self._revert_btn.setEnabled(False)
        else:
            self._update_button_states()
