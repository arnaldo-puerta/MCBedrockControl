from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtGui import QPixmap
from PySide6.QtWidgets import QHBoxLayout, QLabel, QVBoxLayout, QWidget

from app.core.icons import Icons


_LOGO_SIZE = 44


class SidebarBrand(QWidget):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("sidebarBrand")

        layout = QHBoxLayout(self)
        layout.setContentsMargins(16, 20, 16, 24)
        layout.setSpacing(8)
        layout.setAlignment(Qt.AlignLeft | Qt.AlignVCenter)

        # ── Logo ──────────────────────────────────
        self._logo_label = QLabel()
        self._logo_label.setFixedSize(_LOGO_SIZE, _LOGO_SIZE)
        self._load_logo()
        layout.addWidget(self._logo_label)

        # ── Text ──────────────────────────────────
        text_col = QVBoxLayout()
        text_col.setContentsMargins(0, 0, 0, 0)
        text_col.setSpacing(0)
        text_col.setAlignment(Qt.AlignVCenter)

        line1 = QLabel("MC Bedrock")
        line1.setObjectName("sidebarBrandName1")
        text_col.addWidget(line1)

        line2 = QLabel("Control")
        line2.setObjectName("sidebarBrandName2")
        text_col.addWidget(line2)

        layout.addLayout(text_col)
        layout.addStretch()

    def _load_logo(self) -> None:
        from app.core.paths import resource_path
        logo = resource_path() / "resources" / "images" / "logo.png"
        if logo.is_file():
            pixmap = QPixmap(str(logo))
            if not pixmap.isNull():
                self._logo_label.setPixmap(
                    pixmap.scaled(
                        _LOGO_SIZE, _LOGO_SIZE,
                        Qt.KeepAspectRatio,
                        Qt.SmoothTransformation,
                    )
                )
                return
        pixmap = Icons.SERVER.pixmap(_LOGO_SIZE - 4, _LOGO_SIZE - 4)
        self._logo_label.setPixmap(pixmap)
