from __future__ import annotations

from pathlib import Path
from typing import Sequence


def find_property_file(server_path: str | Path) -> Path | None:
    p = Path(server_path) / "server.properties"
    return p if p.is_file() else None


def read_lines(path: str | Path) -> list[str]:
    with open(path, "r", encoding="utf-8") as f:
        return f.readlines()


def write_lines(path: str | Path, lines: Sequence[str]) -> None:
    with open(path, "w", encoding="utf-8") as f:
        f.writelines(lines)


def set_property(lines: list[str], key: str, value: str) -> list[str]:
    result: list[str] = []
    found = False
    for line in lines:
        stripped = line.strip()
        if stripped.startswith(f"{key}=") or stripped.startswith(f"{key}="):
            result.append(f"{key}={value}\n")
            found = True
        else:
            result.append(line)
    if not found:
        result.append(f"{key}={value}\n")
    return result


def get_property(lines: list[str], key: str) -> str | None:
    for line in lines:
        stripped = line.strip()
        if stripped.startswith(f"{key}="):
            return stripped.split("=", 1)[1].strip()
    return None
