from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QCheckBox,
    QDialog,
    QFileDialog,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from app.core.config import Config
from app.core.server_manager import ServerManager
from app.core.task_manager import TaskManager
from app.ui.progress_dialog import ProgressDialog


class InstallWizard(QDialog):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("Instalar servidor Bedrock")
        self.setModal(True)
        self.setMinimumWidth(500)
        self.resize(520, 380)

        self._server_name_input = QLineEdit()
        self._zip_path_input = QLineEdit()
        self._zip_browse_btn = QPushButton("Examinar...")
        self._eula_checkbox = QCheckBox(
            "He le\u00eddo y acepto el EULA de Minecraft"
        )
        self._install_btn = QPushButton("Instalar servidor")

        self._task_manager = TaskManager(self)
        self._progress_dialog = ProgressDialog(self)
        self._progress_dialog.bind(self._task_manager)

        self._server_name_input.textChanged.connect(self._validate)
        self._zip_path_input.textChanged.connect(self._validate)
        self._eula_checkbox.toggled.connect(self._validate)
        self._install_btn.clicked.connect(self._on_install)

        self._task_manager.started.connect(lambda _: self._install_btn.setEnabled(False))
        self._task_manager.finished.connect(self._on_install_done)
        self._task_manager.error.connect(self._on_install_error)
        self._progress_dialog.rejected.connect(self._on_progress_rejected)

        self._setup_ui()
        self._validate()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(0)

        title = QLabel("Instalar servidor")
        title.setObjectName("wizardTitle")
        layout.addWidget(title)

        subtitle = QLabel(
            "Configura tu servidor Bedrock antes de comenzar."
        )
        subtitle.setObjectName("wizardSubtitle")
        subtitle.setWordWrap(True)
        layout.addWidget(subtitle)

        layout.addSpacing(24)

        # ── Server name ──────────────────────────
        name_label = QLabel("Nombre del servidor")
        name_label.setObjectName("fieldLabel")
        layout.addWidget(name_label)

        self._server_name_input.setPlaceholderText("Mi servidor")
        self._server_name_input.setObjectName("wizardInput")
        layout.addWidget(self._server_name_input)

        layout.addSpacing(18)

        # ── ZIP file selector ────────────────────
        zip_label = QLabel("Archivo del servidor (.zip)")
        zip_label.setObjectName("fieldLabel")
        layout.addWidget(zip_label)

        zip_row = QHBoxLayout()
        zip_row.setSpacing(8)

        self._zip_path_input.setPlaceholderText(
            "Selecciona el archivo ZIP del servidor..."
        )
        self._zip_path_input.setObjectName("wizardInput")
        self._zip_path_input.setReadOnly(True)
        zip_row.addWidget(self._zip_path_input, 1)

        self._zip_browse_btn.setObjectName("btnSecondary")
        self._zip_browse_btn.clicked.connect(self._browse_zip)
        zip_row.addWidget(self._zip_browse_btn)

        layout.addLayout(zip_row)

        layout.addSpacing(18)

        # ── EULA ────────────────────────────────
        self._eula_checkbox.setObjectName("wizardEulaCheck")
        layout.addWidget(self._eula_checkbox)

        # eula link
        eula_link = QLabel(
            '<a href="https://aka.ms/MinecraftEULA" '
            'style="color: #7bc96f;">'
            "minecraft.net/eula</a>"
        )
        eula_link.setObjectName("wizardEulaLink")
        eula_link.setOpenExternalLinks(True)
        eula_link.setContentsMargins(26, 0, 0, 0)
        layout.addWidget(eula_link)

        layout.addStretch()

        # ── Install button ──────────────────────
        self._install_btn.setObjectName("btnPrimary")
        self._install_btn.setEnabled(False)
        layout.addWidget(self._install_btn)

    # ── Validation ─────────────────────────────────

    def _validate(self) -> None:
        name_ok = bool(self._server_name_input.text().strip())
        zip_ok = bool(self._zip_path_input.text().strip())
        eula_ok = self._eula_checkbox.isChecked()
        self._install_btn.setEnabled(name_ok and zip_ok and eula_ok)

    # ── File browser ───────────────────────────────

    def _browse_zip(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self,
            "Seleccionar servidor Bedrock",
            "",
            "Archivos ZIP (*.zip);;Todos los archivos (*)",
        )
        if path:
            self._zip_path_input.setText(path)

    # ── Install flow ───────────────────────────────

    def _on_install(self) -> None:
        server_name = self._server_name_input.text().strip()
        zip_path = self._zip_path_input.text().strip()
        self._install_zip_path = zip_path

        if not server_name or not zip_path:
            return

        self._install_btn.setEnabled(False)

        server_mgr = ServerManager()
        self._task_manager.run(
            "Instalando servidor...",
            server_mgr.install,
            zip_path,
            server_name,
        )

    def _on_install_done(self, result: object) -> None:
        cfg = Config.instance()
        cfg.set("server_path", str(ServerManager().server_path))
        cfg.save()

        from app.core.server_metadata import mark_installed
        sm = ServerManager()
        mark_installed(sm.executable, self._install_zip_path)

        self.accept()

    def _on_install_error(self, msg: str) -> None:
        pass

    def _on_progress_rejected(self) -> None:
        self._validate()
