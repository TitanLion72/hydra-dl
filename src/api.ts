import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Status =
  | "queued"
  | "downloading"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export type Kind = "http" | "media" | "hls" | "torrent";

export interface Segment {
  index: number;
  start: number;
  end: number | null;
  done: number;
}

export interface Swarm {
  peers: number;
  seeders: number;
}

/** Geçici hatadan sonra yeniden deneme durumu. */
export interface RetryInfo {
  attempt: number;
  max: number;
  waitMs: number;
}

/** Torrent icindeki tek dosya. */
export interface TorrentFile {
  index: number;
  path: string;
  length: number;
  included: boolean;
}

export interface TorrentListing {
  name: string;
  infoHash: string;
  totalBytes: number;
  files: TorrentFile[];
}

export interface Download {
  id: string;
  url: string;
  filename: string;
  dir: string;
  kind: Kind;
  status: Status;
  totalBytes: number | null;
  downloadedBytes: number;
  segments: Segment[];
  resumable: boolean;
  speedBps: number;
  progress: number | null;
  error: string | null;
  createdAt: number;
  finishedAt: number | null;
  connections: number;
  referer: string | null;
  mime: string | null;
  filePath: string | null;
  format: string | null;
  swarm: Swarm | null;
  source: string | null;
  torrentFiles?: TorrentFile[] | null;
  retry?: RetryInfo | null;
}

export interface Settings {
  downloadDir: string;
  maxConcurrent: number;
  connections: number;
  speedLimitKbps: number;
  captureEnabled: boolean;
  capturePort: number;
  ytdlpPath: string;
  autoResume: boolean;
  askOnDownload: boolean;
  hunterEnabled: boolean;
  webEnabled: boolean;
  webPort: number;
  webBind: string;
  webToken: string;
  torrentEnabled: boolean;
  notifyOnFinish: boolean;
  proxyUrl: string;
}

export interface CaptureStatus {
  enabled: boolean;
  running: boolean;
  port: number;
  url: string | null;
}

export interface WebStatus {
  enabled: boolean;
  running: boolean;
  port: number;
  bind: string;
  url: string | null;
  lanUrls: string[];
  tokenRequired: boolean;
}

export interface YtdlpStatus {
  available: boolean;
  path: string | null;
  version: string | null;
}

export interface AppInfo {
  version: string;
  downloadDir: string;
  statePath: string;
  capture: CaptureStatus;
  web: WebStatus;
  webQr: string | null;
  ytdlp: YtdlpStatus;
  uptimeSecs: number;
}

export interface Notice {
  message: string;
  level: "success" | "error" | "info";
}

export interface FormatOption {
  id: string;
  label: string;
  ext?: string;
  height?: number;
  filesize?: number;
  vcodec?: string;
  acodec?: string;
  note?: string;
}

export interface Candidate {
  url: string;
  kind: string;
  label: string;
  size?: number;
}

export interface ProbeResult {
  ok: boolean;
  url: string;
  finalUrl: string;
  kind: "file" | "media" | "torrent" | "hls" | "dash" | "page" | "unknown" | string;
  title?: string;
  filename?: string;
  size?: number;
  mime?: string;
  durationSecs?: number;
  resumable: boolean;
  needsYtdlp: boolean;
  formats: FormatOption[];
  candidates: Candidate[];
  warnings: string[];
}

export interface AddArgs {
  url: string;
  dir?: string;
  filename?: string;
  media?: boolean;
  referer?: string;
  format?: string;
  /** Torrent'te indirilecek dosya indeksleri. */
  files?: number[];
}

export const api = {
  list: () => invoke<Download[]>("list_downloads"),
  add: (args: AddArgs) => invoke<Download>("add_download", { ...args }),
  probe: (url: string, media?: boolean) =>
    invoke<ProbeResult>("probe_url", { url, media }),
  listTorrentFiles: (source: string) =>
    invoke<TorrentListing>("list_torrent_files", { source }),
  setTorrentFiles: (id: string, files: number[]) =>
    invoke<void>("set_torrent_files", { id, files }),
  pause: (id: string) => invoke<void>("pause_download", { id }),
  resume: (id: string) => invoke<void>("resume_download", { id }),
  cancel: (id: string) => invoke<void>("cancel_download", { id }),
  remove: (id: string, deleteFile = false) =>
    invoke<void>("remove_download", { id, deleteFile }),
  clearFinished: (deleteFiles = false) =>
    invoke<void>("clear_finished", { deleteFiles }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) =>
    invoke<Settings>("save_settings", { settings }),
  appInfo: () => invoke<AppInfo>("get_app_info"),
  captureStatus: () => invoke<CaptureStatus>("get_capture_status"),
  webStatus: () => invoke<WebStatus>("get_web_status"),
  openDownload: (id: string) => invoke<void>("open_download", { id }),
  revealDownload: (id: string) => invoke<void>("reveal_download", { id }),
  installYtdlp: () => invoke<YtdlpStatus>("install_ytdlp"),
  notifyTest: () => invoke<void>("test_notification"),
};

export function onDownloads(handler: (items: Download[]) => void): Promise<UnlistenFn> {
  return listen<Download[]>("downloads:changed", (event) => handler(event.payload));
}

export function onNotice(handler: (notice: Notice) => void): Promise<UnlistenFn> {
  return listen<Notice>("hydra:notice", (event) => handler(event.payload));
}

export interface InstallProgress {
  stage: "downloading" | "verifying" | "done" | "failed" | string;
  downloaded: number;
  total: number | null;
  error?: string;
}

export function onYtdlpInstall(
  handler: (progress: InstallProgress) => void,
): Promise<UnlistenFn> {
  return listen<InstallProgress>("ytdlp:install", (event) => handler(event.payload));
}
