import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ServerState = "stopped" | "starting" | "running" | "stopping" | "crashed";
export type LogLevel = "info" | "warn" | "error" | "plain";

export interface LogLine {
  seq: number;
  level: LogLevel;
  text: string;
}

export interface Player {
  name: string;
  xuid: string;
}

export interface Stats {
  cpuPercent: number;
  memoryMb: number;
  memoryPercent: number;
  uptimeSecs: number;
}

export interface ServerInfo {
  name: string;
  world: string;
  port: string;
  maxPlayers: string;
}

export interface AppSnapshot {
  serverPath: string | null;
  version: string | null;
  info: ServerInfo;
  server: {
    state: ServerState;
    players: Player[];
    stats: Stats;
    history: LogLine[];
  };
}

export type ServerEvent =
  | { kind: "state"; data: ServerState }
  | { kind: "logs"; data: LogLine[] }
  | { kind: "players"; data: Player[] }
  | { kind: "stats"; data: Stats }
  | { kind: "version"; data: string };

export interface InstallProgress {
  phase: "download" | "extract";
  done: number;
  total: number;
}

export interface AllowEntry {
  name: string;
  xuid?: string;
}

export interface AllowList {
  enabled: boolean;
  entries: AllowEntry[];
}

export const api = {
  allowList: () => invoke<AllowList>("get_allow_list"),
  addAllowed: (name: string) => invoke<AllowList>("add_allowed_player", { name }),
  removeAllowed: (name: string) => invoke<AllowList>("remove_allowed_player", { name }),
  setAllowListEnabled: (enabled: boolean) => invoke<AllowList>("set_allow_list_enabled", { enabled }),
  kick: (name: string) => invoke<void>("kick_player", { name }),
  snapshot: () => invoke<AppSnapshot>("get_snapshot"),
  setServerPath: (path: string) => invoke<AppSnapshot>("set_server_path", { path }),
  defaultInstallPath: () => invoke<string>("default_install_path"),
  installServer: (path: string, eulaAccepted: boolean) =>
    invoke<AppSnapshot>("install_server", { path, eulaAccepted }),
  cancelInstall: () => invoke<void>("cancel_install"),
  onInstallProgress: (handler: (progress: InstallProgress) => void) =>
    listen<InstallProgress>("install-progress", (event) => handler(event.payload)),
  start: () => invoke<void>("start_server"),
  stop: () => invoke<void>("stop_server"),
  restart: () => invoke<void>("restart_server"),
  sendCommand: (command: string) => invoke<void>("send_command", { command }),
  onEvent: (handler: (event: ServerEvent) => void) =>
    listen<ServerEvent>("server-event", (event) => handler(event.payload)),
};
