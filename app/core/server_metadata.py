from __future__ import annotations

import re
import uuid
from datetime import datetime
from pathlib import Path
from typing import Any

from app.core.config import Config


_SERVER_TYPE = "Bedrock Dedicated Server"
_ZIP_VERSION_RE = re.compile(
    r"bedrock-server[-_](\d+\.\d+\.\d+\.\d+)", re.IGNORECASE
)


def _get_exe_version(exe_path: Path) -> str | None:
    import platform
    if platform.system() != "Windows":
        return None
    import subprocess
    try:
        result = subprocess.run(
            [
                "powershell", "-NoProfile", "-Command",
                f"(Get-Item '{exe_path}').VersionInfo.FileVersion",
            ],
            capture_output=True, text=True, timeout=10,
        )
        if result.returncode == 0:
            ver = result.stdout.strip()
            if ver:
                return ver
    except Exception:
        pass
    return None


def detect_version(exe_path: Path, zip_path: str | Path | None = None) -> str:
    ver = _get_exe_version(exe_path)
    if ver:
        return ver
    if zip_path:
        m = _ZIP_VERSION_RE.search(str(zip_path))
        if m:
            return m.group(1)
    return "Desconocida"


def load_metadata() -> dict[str, Any]:
    config = Config.instance()
    meta = config.get("server_metadata", {})
    if isinstance(meta, dict):
        return meta
    return {}


def save_metadata(meta: dict[str, Any]) -> None:
    config = Config.instance()
    config.set("server_metadata", meta)
    config.save()


def get_version() -> str:
    meta = load_metadata()
    return meta.get("server_version", "Desconocida")


def set_version(version: str) -> str:
    meta = load_metadata()
    meta["server_version"] = version
    meta["server_type"] = _SERVER_TYPE
    meta["last_update"] = datetime.now().isoformat(timespec="seconds")
    if "installed_at" not in meta:
        meta["installed_at"] = meta["last_update"]
    if "server_uuid" not in meta:
        meta["server_uuid"] = str(uuid.uuid4())
    save_metadata(meta)
    return version


def mark_installed(exe_path: Path, zip_path: str | Path | None = None) -> str:
    version = detect_version(exe_path, zip_path)
    meta = {
        "server_version": version,
        "server_type": _SERVER_TYPE,
        "installed_at": datetime.now().isoformat(timespec="seconds"),
        "last_update": datetime.now().isoformat(timespec="seconds"),
        "server_uuid": str(uuid.uuid4()),
    }
    save_metadata(meta)
    return version
