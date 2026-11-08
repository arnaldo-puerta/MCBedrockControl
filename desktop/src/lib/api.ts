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

export interface WorldInfo {
  folder: string;
  name: string;
  sizeBytes: number;
  modifiedSecs: number;
  active: boolean;
}

export type BackupReason = "manual" | "beforeDelete" | "beforeRestore" | "beforeUpdate" | "beforeRename";

export interface BackupInfo {
  file: string;
  world: string;
  reason: BackupReason;
  created: string;
  sizeBytes: number;
}

export interface Backups {
  dir: string;
  backups: BackupInfo[];
}

export type ServiceState = "missing" | "stopped" | "starting" | "running" | "stopping";

export type AgentPhase =
  | "needsAccount"
  | "invalidAccount"
  | "overLimit"
  | "starting"
  | "running"
  | "stopping"
  | "error";

export interface PlayitTunnel {
  /** What players type, such as `steak-ranks.tun.ply.gg:19932`. */
  address: string;
  /** Where playit forwards it, such as `127.0.0.1:19132`. */
  destination: string;
  disabledReason: string | null;
}

export interface PlayitStatus {
  installed: boolean;
  version: string | null;
  supportsNethernet: boolean;
  service: ServiceState;
  /** Null while the service is off: only it can say. */
  agent: {
    phase: AgentPhase;
    error: string | null;
    emailVerified: boolean | null;
    tunnels: PlayitTunnel[];
    notices: { message: string; link: string | null }[];
  } | null;
}

export interface Playit {
  port: string;
  status: PlayitStatus;
}

export interface UpdateCheck {
  current: string | null;
  latest: string;
  available: boolean;
}

export type PackKind = "resource" | "behavior";

export interface PackInfo {
  kind: PackKind;
  folder: string;
  name: string;
  description: string;
  uuid: string;
  version: number[];
  enabled: boolean;
  needsExperiments: boolean;
}

export interface Packs {
  packs: PackInfo[];
  texturepackRequired: boolean;
  hasWorld: boolean;
}

export type GameMode = "survival" | "creative" | "adventure";
export type Difficulty = "peaceful" | "easy" | "normal" | "hard";
export type Permission = "visitor" | "member" | "operator";

export interface ServerSettings {
  serverName: string;
  /** Null when the server has no world yet. */
  worldName: string | null;
  gamemode: GameMode;
  forceGamemode: boolean;
  difficulty: Difficulty;
  allowCheats: boolean;
  defaultPermission: Permission;
  maxPlayers: number;
  idleTimeoutMinutes: number;
  viewDistance: number;
  tickDistance: number;
  onlineMode: boolean;
  port: number;
}

export const api = {
  settings: () => invoke<ServerSettings>("get_settings"),
  saveSettings: (settings: ServerSettings) => invoke<AppSnapshot>("save_settings", { settings }),
  checkUpdate: () => invoke<UpdateCheck>("check_update"),
  updateServer: () => invoke<AppSnapshot>("update_server"),
  packs: () => invoke<Packs>("list_packs"),
  importPacks: (path: string) => invoke<Packs>("import_packs", { path }),
  setPackEnabled: (pack: PackInfo, enabled: boolean) =>
    invoke<Packs>("set_pack_enabled", { kind: pack.kind, uuid: pack.uuid, version: pack.version, enabled }),
  removePack: (pack: PackInfo) => invoke<Packs>("remove_pack", { kind: pack.kind, folder: pack.folder }),
  setTexturepackRequired: (required: boolean) => invoke<Packs>("set_texturepack_required", { required }),
  playit: () => invoke<Playit>("get_playit"),
  installPlayit: () => invoke<PlayitStatus>("install_playit"),
  startPlayitLink: (replace: boolean) => invoke<{ code: string; url: string }>("start_playit_link", { replace }),
  finishPlayitLink: (code: string) => invoke<PlayitStatus>("finish_playit_link", { code }),
  playitTunnelsUrl: () => invoke<string>("playit_tunnels_url"),
  setPlayitRunning: (running: boolean) => invoke<PlayitStatus>("set_playit_running", { running }),
  worlds: () => invoke<WorldInfo[]>("list_worlds"),
  importWorld: (path: string) => invoke<WorldInfo[]>("import_world", { path }),
  exportWorld: (folder: string, destination: string) =>
    invoke<void>("export_world", { folder, destination }),
  setActiveWorld: (folder: string) => invoke<AppSnapshot>("set_active_world", { folder }),
  deleteWorld: (folder: string) => invoke<WorldInfo[]>("delete_world", { folder }),
  backups: () => invoke<Backups>("list_backups"),
  createBackup: () => invoke<Backups>("create_backup"),
  restoreBackup: (file: string) => invoke<AppSnapshot>("restore_backup", { file }),
  deleteBackup: (file: string) => invoke<Backups>("delete_backup", { file }),
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
