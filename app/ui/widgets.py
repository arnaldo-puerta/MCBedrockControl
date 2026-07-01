import math

from PySide6.QtCore import QRectF, QSize, Qt
from PySide6.QtGui import QColor, QFont, QPainter, QPen
from PySide6.QtWidgets import QSizePolicy, QWidget


class CircularProgress(QWidget):
    def __init__(
        self, label: str = "", parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._value: float = 0.0
        self._label: str = label
        self._unit: str = "%"
        self._fallback_text: str = ""

        self.setMinimumSize(100, 130)
        self.setSizePolicy(
            QSizePolicy.Expanding, QSizePolicy.Expanding
        )

    def set_value(self, value: float) -> None:
        self._value = max(0.0, min(100.0, value))
        self._fallback_text = ""
        self.update()

    def set_fallback(self, text: str) -> None:
        self._fallback_text = text
        self.update()

    def set_label(self, text: str) -> None:
        self._label = text
        self.update()

    def set_unit(self, unit: str) -> None:
        self._unit = unit
        self.update()

    def sizeHint(self) -> QSize:
        return QSize(150, 180)

    def paintEvent(self, event: object) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing)

        w = self.width()
        h = self.height()

        label_height = 26
        circle_area = min(w, h - label_height)
        diameter = circle_area - 16
        if diameter < 20:
            diameter = 20

        margin_x = (w - diameter) / 2.0
        margin_y = (h - label_height - diameter) / 2.0

        rect = QRectF(margin_x, margin_y, diameter, diameter)
        pen_width = max(5, diameter * 0.07)

        has_fallback = bool(self._fallback_text)

        if not has_fallback:
            pen = QPen(QColor("#2a2a2a"), pen_width, Qt.SolidLine, Qt.RoundCap)
            painter.setPen(pen)
            painter.drawArc(rect, 0, 360 * 16)

            span = int(self._value / 100.0 * 360 * 16)
            color = self._arc_color()
            pen = QPen(color, pen_width, Qt.SolidLine, Qt.RoundCap)
            painter.setPen(pen)
            if span > 0:
                painter.drawArc(rect, 90 * 16, -span)

        font = QFont()
        font.setPixelSize(max(16, int(diameter * 0.22)))
        font.setBold(True)
        painter.setFont(font)

        if has_fallback:
            painter.setPen(QColor("#888888"))
            painter.drawText(rect, Qt.AlignCenter, self._fallback_text)
        else:
            painter.setPen(QColor("#f0f0f0"))
            painter.drawText(rect, Qt.AlignCenter, f"{self._value:.0f}{self._unit}")

        font.setPixelSize(max(10, int(diameter * 0.11)))
        font.setBold(False)
        painter.setFont(font)
        painter.setPen(QColor("#aaaaaa"))
        painter.drawText(
            QRectF(0, h - label_height, w, label_height - 2),
            Qt.AlignCenter,
            self._label,
        )

        painter.end()

    def _arc_color(self) -> QColor:
        v = self._value
        if v >= 80:
            return QColor("#c94f4f")
        elif v >= 60:
            return QColor("#d4a843")
        else:
            return QColor("#7bc96f")
