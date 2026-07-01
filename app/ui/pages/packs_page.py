from __future__ import annotations

import json
import re
import shutil
import tempfile
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QMessageBox,
    QPushButton,
    QScrollArea,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from app.core.process_manager import ProcessManager, ServerState
from app.core.server_manager import ServerManager
from app.core.task_manager import (
    ProgressCallback, TaskManager, _CancelledError,
)
from app.core.icons import Icons, BUTTON_SIZE, readonly_banner, title_row
from app.ui.progress_dialog import ProgressDialog

_RESOURCE_TYPE = "resources"
_BEHAVIOR_TYPE = "data"


def _read_json(path: Path) -> Any:
    if path.is_file():
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return None
    return None


def _write_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(data, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def _version_str(ver: list[int] | None) -> str:
    if not ver or not isinstance(ver, list):
        return "?"
    return ".".join(str(v) for v in ver)


def _parse_manifest(manifest: dict[str, Any]) -> tuple[str, str, list[int], str, str]:
    header = manifest.get("header", {})
    name = header.get("name", "(sin nombre)")
    description = header.get("description", "")
    uuid = header.get("uuid", "")
    version = header.get("version", [])
    modules = manifest.get("modules", [])
    pack_type = modules[0].get("type", "") if modules else ""
    return name, description, uuid, version, pack_type


def _sanitize_name(name: str) -> str:
    sanitized = re.sub(r'[<>:"/\\|?*]', "", name)
    sanitized = sanitized.strip()
    sanitized = sanitized.rstrip(". ")
    return sanitized if sanitized else "pack"


def _detect_packs_in_zip(
    zip_path: str | Path,
) -> list[tuple[str, str, str]]:
    results: list[tuple[str, str, str]] = []
    zip_stem = Path(zip_path).stem

    with zipfile.ZipFile(str(zip_path), "r") as zf:
        names = zf.namelist()

        for name in names:
            if not name.endswith("manifest.json"):
                continue
            info = zf.getinfo(name)
            if info.is_dir():
                continue

            try:
                manifest = json.loads(zf.read(name).decode("utf-8"))
            except (json.JSONDecodeError, UnicodeDecodeError, KeyError):
                continue

            _, _, _, _, pack_type = _parse_manifest(manifest)
            if not pack_type:
                continue

            pp = PurePosixPath(name)
            parent_prefix = (
                pp.parent.as_posix() + "/"
                if pp.parent != PurePosixPath(".")
                else ""
            )

            dir_name = (
                pp.parent.name
                if pp.parent != PurePosixPath(".")
                else zip_stem
            )

            results.append((pack_type, dir_name, parent_prefix))

    return results


def _world_packs_file(world_dir: Path, pack_type: str) -> Path:
    if pack_type == _RESOURCE_TYPE:
        return world_dir / "world_resource_packs.json"
    return world_dir / "world_behavior_packs.json"


def _read_world_packs(world_dir: Path, pack_type: str) -> list[dict[str, Any]]:
    path = _world_packs_file(world_dir, pack_type)
    data = _read_json(path)
    if isinstance(data, list):
        return data
    return []


def _save_world_packs(
    world_dir: Path, pack_type: str,
    packs: list[dict[str, Any]],
) -> None:
    _write_json(_world_packs_file(world_dir, pack_type), packs)


def _read_valid_known(server_path: Path) -> list[dict[str, Any]]:
    path = server_path / "valid_known_packs.json"
    data = _read_json(path)
    if isinstance(data, list):
        return data
    return []


def _save_valid_known(server_path: Path, packs: list[dict[str, Any]]) -> None:
    _write_json(server_path / "valid_known_packs.json", packs)


def _level_name(server_mgr: ServerManager) -> str:
    props = server_mgr.read_properties()
    return props.get("level-name", "Bedrock level")


def _debug_zip_contents(
    path: Path, names_in_zip: list[str], prefix: str,
) -> None:
    import logging
    logging.basicConfig(level=logging.DEBUG)
    log = logging.getLogger("PacksPage")
    log.debug("=== DEBUG ZIP ===")
    log.debug("Archivo: %s", path)
    log.debug("Prefijo usado: %r", prefix)
    log.debug("Total archivos en ZIP: %d", len(names_in_zip))
    for name in names_in_zip:
        log.debug("  %s  (startswith prefix=%s)", name, name.startswith(prefix))
    log.debug("=== FIN DEBUG ZIP ===")


class PacksPage(QWidget):
    def __init__(
        self, process_manager: ProcessManager,
        server_manager: ServerManager,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._pm = process_manager
        self._sm = server_manager
        self._server_state: ServerState = ServerState.STOPPED
        self._loading: bool = True

        self._res_table: QTableWidget | None = None
        self._beh_table: QTableWidget | None = None
        self._install_btn: QPushButton | None = None
        self._remove_btn: QPushButton | None = None
        self._reload_btn: QPushButton | None = None

        self._task_mgr = TaskManager(self)

        self._pm.state_changed.connect(self._on_state_changed)

        self._setup_ui()
        self._scan_packs()
        self._loading = False
        self._on_state_changed(self._pm.state)

    # ── Properties ──────────────────────────────────

    @property
    def _server_path(self) -> Path:
        return self._sm.server_path

    @property
    def _res_dir(self) -> Path:
        return self._server_path / "resource_packs"

    @property
    def _beh_dir(self) -> Path:
        return self._server_path / "behavior_packs"

    @property
    def _world_dir(self) -> Path:
        name = _level_name(self._sm)
        return self._server_path / "worlds" / name

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
            Icons.PACKS, "Packs",
            "Instala y administra Resource Packs y Behavior Packs.",
        ))

        # ── Read-only banner ──────────────────────────
        self._read_only_banner = readonly_banner(
            "Det\u00e9n el servidor para instalar o administrar packs."
        )
        content_layout.addWidget(self._read_only_banner)

        # ── World missing banner ───────────────────────
        self._world_banner = QFrame()
        self._world_banner.setObjectName("infoBanner")
        self._world_banner.setVisible(False)
        wb_layout = QHBoxLayout(self._world_banner)
        wb_layout.setContentsMargins(16, 12, 16, 12)
        wb_layout.setSpacing(12)

        wb_icon = QLabel()
        wb_icon.setPixmap(Icons.WORLDS.pixmap(28, 28))
        wb_icon.setFixedSize(28, 28)
        wb_layout.addWidget(wb_icon)

        wb_text = QLabel(
            "No hay ning\u00fan mundo disponible.\n"
            "Inicia el servidor una vez o importa un mundo "
            "para poder activar packs."
        )
        wb_text.setObjectName("bannerText")
        wb_text.setWordWrap(True)
        wb_layout.addWidget(wb_text, 1)

        content_layout.addWidget(self._world_banner)

        # ── Toolbar ──────────────────────────────────
        toolbar = QFrame()
        toolbar.setObjectName("toolbar")
        toolbar_layout = QHBoxLayout(toolbar)
        toolbar_layout.setContentsMargins(0, 0, 0, 0)
        toolbar_layout.setSpacing(8)

        self._install_btn = QPushButton("Instalar")
        self._install_btn.setIcon(Icons.IMPORT)
        self._install_btn.setIconSize(BUTTON_SIZE)
        self._install_btn.setObjectName("btnPrimary")
        self._install_btn.clicked.connect(self._install_pack)
        toolbar_layout.addWidget(self._install_btn)

        self._remove_btn = QPushButton("Eliminar")
        self._remove_btn.setIcon(Icons.DELETE)
        self._remove_btn.setIconSize(BUTTON_SIZE)
        self._remove_btn.setObjectName("btnDanger")
        self._remove_btn.clicked.connect(self._remove_pack)
        toolbar_layout.addWidget(self._remove_btn)

        self._reload_btn = QPushButton("Recargar")
        self._reload_btn.setIcon(Icons.RELOAD)
        self._reload_btn.setIconSize(BUTTON_SIZE)
        self._reload_btn.setObjectName("btnSecondary")
        self._reload_btn.clicked.connect(self._scan_packs)
        toolbar_layout.addWidget(self._reload_btn)

        toolbar_layout.addStretch()
        content_layout.addWidget(toolbar)

        # ── Resource Packs ───────────────────────────
        res_card = QFrame()
        res_card.setObjectName("card")
        res_layout = QVBoxLayout(res_card)
        res_layout.setContentsMargins(16, 12, 16, 12)
        res_layout.setSpacing(8)

        res_title = QLabel("Resource Packs")
        res_title.setObjectName("sectionTitle")
        res_layout.addWidget(res_title)

        self._res_table = QTableWidget(0, 4)
        self._res_table.setObjectName("dataTable")
        self._res_table.setHorizontalHeaderLabels([
            "Activo", "Nombre", "Versi\u00f3n", "UUID",
        ])
        self._res_table.horizontalHeader().setStretchLastSection(True)
        self._res_table.horizontalHeader().setSectionResizeMode(
            1, QHeaderView.Stretch
        )
        self._res_table.verticalHeader().setVisible(False)
        self._res_table.setSelectionBehavior(QTableWidget.SelectRows)
        self._res_table.setSelectionMode(QTableWidget.ExtendedSelection)
        self._res_table.setEditTriggers(QTableWidget.NoEditTriggers)
        self._res_table.itemChanged.connect(self._on_item_changed)
        res_layout.addWidget(self._res_table, 1)

        content_layout.addWidget(res_card)

        # ── Behavior Packs ───────────────────────────
        beh_card = QFrame()
        beh_card.setObjectName("card")
        beh_layout = QVBoxLayout(beh_card)
        beh_layout.setContentsMargins(16, 12, 16, 12)
        beh_layout.setSpacing(8)

        beh_title = QLabel("Behavior Packs")
        beh_title.setObjectName("sectionTitle")
        beh_layout.addWidget(beh_title)

        self._beh_table = QTableWidget(0, 4)
        self._beh_table.setObjectName("dataTable")
        self._beh_table.setHorizontalHeaderLabels([
            "Activo", "Nombre", "Versi\u00f3n", "UUID",
        ])
        self._beh_table.horizontalHeader().setStretchLastSection(True)
        self._beh_table.horizontalHeader().setSectionResizeMode(
            1, QHeaderView.Stretch
        )
        self._beh_table.verticalHeader().setVisible(False)
        self._beh_table.setSelectionBehavior(QTableWidget.SelectRows)
        self._beh_table.setSelectionMode(QTableWidget.ExtendedSelection)
        self._beh_table.setEditTriggers(QTableWidget.NoEditTriggers)
        self._beh_table.itemChanged.connect(self._on_item_changed)
        beh_layout.addWidget(self._beh_table, 1)

        content_layout.addWidget(beh_card)
        content_layout.addStretch()

        scroll.setWidget(content)
        layout.addWidget(scroll, 1)

    # ── Scanning & sync ─────────────────────────────

    def _scan_packs(self) -> None:
        known = _read_valid_known(self._server_path)
        known = self._prune_orphans(known)
        _save_valid_known(self._server_path, known)
        self._populate_table(self._res_table, known, _RESOURCE_TYPE)
        self._populate_table(self._beh_table, known, _BEHAVIOR_TYPE)
        self._check_world_exists()

    def _world_exists(self) -> bool:
        return self._sm.world_exists

    def _check_world_exists(self) -> None:
        exists = self._world_exists()
        self._world_banner.setVisible(not exists)
        self._update_table_flags()

    def _update_table_flags(self) -> None:
        running = self._server_state in (ServerState.RUNNING, ServerState.STARTING)
        world_ok = self._world_exists()
        for table in [self._res_table, self._beh_table]:
            if not table:
                continue
            can_toggle = not running and world_ok
            flags = (
                Qt.ItemIsUserCheckable
                | Qt.ItemIsEnabled
                | Qt.ItemIsSelectable
            ) if can_toggle else Qt.ItemIsSelectable
            table.blockSignals(True)
            for row in range(table.rowCount()):
                item = table.item(row, 0)
                if item:
                    item.setFlags(flags)
            table.blockSignals(False)

    def _prune_orphans(
        self, known: list[dict[str, Any]],
    ) -> list[dict[str, Any]]:
        cleaned: list[dict[str, Any]] = []
        for entry in known:
            pack_path = entry.get("path", "")
            if not pack_path:
                continue
            manifest_path = self._server_path / pack_path / "manifest.json"
            if manifest_path.is_file():
                cleaned.append(entry)
        return cleaned

    def _populate_table(
        self, table: QTableWidget | None,
        known: list[dict[str, Any]], expected_type: str,
    ) -> None:
        if not table:
            return
        table.setRowCount(0)
        table.blockSignals(True)

        prefix = (
            "resource_packs/"
            if expected_type == _RESOURCE_TYPE
            else "behavior_packs/"
        )
        active = _read_world_packs(self._world_dir, expected_type)
        active_ids: set[str] = set()
        for p in active:
            pid = p.get("pack_id", "")
            if pid:
                active_ids.add(pid)

        entries: list[dict[str, Any]] = []
        for entry in known:
            pack_path = entry.get("path", "")
            if not pack_path.startswith(prefix):
                continue
            uuid = entry.get("uuid", "")
            if not uuid:
                continue

            manifest_dir = self._server_path / pack_path
            manifest_path = manifest_dir / "manifest.json"
            if not manifest_path.is_file():
                continue

            manifest = _read_json(manifest_path)
            if not isinstance(manifest, dict):
                continue

            name, _, _, version, _ = _parse_manifest(manifest)
            entries.append({
                "uuid": uuid,
                "name": name,
                "version": version,
                "active": uuid in active_ids,
            })

        for i, e in enumerate(entries):
            row = table.rowCount()
            table.insertRow(row)

            check = QTableWidgetItem()
            check.setFlags(
                Qt.ItemIsUserCheckable
                | Qt.ItemIsEnabled
                | Qt.ItemIsSelectable
            )
            check.setCheckState(
                Qt.Checked if e["active"] else Qt.Unchecked
            )
            check.setData(Qt.UserRole, e["uuid"])
            check.setData(Qt.UserRole + 1, json.dumps(e["version"]))
            check.setData(Qt.UserRole + 2, expected_type)
            table.setItem(row, 0, check)

            table.setItem(row, 1, QTableWidgetItem(e["name"]))
            table.setItem(row, 2, QTableWidgetItem(
                _version_str(e["version"]))
            )
            table.setItem(row, 3, QTableWidgetItem(e["uuid"]))

        table.blockSignals(False)

    # ── Toggle activation ───────────────────────────

    def _on_item_changed(self, item: QTableWidgetItem) -> None:
        if self._loading:
            return
        if item.column() != 0:
            return

        uuid = item.data(Qt.UserRole)
        version = json.loads(item.data(Qt.UserRole + 1))
        pack_type = item.data(Qt.UserRole + 2)
        checked = item.checkState() == Qt.Checked

        if not uuid:
            return

        active = _read_world_packs(self._world_dir, pack_type)
        if checked:
            if not any(p.get("pack_id") == uuid for p in active):
                active.append({
                    "pack_id": uuid,
                    "version": version,
                })
        else:
            active = [p for p in active if p.get("pack_id") != uuid]

        _save_world_packs(self._world_dir, pack_type, active)

    # ── Install ─────────────────────────────────────

    def _peek_zip_uuids(self, zip_path: str) -> dict[str, str]:
        result: dict[str, str] = {}
        try:
            with zipfile.ZipFile(zip_path, "r") as zf:
                for entry in zf.namelist():
                    if entry.endswith("manifest.json"):
                        try:
                            raw = json.loads(
                                zf.read(entry).decode("utf-8")
                            )
                            hdr = raw.get("header", {})
                            mods = raw.get("modules", [])
                            pt = mods[0].get("type", "") if mods else ""
                            uid = hdr.get("uuid", "")
                            if pt and uid:
                                result[uid] = pt
                        except Exception:
                            continue
        except Exception:
            pass
        return result

    def _install_pack(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Seleccionar pack",
            "",
            "Packs (*.mcpack *.mcaddon);;Todos los archivos (*)",
        )
        if not path:
            return

        # Detect UUIDs and check for existing packs
        uuids = self._peek_zip_uuids(path)
        known = _read_valid_known(self._server_path)
        updates: dict[str, dict[str, Any]] = {}
        for uid in uuids:
            existing = next(
                (e for e in known if e.get("uuid") == uid), None
            )
            if existing:
                manifest_path = (
                    self._server_path
                    / existing["path"]
                    / "manifest.json"
                )
                if manifest_path.is_file():
                    updates[uid] = existing

        if updates:
            reply = QMessageBox.question(
                self,
                "Pack existente",
                "Ya existe un pack instalado con el mismo UUID.\n"
                "\u00bfDeseas reemplazar la versi\u00f3n actual?",
                QMessageBox.Yes | QMessageBox.No,
            )
            if reply != QMessageBox.Yes:
                return

        dlg = ProgressDialog(self)
        dlg.bind(self._task_mgr)
        self._task_mgr.run(
            "Instalando pack...", self._do_install,
            path, updates=updates,
        )
        dlg.exec()

        self._scan_packs()

    def _do_install(
        self, zip_path: str, progress: ProgressCallback,
        updates: dict[str, dict[str, Any]] | None = None,
    ) -> None:
        path = Path(zip_path)
        if not path.is_file():
            raise RuntimeError(f"Archivo no encontrado: {path}")

        if updates is None:
            updates = {}

        suffix = path.suffix.lower()

        if suffix == ".mcpack":
            self._install_single_pack(path, progress, updates)
        elif suffix == ".mcaddon":
            self._install_addon(path, progress, updates)
        else:
            raise RuntimeError(
                "Formato no soportado. Usa .mcpack o .mcaddon."
            )

    def _extract_to_temp(
        self, path: Path, prefix: str,
        target_base: Path, pack_folder: str,
        progress: ProgressCallback,
        all_names: list[str], total: int,
    ) -> Path:
        """Extract pack files to a temp dir and return its path."""
        temp_dir = Path(tempfile.mkdtemp(dir=str(target_base)))
        with zipfile.ZipFile(str(path), "r") as zf:
            for i, name in enumerate(all_names):
                progress(
                    i + 1, total, "Extrayendo...",
                    Path(name).name or name,
                )
                if name.endswith("/"):
                    continue
                if prefix and not name.startswith(prefix):
                    continue
                relative = (
                    Path(name).relative_to(prefix.rstrip("/"))
                    if prefix else Path(name)
                )
                target = temp_dir / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(zf.read(name))
        return temp_dir

    @staticmethod
    def _replace_atomic(src: Path, dst: Path) -> None:
        """Replace dst with src atomically, with fallback for cross-device / Windows locks."""
        if dst.is_dir():
            shutil.rmtree(str(dst), ignore_errors=True)
        dst.parent.mkdir(parents=True, exist_ok=True)
        try:
            src.rename(dst)
        except OSError:
            shutil.copytree(src, dst, dirs_exist_ok=True)
            shutil.rmtree(str(src), ignore_errors=True)

    def _locate_manifest(
        self, directory: Path,
    ) -> Path | None:
        for path in directory.rglob("manifest.json"):
            if path.is_file():
                return path
        return None

    def _flatten_temp_dir(
        self, temp_dir: Path, subdir: Path,
    ) -> None:
        """Move all content from subdir up to temp_dir, then remove subdir."""
        for item in list(subdir.iterdir()):
            dest = temp_dir / item.name
            if dest.exists():
                if item.is_dir():
                    shutil.copytree(item, dest, dirs_exist_ok=True)
                    shutil.rmtree(str(item))
                else:
                    dest.write_bytes(item.read_bytes())
                    item.unlink()
            else:
                item.rename(dest)
        if subdir.is_dir():
            shutil.rmtree(str(subdir))

    def _install_single_pack(
        self, path: Path, progress: ProgressCallback,
        updates: dict[str, dict[str, Any]],
    ) -> None:
        with zipfile.ZipFile(str(path), "r") as zf:
            all_entries = zf.infolist()
            all_names = zf.namelist()
            total = len(all_names)

        # Buscar explícitamente manifest.json (case-insensitive)
        manifest_entry: str | None = None
        for zi in all_entries:
            candidate = Path(zi.filename).name.lower()
            if candidate == "manifest.json":
                manifest_entry = zi.filename
                break

        if manifest_entry is None:
            _debug_zip_contents(path, all_names, "")
            raise RuntimeError(
                "No se encontr\u00f3 ning\u00fan archivo "
                "llamado manifest.json en el .mcpack."
            )

        # Leer el manifest
        with zipfile.ZipFile(str(path), "r") as zf:
            try:
                raw = json.loads(
                    zf.read(manifest_entry).decode("utf-8")
                )
            except Exception as exc:
                raise RuntimeError(
                    f"No se pudo leer {manifest_entry!r}: {exc}"
                ) from exc

        hdr = raw.get("header", {})
        mods = raw.get("modules", [])
        pack_type = mods[0].get("type", "") if mods else ""
        if not pack_type:
            raise RuntimeError(
                f"El manifest {manifest_entry!r} no tiene "
                "un tipo v\u00e1lido en modules[].type."
            )

        uuid = hdr.get("uuid", "")
        version = hdr.get("version", [])

        # Determinar el directorio base del pack dentro del ZIP
        manifest_pp = PurePosixPath(manifest_entry)
        if manifest_pp.parent == PurePosixPath("."):
            prefix = ""
        else:
            prefix = manifest_pp.parent.as_posix() + "/"

        target_base = (
            self._res_dir if pack_type == _RESOURCE_TYPE
            else self._beh_dir
        )
        pack_folder = uuid if uuid else path.stem

        # Extraer a temp dir
        temp_dir = self._extract_to_temp(
            path, prefix, target_base, pack_folder,
            progress, all_names, total,
        )

        # Verificar que manifest.json exista en temp_dir
        manifest_in_temp = self._locate_manifest(temp_dir)
        if manifest_in_temp is None:
            import sys
            print(
                "=== DEBUG manifest ===",
                file=sys.stderr,
            )
            print(
                f"  manifest_entry={manifest_entry!r}",
                file=sys.stderr,
            )
            extraido = (temp_dir / Path(manifest_entry).name).is_file()
            print(
                f"  ¿existe en temp_dir? {extraido}",
                file=sys.stderr,
            )
            print(
                f"  temp_dir={temp_dir}",
                file=sys.stderr,
            )
            print(
                f"  ruta absoluta="
                f"{(temp_dir / Path(manifest_entry).name).resolve()}",
                file=sys.stderr,
            )
            _debug_zip_contents(path, all_names, prefix)
            shutil.rmtree(str(temp_dir))
            raise RuntimeError(
                "Error: el manifest.json no se extrajo "
                "correctamente.\n"
                f"Manifest encontrado como: {manifest_entry!r}\n"
                f"Prefijo usado: {prefix!r}"
            )

        if manifest_in_temp.parent != temp_dir:
            self._flatten_temp_dir(temp_dir, manifest_in_temp.parent)

        # Finalize: swap or install
        if uuid in updates:
            entry = updates[uuid]
            old_path = self._server_path / entry["path"]
            self._replace_atomic(temp_dir, old_path)
            # Update valid_known_packs
            known = _read_valid_known(self._server_path)
            known = [p for p in known if p.get("uuid") != uuid]
            known.append({
                "file_system": "RawPath",
                "path": entry["path"],
                "uuid": uuid,
                "version": version,
            })
            _save_valid_known(self._server_path, known)
        else:
            final_path = target_base / pack_folder
            self._replace_atomic(temp_dir, final_path)
            self._register_pack(target_base, pack_folder, uuid, version)

    def _install_addon(
        self, path: Path, progress: ProgressCallback,
        updates: dict[str, dict[str, Any]],
    ) -> None:
        detected = _detect_packs_in_zip(str(path))
        if not detected:
            raise RuntimeError(
                "No se encontraron packs v\u00e1lidos en el .mcaddon."
            )

        with zipfile.ZipFile(str(path), "r") as zf:
            all_names = zf.namelist()
            total = len(all_names)

        plan: list[tuple[str, Path, str, str, list[int]]] = []
        for pack_type, dir_name, prefix in detected:
            target_base = (
                self._res_dir if pack_type == _RESOURCE_TYPE
                else self._beh_dir
            )

            manifest_path_in_zip = prefix + "manifest.json"
            uuid = ""
            version: list[int] = []
            with zipfile.ZipFile(str(path), "r") as zf:
                try:
                    raw = json.loads(
                        zf.read(manifest_path_in_zip).decode("utf-8")
                    )
                    _, _, p_uuid, p_version, _ = _parse_manifest(raw)
                    uuid = p_uuid
                    version = p_version
                except Exception:
                    pass

            pack_folder = uuid if uuid else _sanitize_name(dir_name)
            plan.append((prefix, target_base, pack_folder, uuid, version))

        plan.sort(key=lambda x: len(x[0]), reverse=True)

        # Extract each pack to its own temp dir
        temp_dirs: list[tuple[Path, Path, str, str, list[int]]] = []
        try:
            for prefix, target_base, pack_folder, uuid, version in plan:
                temp_dir = self._extract_to_temp(
                    path, prefix, target_base, pack_folder,
                    progress, all_names, total,
                )
                manifest_in_temp = self._locate_manifest(temp_dir)
                if manifest_in_temp is None:
                    raise RuntimeError(
                        "Error al extraer un pack del .mcaddon."
                    )
                if manifest_in_temp.parent != temp_dir:
                    self._flatten_temp_dir(temp_dir, manifest_in_temp.parent)
                temp_dirs.append((
                    temp_dir, target_base, pack_folder, uuid, version,
                ))

            # All extractions succeeded — finalize
            for temp_dir, target_base, pack_folder, uuid, version in temp_dirs:
                if uuid in updates:
                    entry = updates[uuid]
                    old_path = self._server_path / entry["path"]
                    self._replace_atomic(temp_dir, old_path)
                    known = _read_valid_known(self._server_path)
                    known = [p for p in known if p.get("uuid") != uuid]
                    known.append({
                        "file_system": "RawPath",
                        "path": entry["path"],
                        "uuid": uuid,
                        "version": version,
                    })
                    _save_valid_known(self._server_path, known)
                else:
                    final_path = target_base / pack_folder
                    self._replace_atomic(temp_dir, final_path)
                    if uuid:
                        self._register_pack(
                            target_base, pack_folder, uuid, version,
                        )

        except (_CancelledError, Exception):
            # Clean up any temp dirs on failure or cancellation
            for td, _, _, _, _ in temp_dirs:
                if td.is_dir():
                    shutil.rmtree(str(td))
            raise

    def _register_pack(
        self, target_base: Path, pack_folder: str,
        uuid: str, version: list[int],
    ) -> None:
        if not uuid:
            return
        packs = _read_valid_known(self._server_path)
        if any(p.get("uuid") == uuid for p in packs):
            return
        entry = {
            "file_system": "RawPath",
            "path": (
                f"{target_base.relative_to(self._server_path)}"
                f"/{pack_folder}/"
            ),
            "uuid": uuid,
            "version": version,
        }
        packs.append(entry)
        _save_valid_known(self._server_path, packs)

    # ── Remove ──────────────────────────────────────

    def _remove_pack(self) -> None:
        tables: list[QTableWidget | None] = [
            self._res_table, self._beh_table,
        ]

        # Collect all selected UUIDs + paths from both tables
        selections: list[tuple[str, str, str]] = []  # (uuid, pack_path, name)
        known = _read_valid_known(self._server_path)
        for table in tables:
            if not table:
                continue
            rows = table.selectionModel().selectedRows()
            if not rows:
                continue
            for index in rows:
                row = index.row()
                item = table.item(row, 0)
                if not item:
                    continue
                uid = item.data(Qt.UserRole)
                if not uid:
                    continue
                pack_path = ""
                for entry in known:
                    if entry.get("uuid") == uid:
                        pack_path = entry.get("path", "")
                        break
                name = table.item(row, 1).text() if table.item(row, 1) else ""
                selections.append((uid, pack_path, name))

        if not selections:
            return

        # Single confirmation for all selected packs
        names = "\n".join(f"  \u2022 {s[2] or s[0]}" for s in selections)
        reply = QMessageBox.question(
            self,
            "Eliminar packs",
            f"\u00bfEst\u00e1s seguro de eliminar "
            f"{len(selections)} pack(s)?\n\n{names}",
            QMessageBox.Yes | QMessageBox.No,
        )
        if reply != QMessageBox.Yes:
            return

        dlg = ProgressDialog(self)
        dlg.bind(self._task_mgr)
        self._task_mgr.run(
            "Eliminando packs...", self._do_remove,
            [(uid, path) for uid, path, _ in selections],
        )
        dlg.exec()

        self._scan_packs()
        QMessageBox.information(
            self, "Completado",
            f"{len(selections)} pack(s) eliminado(s) correctamente.",
        )

    def _do_remove(
        self,
        pack_list: list[tuple[str, str]],
        progress: ProgressCallback,
    ) -> None:
        """Remove multiple packs in one task.

        pack_list: list of (uuid, pack_path) tuples.
        """
        server_path = self._server_path
        world_dir = self._world_dir

        # Step 1: count all files across all pack dirs
        all_items: list[tuple[Path, str]] = []  # (path, uuid_for_lookup)
        for uid, pack_path in pack_list:
            d = server_path / pack_path if pack_path else None
            if d and d.is_dir():
                for p in d.rglob("*"):
                    all_items.append((p, uid))
                all_items.append((d, uid))
        total = len(all_items)

        if total == 0:
            progress(0, 1, "Eliminando...", "No hay archivos que eliminar")
        else:
            progress(0, total, "Eliminando...", "Preparando...")

        # Step 2: remove from valid_known_packs.json
        uuids = {uid for uid, _ in pack_list}
        known = _read_valid_known(server_path)
        known = [p for p in known if p.get("uuid") not in uuids]
        _save_valid_known(server_path, known)

        # Step 3: remove from world packs (both types) — only if world exists
        if self._world_exists():
            for wt in (_RESOURCE_TYPE, _BEHAVIOR_TYPE):
                active = _read_world_packs(world_dir, wt)
                active = [p for p in active if p.get("pack_id") not in uuids]
                _save_world_packs(world_dir, wt, active)

        # Step 4: delete directory trees with progress
        if total > 0:
            # Sort reverse so deepest files come first
            for i, (item, _) in enumerate(
                sorted(all_items, key=lambda x: str(x[0]), reverse=True)
            ):
                progress(i + 1, total, "Eliminando...", item.name)
                if item.is_dir():
                    item.rmdir()
                else:
                    item.unlink()

    # ── State management ─────────────────────────────

    def _on_state_changed(self, state: ServerState) -> None:
        self._server_state = state
        running = state == ServerState.RUNNING

        self._read_only_banner.setVisible(
            running or state == ServerState.STARTING
        )

        if self._install_btn:
            self._install_btn.setEnabled(not running)
        if self._remove_btn:
            self._remove_btn.setEnabled(not running)
        if self._reload_btn:
            self._reload_btn.setEnabled(not running)

        self._update_table_flags()
