from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog,
    QLabel,
    QProgressBar,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from app.core.task_manager import TaskManager


class ProgressDialog(QDialog):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("Progreso")
        self.setModal(True)
        self.resize(440, 180)
        self.setMinimumWidth(380)

        self._task_name_label = QLabel()
        self._status_label = QLabel()
        self._detail_label = QLabel()
        self._progress_bar = QProgressBar()
        self._cancel_button = QPushButton("Cancelar")

        self._manager: TaskManager | None = None

        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(24, 20, 24, 20)
        layout.setSpacing(8)

        self._task_name_label.setObjectName("progressTaskName")
        layout.addWidget(self._task_name_label)

        self._progress_bar.setRange(0, 100)
        self._progress_bar.setValue(0)
        self._progress_bar.setTextVisible(True)
        self._progress_bar.setObjectName("progressBar")
        layout.addWidget(self._progress_bar)

        self._status_label.setObjectName("progressStatus")
        layout.addWidget(self._status_label)

        self._detail_label.setObjectName("progressDetail")
        layout.addWidget(self._detail_label)

        layout.addStretch()

        btn_row = QVBoxLayout()
        self._cancel_button.setObjectName("btnDanger")
        self._cancel_button.clicked.connect(self._on_cancel)
        btn_row.addWidget(self._cancel_button, alignment=Qt.AlignCenter)
        layout.addLayout(btn_row)

    def set_task_name(self, text: str) -> None:
        self._task_name_label.setText(text)

    def bind(self, manager: TaskManager) -> None:
        self._manager = manager
        manager.started.connect(self._on_started)
        manager.progress_changed.connect(self._on_progress)
        manager.finished.connect(self._on_finished)
        manager.error.connect(self._on_error)
        manager.cancelled.connect(self._on_cancelled)

    def _on_started(self, task_name: str) -> None:
        self.set_task_name(task_name)
        self._cancel_button.setEnabled(True)
        self.show()

    def _on_progress(self, current: int, total: int, status: str, detail: str) -> None:
        self._status_label.setText(status)
        self._detail_label.setText(detail)

        if total > 0:
            pct = int(current / total * 100)
            self._progress_bar.setValue(pct)
            self._progress_bar.setFormat(
                f"{current} / {total}  ({pct}%)"
            )
        else:
            self._progress_bar.setRange(0, 0)
            self._progress_bar.setFormat("")

    def _on_finished(self, result: object) -> None:
        self.accept()

    def _on_error(self, msg: str) -> None:
        self._cancel_button.setText("Cerrar")
        self._cancel_button.setEnabled(True)
        self._status_label.setText(f"Error: {msg}")
        self._status_label.setStyleSheet("color: #c94f4f;")

    def _on_cancelled(self) -> None:
        self.reject()

    def _on_cancel(self) -> None:
        if self._cancel_button.text() == "Cerrar":
            self.reject()
            return
        self._cancel_button.setEnabled(False)
        self._status_label.setText("Cancelando...")
        if self._manager:
            self._manager.cancel()
