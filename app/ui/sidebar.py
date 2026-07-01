from __future__ import annotations

from PySide6.QtCore import Signal
from PySide6.QtWidgets import QFrame, QPushButton, QVBoxLayout, QWidget

from app.core.icons import Icons, SIDEBAR_SIZE
from app.ui.sidebar_brand import SidebarBrand


class Sidebar(QFrame):
    navigation_requested = Signal(int)
    about_requested = Signal()

    _NAV_ITEMS: list[tuple[str, str]] = [
        ("SERVER", "Servidor"),
        ("CONSOLE", "Consola"),
        ("OPTIONS", "Opciones"),
        ("PLAYERS", "Jugadores"),
        ("PACKS", "Packs"),
        ("WORLDS", "Mundos"),
        ("UPDATE", "Actualizaci\u00f3n"),
    ]

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("sidebar")
        self.setFixedWidth(200)

        self._buttons: list[QPushButton] = []

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        layout.addWidget(SidebarBrand())

        for i, (attr, text) in enumerate(self._NAV_ITEMS):
            btn = QPushButton(text)
            btn.setObjectName("sidebarBtn")
            btn.setIcon(getattr(Icons, attr))
            btn.setIconSize(SIDEBAR_SIZE)
            btn.setCheckable(True)
            btn.clicked.connect(lambda checked, idx=i: self._on_navigate(idx))
            self._buttons.append(btn)
            layout.addWidget(btn)

        layout.addStretch()

        about_btn = QPushButton("Acerca de")
        about_btn.setObjectName("sidebarBtn")
        about_btn.setIcon(Icons.INFO)
        about_btn.setIconSize(SIDEBAR_SIZE)
        about_btn.setCheckable(False)
        about_btn.clicked.connect(self._on_about)
        layout.addWidget(about_btn)

    def set_active(self, index: int) -> None:
        for i, btn in enumerate(self._buttons):
            btn.setChecked(i == index)

    def _on_navigate(self, index: int) -> None:
        self.set_active(index)
        self.navigation_requested.emit(index)

    def _on_about(self) -> None:
        self.about_requested.emit()
