from __future__ import annotations

import qtawesome as qta
from PySide6.QtCore import QSize, Qt
from PySide6.QtGui import QIcon
from PySide6.QtWidgets import QHBoxLayout, QLabel, QVBoxLayout, QWidget


def readonly_banner(message: str) -> QLabel:
    label = QLabel(
        f"\u26a0  Servidor en ejecuci\u00f3n\n\n{message}"
    )
    label.setObjectName("readOnlyBanner")
    label.setVisible(False)
    label.setWordWrap(True)
    return label

BUTTON_SIZE = QSize(30, 30)
SIDEBAR_SIZE = QSize(28, 28)
TITLE_SIZE = QSize(50, 50)


class _IconsMeta(type):
    _mapping: dict[str, str] = {
        "SERVER": "mdi.server",
        "CONSOLE": "mdi.console",
        "OPTIONS": "mdi.cog",
        "PLAYERS": "mdi.account-group",
        "PACKS": "mdi.package-variant",
        "WORLDS": "mdi.earth",
        "START": "mdi.play",
        "STOP": "mdi.stop",
        "RESTART": "mdi.restart",
        "IMPORT": "mdi.import",
        "EXPORT": "mdi.export",
        "SAVE": "mdi.content-save",
        "FOLDER": "mdi.folder-open",
        "ADD": "mdi.plus",
        "DELETE": "mdi.delete",
        "CANCEL": "mdi.close",
        "RELOAD": "mdi.refresh",
        "SEARCH": "mdi.magnify",
        "UPDATE": "mdi.update",
        "INFO": "mdi.information",
    }
    _cache: dict[str, QIcon] = {}

    def __getattr__(cls, name: str) -> QIcon:
        if name not in cls._mapping:
            raise AttributeError(f"Icons has no attribute '{name}'")
        if name not in cls._cache:
            cls._cache[name] = qta.icon(cls._mapping[name])
        return cls._cache[name]


class Icons(metaclass=_IconsMeta):
    pass


def title_row(icon: QIcon, text: str, subtitle: str = "") -> QWidget:
    row = QWidget()
    row.setObjectName("pageHeader")
    row.setAttribute(Qt.WA_StyledBackground, False)
    layout = QHBoxLayout(row)
    layout.setContentsMargins(0, 0, 0, 0)
    layout.setSpacing(14)

    lbl = QLabel()
    lbl.setPixmap(icon.pixmap(TITLE_SIZE))
    lbl.setFixedSize(TITLE_SIZE)
    lbl.setObjectName("pageTitleIcon")

    texts = QWidget()
    texts.setAttribute(Qt.WA_StyledBackground, False)
    txt_layout = QVBoxLayout(texts)
    txt_layout.setContentsMargins(0, 0, 0, 0)
    txt_layout.setSpacing(2)

    title = QLabel(text)
    title.setObjectName("pageTitle")
    txt_layout.addWidget(title)

    if subtitle:
        sub = QLabel(subtitle)
        sub.setObjectName("pageSubtitle")
        txt_layout.addWidget(sub)

    layout.addWidget(lbl)
    layout.addWidget(texts, 1)
    return row
