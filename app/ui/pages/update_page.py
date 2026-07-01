from __future__ import annotations

import shutil
import tempfile
import zipfile
from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QLabel,
    QMessageBox,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from app.core.process_manager import ProcessManager, ServerState
from app.core.server_manager import ServerManager
from app.core.server_metadata import detect_version, get_version as get_server_version, set_version as set_server_version
from app.core.task_manager import ProgressCallback, TaskManager
from app.core.icons import Icons, BUTTON_SIZE, readonly_banner, title_row
from app.ui.progress_dialog import ProgressDialog

_PROTECTED_PATHS: set[str] = {
    "worlds",
    "resource_packs",
    "behavior_packs",
    "premium_cache",
}

_PROTECTED_FILES: set[str] = {
    "server.properties",
    "allowlist.json",
    "permissions.json",
    "valid_known_packs.json",
}


def _is_protected(path: Path) -> bool:
    parts = path.parts
    if not parts:
        return False
    top = parts[0]
    if top in _PROTECTED_PATHS:
        return True
    name = path.name
    if name in _PROTECTED_FILES:
        return True
    return False


class UpdatePage(QWidget):
    def __init__(
        self, process_manager: ProcessManager,
        server_manager: ServerManager,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._pm = process_manager
        self._sm = server_manager
        self._server_state: ServerState = ServerState.STOPPED
        self._zip_path: str | None = None

        self._task_mgr = TaskManager(self)
        self._select_btn: QPushButton | None = None
        self._update_btn: QPushButton | None = None

        self._pm.state_changed.connect(self._on_state_changed)

        self._setup_ui()
        self._refresh_version()
        self._on_state_changed(self._pm.state)

    # ── Properties ──────────────────────────────────

    @property
    def _server_path(self) -> Path:
        return self._sm.server_path

    # ── Version ─────────────────────────────────────

    def _detect_version(self) -> str:
        return get_server_version()

    def _refresh_version(self) -> None:
        self._version_value.setText(get_server_version())

    # ── UI ──────────────────────────────────────────

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
            Icons.UPDATE, "Actualizaci\u00f3n",
            "Actualiza el servidor Bedrock a una versi\u00f3n m\u00e1s "
            "reciente conservando la configuraci\u00f3n y los mundos.",
        ))

        # ── Read-only banner ──────────────────────────
        self._read_only_banner = readonly_banner(
            "Det\u00e9n el servidor para utilizar este m\u00f3dulo."
        )
        content_layout.addWidget(self._read_only_banner)

        # ── Main card ─────────────────────────────────
        card = QFrame()
        card.setObjectName("card")
        card_layout = QVBoxLayout(card)
        card_layout.setContentsMargins(16, 16, 16, 16)
        card_layout.setSpacing(14)

        # ── Installed version ─────────────────────────
        ver_section = QFrame()
        ver_section.setObjectName("card")
        ver_layout = QVBoxLayout(ver_section)
        ver_layout.setContentsMargins(12, 10, 12, 10)
        ver_layout.setSpacing(4)

        ver_label = QLabel("Versi\u00f3n instalada")
        ver_label.setObjectName("fieldLabel")
        ver_layout.addWidget(ver_label)

        self._version_value = QLabel("\u2014")
        self._version_value.setObjectName("uptimeValue")
        ver_layout.addWidget(self._version_value)

        card_layout.addWidget(ver_section)

        # ── File selection ────────────────────────────
        file_section = QFrame()
        file_section.setObjectName("card")
        file_layout = QVBoxLayout(file_section)
        file_layout.setContentsMargins(12, 10, 12, 10)
        file_layout.setSpacing(8)

        file_label = QLabel("Archivo de actualizaci\u00f3n")
        file_label.setObjectName("fieldLabel")
        file_layout.addWidget(file_label)

        self._file_info = QLabel("Ning\u00fan archivo seleccionado.")
        self._file_info.setObjectName("optionReadOnlyLabel")
        self._file_info.setWordWrap(True)
        file_layout.addWidget(self._file_info)

        self._select_btn = QPushButton("Seleccionar server.zip")
        self._select_btn.setIcon(Icons.FOLDER)
        self._select_btn.setIconSize(BUTTON_SIZE)
        self._select_btn.setObjectName("btnSecondary")
        self._select_btn.clicked.connect(self._select_zip)
        file_layout.addWidget(self._select_btn, 0, Qt.AlignLeft)

        card_layout.addWidget(file_section)

        # ── Update button ─────────────────────────────
        btn_row = QHBoxLayout()
        btn_row.setSpacing(8)

        self._update_btn = QPushButton("Actualizar servidor")
        self._update_btn.setIcon(Icons.UPDATE)
        self._update_btn.setIconSize(BUTTON_SIZE)
        self._update_btn.setObjectName("btnPrimary")
        self._update_btn.clicked.connect(self._do_update)
        self._update_btn.setEnabled(False)
        btn_row.addWidget(self._update_btn)

        btn_row.addStretch()

        card_layout.addLayout(btn_row)

        content_layout.addWidget(card)
        content_layout.addStretch()

        scroll.setWidget(content)
        layout.addWidget(scroll, 1)

    # ── ZIP selection ───────────────────────────────

    def _select_zip(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Seleccionar actualizaci\u00f3n",
            "",
            "Archivo ZIP (*.zip);;Todos los archivos (*)",
        )
        if not path:
            return

        if not self._validate_zip(path):
            QMessageBox.warning(
                self,
                "ZIP no v\u00e1lido",
                "El archivo seleccionado no corresponde a un "
                "servidor Bedrock v\u00e1lido.\n\n"
                "Aseg\u00farate de seleccionar el ZIP oficial de "
                "Minecraft Bedrock Server.",
            )
            return

        self._zip_path = path
        self._file_info.setText(Path(path).name)
        self._update_btn.setEnabled(True)

    def _validate_zip(self, path: str) -> bool:
        required = {"bedrock_server.exe", "server.properties"}
        found: set[str] = set()
        try:
            with zipfile.ZipFile(path, "r") as zf:
                for name in zf.namelist():
                    stem = Path(name).name
                    if stem in required:
                        found.add(stem)
                        if found == required:
                            return True
        except (zipfile.BadZipFile, OSError):
            return False
        return False

    # ── Update ──────────────────────────────────────

    def _do_update(self) -> None:
        if not self._zip_path:
            return

        dlg = ProgressDialog(self)
        dlg.bind(self._task_mgr)
        self._task_mgr.run(
            "Actualizando servidor...",
            self._do_update_task,
            self._zip_path,
        )
        dlg.exec()

        self._refresh_version()
        self._zip_path = None
        self._file_info.setText("Ning\u00fan archivo seleccionado.")
        self._update_btn.setEnabled(False)

        QMessageBox.information(
            self,
            "Actualizaci\u00f3n completada",
            "Servidor actualizado correctamente.",
        )

    def _do_update_task(
        self, zip_path: str, progress: ProgressCallback,
    ) -> None:
        path = Path(zip_path)

        # Count files to update
        total = 0
        to_copy: list[str] = []
        with zipfile.ZipFile(str(path), "r") as zf:
            for name in zf.namelist():
                p = Path(name)
                if p.is_absolute() or ".." in p.parts:
                    continue
                if _is_protected(p):
                    continue
                if name.endswith("/"):
                    continue
                total += 1
                to_copy.append(name)

        if total == 0:
            progress(0, 1, "Actualizando...", "No hay archivos que copiar")
            return

        # Extract to temp
        progress(0, total, "Extrayendo archivos...", "Preparando...")
        temp_dir = Path(tempfile.mkdtemp(dir=str(self._server_path.parent)))
        try:
            with zipfile.ZipFile(str(path), "r") as zf:
                for i, name in enumerate(to_copy):
                    progress(i, total, "Extrayendo...", name)
                    src_info = zf.getinfo(name)
                    target = temp_dir / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    zf.extract(src_info, temp_dir)

            # Copy to server
            for i, name in enumerate(to_copy):
                progress(i, total, "Copiando archivos...", name)
                src = temp_dir / name
                dst = self._server_path / name
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(str(src), str(dst))

            # Detect and store new version
            new_ver = detect_version(
                self._server_path / "bedrock_server.exe", path
            )
            set_server_version(new_ver)

            progress(total, total, "Finalizando...", "Actualizaci\u00f3n completada.")
        except Exception:
            raise
        finally:
            if temp_dir.is_dir():
                shutil.rmtree(str(temp_dir), ignore_errors=True)

    # ── State management ─────────────────────────────

    def _on_state_changed(self, state: ServerState) -> None:
        self._server_state = state
        running = state == ServerState.RUNNING

        self._read_only_banner.setVisible(
            running or state == ServerState.STARTING
        )

        enabled = not running
        if self._select_btn:
            self._select_btn.setEnabled(enabled)
        if self._update_btn:
            self._update_btn.setEnabled(enabled and self._zip_path is not None)
