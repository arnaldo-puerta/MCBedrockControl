from __future__ import annotations

import gzip
import re
import shutil
import struct
import tempfile
import zipfile
from datetime import datetime
from pathlib import Path
from typing import Any

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QPixmap
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
from app.core.icons import Icons, BUTTON_SIZE, readonly_banner, title_row
from app.core.task_manager import (
    ProgressCallback, TaskManager,
)
from app.ui.progress_dialog import ProgressDialog


def _sanitize_folder_name(name: str) -> str:
    sanitized = re.sub(r'[<>:"/\\|?*]', "", name)
    sanitized = sanitized.strip().rstrip(". ")
    return sanitized if sanitized else "world"


def _format_size(size: int) -> str:
    if size < 1024:
        return f"{size} B"
    elif size < 1024 ** 2:
        return f"{size / 1024:.1f} KB"
    elif size < 1024 ** 3:
        return f"{size / 1024 ** 2:.1f} MB"
    return f"{size / 1024 ** 3:.2f} GB"


def _try_read_level_dat(path: Path) -> dict[str, Any]:
    raw: bytes | None = None
    try:
        with gzip.open(str(path), "rb") as f:
            raw = f.read()
    except Exception:
        try:
            raw = path.read_bytes()
        except Exception:
            return {}
    if not raw:
        return {}
    pos = 0

    def _read_byte() -> int:
        nonlocal pos
        b = raw[pos]
        pos += 1
        return b

    def _read_short() -> int:
        nonlocal pos
        v, = struct.unpack_from(">h", raw, pos)
        pos += 2
        return v

    def _read_int() -> int:
        nonlocal pos
        v, = struct.unpack_from(">i", raw, pos)
        pos += 4
        return v

    def _read_long() -> int:
        nonlocal pos
        v, = struct.unpack_from(">q", raw, pos)
        pos += 8
        return v

    def _read_string() -> str:
        length = _read_short()
        s = raw[pos:pos + length].decode("utf-8", errors="replace")
        pos += length
        return s

    def _read_payload(tag_type: int) -> Any:
        if tag_type == 0:
            return None
        elif tag_type == 1:
            return _read_byte()
        elif tag_type == 2:
            return _read_short()
        elif tag_type == 3:
            return _read_int()
        elif tag_type == 4:
            return _read_long()
        elif tag_type == 5:
            v, = struct.unpack_from(">f", raw, pos)
            pos += 4
            return v
        elif tag_type == 6:
            v, = struct.unpack_from(">d", raw, pos)
            pos += 8
            return v
        elif tag_type == 7:
            length = _read_int()
            arr = raw[pos:pos + length]
            pos += length
            return list(arr)
        elif tag_type == 8:
            return _read_string()
        elif tag_type == 9:
            list_type = _read_byte()
            list_len = _read_int()
            return [_read_payload(list_type) for _ in range(list_len)]
        elif tag_type == 10:
            compound: dict[str, Any] = {}
            while True:
                t = _read_byte()
                if t == 0:
                    break
                name = _read_string()
                compound[name] = _read_payload(t)
            return compound
        elif tag_type == 11:
            length = _read_int()
            return [_read_int() for _ in range(length)]
        elif tag_type == 12:
            length = _read_int()
            return [_read_long() for _ in range(length)]
        return None

    try:
        root_type = _read_byte()
        if root_type != 10:
            return {}
        _read_string()
        result = _read_payload(10)
        if isinstance(result, dict):
            return result
    except Exception:
        pass
    return {}


def _flatten_world_structure(temp_dir: Path) -> Path | None:
    for p in temp_dir.rglob("level.dat"):
        if p.is_file():
            parent = p.parent
            if parent == temp_dir:
                return temp_dir
            flat = temp_dir / "_world_content"
            parent.rename(flat)
            for item in list(temp_dir.iterdir()):
                if item != flat:
                    if item.is_dir():
                        shutil.rmtree(str(item))
                    else:
                        item.unlink()
            return flat
    return None


