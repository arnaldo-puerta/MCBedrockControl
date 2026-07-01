from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QMainWindow, QStackedWidget, QVBoxLayout, QWidget

from app.core.paths import get_app_icon
from app.ui.about_dialog import AboutDialog
from app.ui.sidebar import Sidebar


class MainWindow(QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("MCBedrockControl")
        self.setWindowIcon(get_app_icon())
        self.resize(1100, 700)

        self._stack: QStackedWidget = QStackedWidget()
        self._sidebar: Sidebar = Sidebar()

        central = QWidget()
        layout = QHBoxLayout(central)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)
        layout.addWidget(self._sidebar)
        layout.addWidget(self._stack, 1)
        self.setCentralWidget(central)

        self._sidebar.navigation_requested.connect(self._navigate_to)
        self._sidebar.about_requested.connect(self._open_about)

    def add_page(self, widget: QWidget, index: int) -> None:
        self._stack.insertWidget(index, widget)

    def select_page(self, index: int) -> None:
        self._sidebar.set_active(index)
        self._navigate_to(index)

    def _navigate_to(self, index: int) -> None:
        current = self._stack.currentWidget()
        if current is not None and current is not self._stack.widget(index):
            confirm = getattr(current, "confirm_leave", None)
            if confirm is not None and not confirm():
                return
        self._stack.setCurrentIndex(index)

    def _open_about(self) -> None:
        dialog = AboutDialog(self)
        dialog.exec()

    def closeEvent(self, event: object) -> None:
        current = self._stack.currentWidget()
        if current is not None:
            confirm = getattr(current, "confirm_leave", None)
            if confirm is not None and not confirm():
                event.ignore()  # type: ignore[attr-defined]
                return
        super().closeEvent(event)  # type: ignore[misc]
