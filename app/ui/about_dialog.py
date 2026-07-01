from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtGui import QPixmap
from PySide6.QtWidgets import (
    QDialog,
    QFrame,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from app.core.app_info import (
    APP_AUTHOR,
    APP_COPYRIGHT,
    APP_COPYRIGHT_AUTHOR,
    APP_DESCRIPTION,
    APP_LICENSE,
    APP_LICENSE_VERSION,
    APP_NAME,
    APP_VERSION,
    get_tech_stack,
)
from app.core.paths import get_app_icon, resource_path


class AboutDialog(QDialog):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("aboutDialog")
        self.setWindowTitle("Acerca de")
        self.setWindowIcon(get_app_icon())
        self.setFixedSize(420, 560)

        root = QVBoxLayout(self)
        root.setContentsMargins(16, 16, 16, 16)
        root.setSpacing(0)

        # ── Card ──────────────────────────────────────
        card = QFrame()
        card.setObjectName("card")
        c = QVBoxLayout(card)
        c.setContentsMargins(28, 28, 28, 24)
        c.setSpacing(0)

        # Logo
        logo = QLabel()
        logo.setAlignment(Qt.AlignCenter)
        p = resource_path() / "resources" / "images" / "logo.png"
        if p.is_file():
            pix = QPixmap(str(p))
            if not pix.isNull():
                logo.setPixmap(pix.scaled(64, 64, Qt.KeepAspectRatio, Qt.SmoothTransformation))
        logo.setFixedHeight(68)
        c.addWidget(logo)

        # App name
        c.addSpacing(12)
        name = QLabel(APP_NAME)
        name.setObjectName("aboutAppName")
        name.setAlignment(Qt.AlignCenter)
        c.addWidget(name)

        # Slogan
        c.addSpacing(4)
        slogan = QLabel("Administra tu servidor.\nSin complicaciones.")
        slogan.setObjectName("aboutSlogan")
        slogan.setAlignment(Qt.AlignCenter)
        slogan.setWordWrap(True)
        c.addWidget(slogan)

        # Version
        c.addSpacing(4)
        ver = QLabel(f"v{APP_VERSION}")
        ver.setObjectName("aboutVersion")
        ver.setAlignment(Qt.AlignCenter)
        c.addWidget(ver)

        # Description
        c.addSpacing(12)
        desc = QLabel(APP_DESCRIPTION)
        desc.setObjectName("aboutDescription")
        desc.setAlignment(Qt.AlignCenter)
        desc.setWordWrap(True)
        c.addWidget(desc)

        # Separator
        c.addSpacing(14)
        sep = QFrame()
        sep.setObjectName("aboutSeparator")
        sep.setFixedHeight(1)
        c.addWidget(sep)

        # Author
        c.addSpacing(14)
        c.addLayout(_section("AUTOR", APP_AUTHOR))

        # Tech stack
        c.addSpacing(14)
        c.addLayout(_section("TECNOLOG\u00cdAS"))
        c.addSpacing(4)
        for tech in get_tech_stack():
            row = QLabel(tech)
            row.setObjectName("aboutValue")
            c.addWidget(row)
            c.addSpacing(2)

        # License
        c.addSpacing(12)
        c.addLayout(_section("LICENCIA", APP_LICENSE))
        c.addSpacing(2)
        lv = QLabel(APP_LICENSE_VERSION)
        lv.setObjectName("aboutLicenseSub")
        lv.setAlignment(Qt.AlignCenter)
        c.addWidget(lv)

        # Buttons
        c.addSpacing(16)
        btns = QHBoxLayout()
        btns.setSpacing(10)
        btns.addStretch()
        self._github_btn = QPushButton("GitHub")
        self._github_btn.setObjectName("btnSecondary")
        self._github_btn.setVisible(False)
        btns.addWidget(self._github_btn)
        close = QPushButton("Cerrar")
        close.setObjectName("btnPrimary")
        close.setMinimumWidth(130)
        close.clicked.connect(self.accept)
        btns.addWidget(close)
        btns.addStretch()
        c.addLayout(btns)

        root.addWidget(card)

        # ── Copyright footer (outside card) ──────────
        root.addSpacing(10)
        ftr = QWidget()
        ftr.setAttribute(Qt.WA_StyledBackground, False)
        fl = QVBoxLayout(ftr)
        fl.setContentsMargins(0, 0, 0, 0)
        fl.setSpacing(0)
        fl.setAlignment(Qt.AlignCenter)
        for line in (APP_COPYRIGHT, APP_COPYRIGHT_AUTHOR):
            lbl = QLabel(line)
            lbl.setObjectName("aboutCopyright")
            lbl.setAlignment(Qt.AlignCenter)
            fl.addWidget(lbl)
        root.addWidget(ftr)

    def set_github_url(self, url: str) -> None:
        if not url:
            return
        self._github_btn.setVisible(True)
        import webbrowser

        self._github_btn.clicked.connect(lambda: webbrowser.open(url))


def _section(title: str, value: str = "") -> QVBoxLayout:
    lay = QVBoxLayout()
    lay.setSpacing(2)
    t = QLabel(title)
    t.setObjectName("aboutSectionTitle")
    lay.addWidget(t)
    if value:
        v = QLabel(value)
        v.setObjectName("aboutValue")
        lay.addWidget(v)
    return lay
