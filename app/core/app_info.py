from __future__ import annotations

APP_NAME = "MC Bedrock Control"
APP_VERSION = "1.0.0"
APP_DESCRIPTION = "Administrador gr\u00e1fico para servidores dedicados de Minecraft Bedrock Edition."

APP_AUTHOR = "Arnaldo Puerta"

APP_LICENSE = "MC Bedrock Control License"
APP_LICENSE_VERSION = "Versi\u00f3n 1.0"

APP_COPYRIGHT = "Copyright \u00a9 2026"
APP_COPYRIGHT_AUTHOR = "Arnaldo Puerta"

APP_WEBSITE = ""
APP_GITHUB = ""

_APP_TECH_STACK: list[str] = [
    "Python 3.14",
    "PySide6 (Qt for Python)",
    "QtAwesome",
    "PyInstaller",
]


def get_tech_stack() -> list[str]:
    return list(_APP_TECH_STACK)