class WorldsPage(QWidget):
    world_changed = Signal()

    def __init__(
        self, process_manager: ProcessManager,
        server_manager: ServerManager,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._pm = process_manager
        self._sm = server_manager
        self._server_state: ServerState = ServerState.STOPPED

        self._task_mgr = TaskManager(self)

        self._import_btn: QPushButton | None = None
        self._import_folder_btn: QPushButton | None = None
        self._export_btn: QPushButton | None = None
        self._reload_btn: QPushButton | None = None

        self._info_card: QFrame | None = None
        self._empty_label: QLabel | None = None

        self._world_icon_label = QLabel()
        self._world_name_label = QLabel()
        self._world_folder_label = QLabel()
        self._world_size_label = QLabel()
        self._world_date_label = QLabel()
        self._world_seed_label = QLabel()
        self._world_version_label = QLabel()

        self._pm.state_changed.connect(self._on_state_changed)

        self._setup_ui()
        self._load_world_info()

    # ── Properties ──────────────────────────────────

    @property
    def _server_path(self) -> Path:
        return self._sm.server_path

    @property
    def _worlds_dir(self) -> Path:
        return self._server_path / "worlds"

    @property
    def _level_name(self) -> str:
        props = self._sm.read_properties()
        return props.get("level-name", "Bedrock level")

    @property
    def _current_world_path(self) -> Path:
        return self._worlds_dir / self._level_name

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
        content_layout.setContentsMargins(24, 20, 24, 24)
        content_layout.setSpacing(14)

        # ── Page title ──────────────────────────────
        content_layout.addWidget(title_row(
            Icons.WORLDS, "Mundos",
            "Administra el mundo activo del servidor.",
        ))

        # ── Read-only banner ──────────────────────────
        self._read_only_banner = readonly_banner(
            "Det\u00e9n el servidor para importar o exportar mundos."
        )
        content_layout.addWidget(self._read_only_banner)

        # ── Toolbar ──────────────────────────────────
        toolbar = QFrame()
        toolbar.setObjectName("toolbar")
        toolbar_layout = QHBoxLayout(toolbar)
        toolbar_layout.setContentsMargins(0, 0, 0, 0)
        toolbar_layout.setSpacing(8)

        self._import_btn = QPushButton("Importar Mundo")
        self._import_btn.setIcon(Icons.IMPORT)
        self._import_btn.setIconSize(BUTTON_SIZE)
        self._import_btn.setObjectName("btnPrimary")
        self._import_btn.clicked.connect(
            lambda: self._import_world(False)
        )

        self._import_folder_btn = QPushButton("Importar Carpeta")
        self._import_folder_btn.setIcon(Icons.FOLDER)
        self._import_folder_btn.setIconSize(BUTTON_SIZE)
        self._import_folder_btn.setObjectName("btnPrimary")
        self._import_folder_btn.clicked.connect(
            lambda: self._import_world(True)
        )

        self._export_btn = QPushButton("Exportar Mundo")
        self._export_btn.setIcon(Icons.EXPORT)
        self._export_btn.setIconSize(BUTTON_SIZE)
        self._export_btn.setObjectName("btnSecondary")
        self._export_btn.clicked.connect(self._export_world)

        toolbar_layout.addWidget(self._import_btn)
        toolbar_layout.addWidget(self._import_folder_btn)
        toolbar_layout.addWidget(self._export_btn)
        toolbar_layout.addStretch()

        self._reload_btn = QPushButton("Recargar")
        self._reload_btn.setIcon(Icons.RELOAD)
        self._reload_btn.setIconSize(BUTTON_SIZE)
        self._reload_btn.setObjectName("btnSecondary")
        self._reload_btn.clicked.connect(self._load_world_info)
        toolbar_layout.addWidget(self._reload_btn)

        content_layout.addWidget(toolbar)

        # ── World info card ──────────────────────────
        self._info_card = QFrame()
        self._info_card.setObjectName("cardHighlight")
        info_card_layout = QHBoxLayout(self._info_card)
        info_card_layout.setContentsMargins(20, 18, 20, 18)
        info_card_layout.setSpacing(24)

        # Icon frame
        icon_frame = QFrame()
        icon_frame.setObjectName("worldsIconFrame")
        icon_frame.setFixedSize(148, 148)
        icon_layout = QVBoxLayout(icon_frame)
        icon_layout.setContentsMargins(0, 0, 0, 0)

        self._world_icon_label.setObjectName("worldsIcon")
        self._world_icon_label.setFixedSize(148, 148)
        self._world_icon_label.setAlignment(Qt.AlignCenter)
        icon_layout.addWidget(self._world_icon_label)
        info_card_layout.addWidget(icon_frame)

        # Fields
        fields_frame = QFrame()
        fields_frame.setObjectName("worldsFieldsFrame")
        fields_layout = QVBoxLayout(fields_frame)
        fields_layout.setContentsMargins(0, 0, 0, 0)
        fields_layout.setSpacing(6)

        def _make_row(emoji: str, label: str, widget: QLabel) -> QWidget:
            row = QWidget()
            row.setObjectName("worldsFieldRow")
            row_layout = QHBoxLayout(row)
            row_layout.setContentsMargins(0, 0, 0, 0)
            row_layout.setSpacing(8)
            icon_lbl = QLabel(emoji)
            icon_lbl.setObjectName("fieldIcon")
            icon_lbl.setFixedWidth(22)
            lbl = QLabel(label)
            lbl.setObjectName("fieldLabel")
            lbl.setFixedWidth(90)
            widget.setObjectName("fieldValue")
            widget.setWordWrap(True)
            row_layout.addWidget(icon_lbl)
            row_layout.addWidget(lbl)
            row_layout.addWidget(widget, 1)
            row_layout.addStretch()
            return row

        info_fields: list[tuple[str, str, QLabel]] = [
            ("\U0001f3ae", "Nombre:", self._world_name_label),
            ("\U0001f4c1", "Carpeta:", self._world_folder_label),
            ("\U0001f4ca", "Tama\u00f1o:", self._world_size_label),
            ("\U0001f4c5", "Modificado:", self._world_date_label),
            ("\U0001f331", "Seed:", self._world_seed_label),
            ("\U0001f4e6", "Versi\u00f3n:", self._world_version_label),
        ]
        for emoji, label, widget in info_fields:
            fields_layout.addWidget(_make_row(emoji, label, widget))

        fields_layout.addStretch()
        info_card_layout.addWidget(fields_frame, 1)

        content_layout.addWidget(self._info_card)

        # ── Empty state ──────────────────────────────
        self._empty_label = QLabel(
            "No hay mundo instalado.\n\n"
            "Importa un mundo usando los botones superiores."
        )
        self._empty_label.setObjectName("emptyLabel")
        self._empty_label.setAlignment(Qt.AlignCenter)
        self._empty_label.setWordWrap(True)
        self._empty_label.hide()
        content_layout.addWidget(self._empty_label, 1)

        # ── Info banner ──────────────────────────────
        banner = QFrame()
        banner.setObjectName("banner")
        banner_layout = QHBoxLayout(banner)
        banner_layout.setContentsMargins(16, 12, 16, 12)
        banner_layout.setSpacing(10)

        banner_icon = QLabel("\u2139\ufe0f")
        banner_icon.setObjectName("bannerIcon")
        banner_layout.addWidget(banner_icon)

        banner_text = QLabel(
            "Solo puede existir un mundo instalado. Al importar un nuevo "
            "mundo, el actual ser\u00e1 reemplazado. "
            "El servidor debe estar detenido para realizar cambios."
        )
        banner_text.setObjectName("bannerText")
        banner_text.setWordWrap(True)
        banner_layout.addWidget(banner_text, 1)

        content_layout.addWidget(banner)

        # ── Quick actions ────────────────────────────
        qa_card = QFrame()
        qa_card.setObjectName("card")
        qa_layout = QHBoxLayout(qa_card)
        qa_layout.setContentsMargins(16, 14, 16, 14)
        qa_layout.setSpacing(12)

        actions: list[tuple[str, str, str]] = [
            (
                "\U0001f4e5",
                "Importar Mundo",
                "Importa un archivo .mcworld.",
            ),
            (
                "\U0001f4c2",
                "Importar Carpeta",
                "Importa una carpeta que contenga\nun mundo v\u00e1lido.",
            ),
            (
                "\U0001f4e4",
                "Exportar Mundo",
                "Exporta el mundo actual\ncomo .mcworld.",
            ),
            (
                "\U0001f504",
                "Recargar",
                "Vuelve a leer la informaci\u00f3n\ndel mundo instalado.",
            ),
        ]

        for emoji, title_txt, desc in actions:
            card = QFrame()
            card.setObjectName("worldsActionCard")
            card_layout = QVBoxLayout(card)
            card_layout.setContentsMargins(12, 12, 12, 12)
            card_layout.setSpacing(6)
            card_layout.setAlignment(Qt.AlignCenter)

            icon_lbl = QLabel(emoji)
            icon_lbl.setObjectName("worldsActionIcon")
            icon_lbl.setAlignment(Qt.AlignCenter)
            card_layout.addWidget(icon_lbl)

            title_lbl = QLabel(title_txt)
            title_lbl.setObjectName("worldsActionTitle")
            title_lbl.setAlignment(Qt.AlignCenter)
            card_layout.addWidget(title_lbl)

            desc_lbl = QLabel(desc)
            desc_lbl.setObjectName("worldsActionDesc")
            desc_lbl.setAlignment(Qt.AlignCenter)
            desc_lbl.setWordWrap(True)
            card_layout.addWidget(desc_lbl)

            qa_layout.addWidget(card, 1)

        content_layout.addWidget(qa_card)
        content_layout.addStretch()

        scroll.setWidget(content)
        layout.addWidget(scroll, 1)

    def _load_world_info(self) -> None:
        world_path = self._current_world_path
        props = self._sm.read_properties()
        level_name = props.get("level-name", "Bedrock level")

        if not world_path.is_dir():
            self._info_card.setVisible(False)
            self._empty_label.show()
            return

        self._info_card.setVisible(True)
        self._empty_label.hide()

        # Name
        levelname_file = world_path / "levelname.txt"
        display_name = level_name
        if levelname_file.is_file():
            display_name = levelname_file.read_text(
                encoding="utf-8-sig",
            ).strip()
        self._world_name_label.setText(display_name or level_name)

        # Folder
        self._world_folder_label.setText(world_path.name)

        # Size
        total_size = 0
        for p in world_path.rglob("*"):
            if p.is_file():
                total_size += p.stat().st_size
        self._world_size_label.setText(_format_size(total_size))

        # Date
        latest = 0.0
        for p in world_path.rglob("*"):
            if p.is_file():
                mtime = p.stat().st_mtime
                if mtime > latest:
                    latest = mtime
        if latest > 0:
            self._world_date_label.setText(
                datetime.fromtimestamp(latest).strftime(
                    "%Y-%m-%d  %H:%M:%S",
                ),
            )
        else:
            self._world_date_label.setText("\u2014")

        # Seed & Version
        level_dat = world_path / "level.dat"
        seed_str = "\u2014"
        version_str = "\u2014"
        if level_dat.is_file():
            data = _try_read_level_dat(level_dat)
            inner = data.get("Data", {}) if isinstance(data, dict) else {}
            seed_raw = inner.get("RandomSeed", None)
            if seed_raw is not None:
                seed_str = str(seed_raw)
            storage = inner.get("StorageVersion", None)
            if storage is not None:
                version_str = str(storage)
        self._world_seed_label.setText(seed_str)
        self._world_version_label.setText(version_str)

        # Icon
        icon_path = world_path / "world_icon.jpeg"
        if icon_path.is_file():
            pixmap = QPixmap(str(icon_path))
            self._world_icon_label.setPixmap(
                pixmap.scaled(
                    148, 148,
                    Qt.KeepAspectRatio,
                    Qt.SmoothTransformation,
                ),
            )
        else:
            self._world_icon_label.setText("\U0001f3ae")
            self._world_icon_label.setStyleSheet(
                "font-size: 56px; color: #555555;"
            )

    # ── Import ──────────────────────────────────────

    def _import_world(self, folder_mode: bool = False) -> None:
        if self._server_state == ServerState.RUNNING:
            QMessageBox.information(
                self, "Servidor en ejecuci\u00f3n",
                "Debes detener el servidor antes de importar un mundo.",
            )
            return

        if folder_mode:
            path = QFileDialog.getExistingDirectory(
                self, "Importar carpeta de mundo",
            )
            if not path:
                return
        else:
            path, _ = QFileDialog.getOpenFileName(
                self, "Importar mundo",
                "",
                "Mundos (*.mcworld);;Todos los archivos (*)",
            )
            if not path:
                return

        dlg = ProgressDialog(self)
        dlg.bind(self._task_mgr)
        self._task_mgr.run(
            "Importando mundo...", self._do_import, path,
        )
        dlg.exec()

        self._load_world_info()
        self.world_changed.emit()
        QMessageBox.information(
            self, "Completado",
            "Mundo importado correctamente.",
        )

    def _do_import(
        self, src_path: str,
        progress: ProgressCallback,
    ) -> None:
        src = Path(src_path)
        temp_root = Path(tempfile.mkdtemp())
        try:
            if src.suffix.lower() == ".mcworld":
                with zipfile.ZipFile(str(src), "r") as zf:
                    names = zf.namelist()
                    total = len(names)
                    progress(0, total, "Extrayendo...", "Preparando...")
                    for i, name in enumerate(names):
                        if name.endswith("/"):
                            continue
                        progress(i + 1, total, "Extrayendo...",
                                 Path(name).name or name)
                        target = temp_root / name
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(zf.read(name))
            else:
                items = list(src.rglob("*"))
                total = len(items)
                progress(0, total, "Copiando...", "Preparando...")
                for i, item in enumerate(items):
                    rel = item.relative_to(src)
                    progress(i + 1, total, "Copiando...", str(rel))
                    target = temp_root / rel
                    if item.is_dir():
                        target.mkdir(parents=True, exist_ok=True)
                    else:
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(item.read_bytes())

            world_content = _flatten_world_structure(temp_root)
            if world_content is None:
                raise RuntimeError(
                    "El elemento seleccionado no es un mundo "
                    "v\u00e1lido de Minecraft Bedrock.",
                )

            levelname_file = world_content / "levelname.txt"
            world_name = (
                levelname_file.read_text(encoding="utf-8-sig").strip()
                if levelname_file.is_file()
                else src.stem
            )
            world_folder_name = _sanitize_folder_name(world_name)
            final_path = self._worlds_dir / world_folder_name

            old_world = self._current_world_path
            if old_world.is_dir() and old_world != final_path:
                progress(0, 1, "Eliminando mundo anterior...", old_world.name)
                shutil.rmtree(str(old_world))
                if old_world.is_dir():
                    raise RuntimeError(
                        "No se pudo eliminar el mundo anterior."
                    )

            final_path.parent.mkdir(parents=True, exist_ok=True)
            world_content.rename(final_path)

            props = self._sm.read_properties()
            props["level-name"] = world_folder_name
            self._sm.write_properties(props)

        except Exception:
            shutil.rmtree(str(temp_root), ignore_errors=True)
            raise

        shutil.rmtree(str(temp_root), ignore_errors=True)

    # ── Export ──────────────────────────────────────

    def _export_world(self) -> None:
        if self._server_state == ServerState.RUNNING:
            QMessageBox.information(
                self, "Servidor en ejecuci\u00f3n",
                "Debes detener el servidor antes de exportar el mundo.",
            )
            return

        world_path = self._current_world_path
        if not world_path.is_dir():
            QMessageBox.information(
                self, "Sin mundo",
                "No hay ning\u00fan mundo instalado para exportar.",
            )
            return

        default_name = f"{world_path.name}.mcworld"
        save_path, _ = QFileDialog.getSaveFileName(
            self, "Exportar mundo",
            default_name,
            "Mundos (*.mcworld);;Todos los archivos (*)",
        )
        if not save_path:
            return

        dlg = ProgressDialog(self)
        dlg.bind(self._task_mgr)
        self._task_mgr.run(
            "Exportando mundo...", self._do_export, save_path,
        )
        dlg.exec()

    def _do_export(
        self, save_path: str,
        progress: ProgressCallback,
    ) -> None:
        world_path = self._current_world_path
        items = list(world_path.rglob("*"))
        files = [p for p in items if p.is_file()]
        total = len(files)

        progress(0, total, "Comprimiendo...", "Preparando...")

        with zipfile.ZipFile(
            save_path, "w", zipfile.ZIP_DEFLATED,
        ) as zf:
            for i, f in enumerate(files):
                rel = f.relative_to(world_path)
                progress(i + 1, total, "Comprimiendo...", str(rel))
                zf.write(str(f), str(rel))

    # ── State management ─────────────────────────────

    def _on_state_changed(self, state: ServerState) -> None:
        self._server_state = state
        running = state == ServerState.RUNNING

        self._read_only_banner.setVisible(
            running or state == ServerState.STARTING
        )

        for btn in (
            self._import_btn, self._import_folder_btn,
            self._export_btn, self._reload_btn,
        ):
            if btn:
                btn.setEnabled(not running)
