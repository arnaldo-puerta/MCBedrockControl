from __future__ import annotations

import sys
from pathlib import Path

from PySide6.QtGui import QIcon


def _is_frozen() -> bool:
    return getattr(sys, "frozen", False)


def resource_path() -> Path:
    if _is_frozen():
        return Path(sys._MEIPASS)  # type: ignore[attr-defined]
    return Path(__file__).resolve().parent.parent


def get_app_icon() -> QIcon:
    ico = resource_path() / "resources" / "images" / "logo.ico"
    if ico.is_file():
        return QIcon(str(ico))
    return QIcon()
