from __future__ import annotations

import json
from pathlib import Path
from typing import Any


class Config:
    _instance: Config | None = None

    def __init__(self) -> None:
        self._path: Path = Path("config.json")
        self._data: dict[str, Any] = {}

    @classmethod
    def instance(cls) -> Config:
        if cls._instance is None:
            cls._instance = Config()
        return cls._instance

    def load(self, path: str | Path | None = None) -> None:
        if path is not None:
            self._path = Path(path)
        if self._path.is_file():
            with open(self._path, "r", encoding="utf-8") as f:
                self._data = json.load(f)

    def save(self) -> None:
        with open(self._path, "w", encoding="utf-8") as f:
            json.dump(self._data, f, indent=4, ensure_ascii=False)

    def get(self, key: str, default: Any = None) -> Any:
        return self._data.get(key, default)

    def set(self, key: str, value: Any) -> None:
        self._data[key] = value
