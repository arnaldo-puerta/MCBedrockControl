from __future__ import annotations

from pathlib import Path
from typing import Any

from app.core.task_manager import ProgressCallback
from app.core.utils import read_lines, set_property, write_lines


class ServerManager:
    def __init__(self, server_path: str = "") -> None:
        self._server_path = Path(server_path) if server_path else Path("server")

    @property
    def server_path(self) -> Path:
        return self._server_path

    @server_path.setter
    def server_path(self, path: str | Path) -> None:
        self._server_path = Path(path)

    @property
    def executable(self) -> Path:
        return self._server_path / "bedrock_server.exe"

    @property
    def properties_path(self) -> Path:
        return self._server_path / "server.properties"

    @property
    def eula_path(self) -> Path:
        return self._server_path / "eula.txt"

    def is_installed(self) -> bool:
        return (
            self._server_path.is_dir()
            and self.executable.is_file()
        )

    def install(
        self,
        zip_path: str | Path,
        server_name: str,
        progress: ProgressCallback | None = None,
    ) -> None:
        import zipfile

        server_path = self._server_path
        server_path.mkdir(parents=True, exist_ok=True)

        with zipfile.ZipFile(str(zip_path), "r") as zf:
            members = [m for m in zf.infolist() if not m.is_dir()]
            total = len(members)

            if progress:
                progress(0, total, "Extrayendo servidor...", "Preparando archivos...")

            for i, member in enumerate(members):
                if progress:
                    progress(
                        i + 1, total,
                        "Extrayendo servidor...",
                        member.filename,
                    )

                target = server_path / member.filename
                target.parent.mkdir(parents=True, exist_ok=True)
                zf.extract(member, server_path)

        if self.properties_path.is_file():
            lines = read_lines(self.properties_path)
            lines = set_property(lines, "server-name", server_name)
            lines = set_property(lines, "emit-server-telemetry", "true")
            write_lines(self.properties_path, lines)

        with open(self.eula_path, "w", encoding="utf-8") as f:
            f.write("eula=true\n")

        if progress:
            progress(total, total, "Instalaci\u00f3n completada", "")

    def detect_executable(self) -> str:
        return str(self.executable)

    @property
    def server_name(self) -> str:
        props = self.read_properties()
        return props.get("server-name", "").strip()

    def read_properties(self) -> dict[str, str]:
        if not self.properties_path.is_file():
            return {}
        lines = read_lines(self.properties_path)
        result: dict[str, str] = {}
        for line in lines:
            stripped = line.strip()
            if "=" in stripped and not stripped.startswith("#"):
                key, _, val = stripped.partition("=")
                result[key.strip()] = val.strip()
        return result

    @property
    def world_exists(self) -> bool:
        level_name = self.read_properties().get("level-name", "").strip()
        if not level_name:
            return False
        world_dir = self._server_path / "worlds" / level_name
        return (world_dir / "level.dat").is_file()

    def write_properties(self, props: dict[str, str]) -> None:
        if not self.properties_path.is_file():
            return
        lines = read_lines(self.properties_path)
        for key, value in props.items():
            lines = set_property(lines, key, value)
        write_lines(self.properties_path, lines)
