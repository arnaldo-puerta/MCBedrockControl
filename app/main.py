from __future__ import annotations

import sys

from PySide6.QtCore import QEvent, QObject
from PySide6.QtWidgets import QApplication, QComboBox, QDoubleSpinBox, QSpinBox

from app.core.config import Config
from app.core.paths import get_app_icon, resource_path
from app.core.process_manager import ProcessManager
from app.core.server_manager import ServerManager
from app.ui.install_wizard import InstallWizard
from app.ui.main_window import MainWindow
from app.ui.pages.console_page import ConsolePage
from app.ui.pages.options_page import OptionsPage
from app.ui.pages.packs_page import PacksPage
from app.ui.pages.players_page import PlayersPage
from app.ui.pages.server_page import ServerPage
from app.ui.pages.update_page import UpdatePage
from app.ui.pages.worlds_page import WorldsPage


class _WheelBlocker(QObject):
    def eventFilter(self, obj: QObject, event: QEvent) -> bool:
        if event.type() == QEvent.Wheel and isinstance(
            obj, (QComboBox, QSpinBox, QDoubleSpinBox),
        ):
            event.ignore()
            return True
        return super().eventFilter(obj, event)


def load_theme(app: QApplication) -> None:
    qss_path = resource_path() / "resources" / "styles" / "theme.qss"
    if qss_path.is_file():
        with open(qss_path, "r", encoding="utf-8") as f:
            app.setStyleSheet(f.read())


def main() -> None:
    app = QApplication(sys.argv)
    app.setApplicationName("MCBedrockControl")
    app.setWindowIcon(get_app_icon())

    load_theme(app)

    blocker = _WheelBlocker()
    app.installEventFilter(blocker)

    config = Config.instance()
    config.load()

    server_path = config.get("server_path", "")
    server_mgr = ServerManager(server_path)

    if not server_mgr.is_installed():
        wizard = InstallWizard()
        if wizard.exec() != InstallWizard.Accepted:
            sys.exit(0)
        config.load()
        server_path = config.get("server_path", "")
        server_mgr = ServerManager(server_path)

    process_mgr = ProcessManager()
    process_mgr.configure(
        str(server_mgr.executable),
        str(server_mgr.server_path),
    )

    window = MainWindow()

    server_page = ServerPage(process_mgr, server_mgr)
    window.add_page(server_page, 0)
    window.add_page(ConsolePage(process_mgr), 1)

    options_page = OptionsPage(process_mgr, server_mgr)
    window.add_page(options_page, 2)
    options_page.settings_saved.connect(server_page._refresh_server_name)
    options_page.settings_saved.connect(server_page._refresh_quick_info)

    window.add_page(PlayersPage(process_mgr, server_mgr), 3)
    packs_page = PacksPage(process_mgr, server_mgr)
    window.add_page(packs_page, 4)
    worlds_page = WorldsPage(process_mgr, server_mgr)
    window.add_page(worlds_page, 5)
    worlds_page.world_changed.connect(server_page._refresh_quick_info)
    worlds_page.world_changed.connect(server_page._refresh_server_name)
    worlds_page.world_changed.connect(packs_page._scan_packs)
    window.add_page(UpdatePage(process_mgr, server_mgr), 6)

    window.select_page(0)

    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
