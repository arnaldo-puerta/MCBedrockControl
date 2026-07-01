from __future__ import annotations

from typing import Any, Callable

from PySide6.QtCore import QObject, QThread, Signal

ProgressCallback = Callable[[int, int, str, str], None]


class _CancelledError(BaseException):
    pass


class _TaskWorker(QObject):
    finished = Signal(object)
    error = Signal(str)
    progress_changed = Signal(int, int, str, str)

    def __init__(
        self,
        task_fn: Callable[..., Any],
        args: tuple[Any, ...],
        kwargs: dict[str, Any],
    ) -> None:
        super().__init__()
        self._task_fn = task_fn
        self._args = args
        self._kwargs = kwargs
        self._cancelled = False

    def cancel(self) -> None:
        self._cancelled = True

    def _progress(self, current: int, total: int, status: str, detail: str) -> None:
        if self._cancelled:
            raise _CancelledError("Task cancelled")
        self.progress_changed.emit(current, total, status, detail)

    def run(self) -> None:
        try:
            result = self._task_fn(
                *self._args,
                progress=self._progress,
                **self._kwargs,
            )
            self.finished.emit(result)
        except _CancelledError:
            pass
        except Exception as e:
            self.error.emit(str(e))


class TaskManager(QObject):
    started = Signal(str)
    progress_changed = Signal(int, int, str, str)
    finished = Signal(object)
    error = Signal(str)
    cancelled = Signal()

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._thread: QThread | None = None
        self._worker: _TaskWorker | None = None
        self._task_name: str = ""

    @property
    def is_running(self) -> bool:
        return self._thread is not None and self._thread.isRunning()

    def run(
        self,
        task_name: str,
        task_fn: Callable[..., Any],
        *args: Any,
        **kwargs: Any,
    ) -> None:
        if self.is_running:
            return

        self._task_name = task_name
        self._thread = QThread(self)
        self._worker = _TaskWorker(task_fn, args, kwargs)
        self._worker.moveToThread(self._thread)

        self._worker.finished.connect(self._thread.quit)
        self._worker.finished.connect(self._on_finished)
        self._worker.error.connect(self._thread.quit)
        self._worker.error.connect(self._on_error)
        self._worker.progress_changed.connect(self.progress_changed)
        self._thread.started.connect(self._worker.run)
        self._thread.finished.connect(self._cleanup)

        self._thread.start()
        self.started.emit(self._task_name)

    def cancel(self) -> None:
        if self._worker:
            self._worker.cancel()
        thread = self._thread
        if thread and thread.isRunning():
            thread.quit()
            thread.wait(3000)
        self.cancelled.emit()
        self._cleanup()

    def _on_finished(self, result: Any) -> None:
        self.finished.emit(result)

    def _on_error(self, msg: str) -> None:
        self.error.emit(msg)

    def _cleanup(self) -> None:
        if self._worker:
            self._worker.deleteLater()
        if self._thread:
            self._thread.deleteLater()
        self._worker = None
        self._thread = None
