// Sahte `src/api.ts` modülü.
//
// run.mjs bu dosyanın *kaynağını* `/src/api.ts` isteğine olduğu gibi servis eder;
// böylece arayüz Tauri olmadan gerçek tarayıcıda çalışır. Burada yalnız arayüzün
// `./api` üzerinden kullandığı üç dışa aktarım (api, onDownloads, onNotice) vardır.
//
// Çağrı kayıtları `window.__hydraCalls`, hazır veriler `window.__hydraMock` altında.

const NOW = Date.now();

const DOWNLOAD_DIR = "C:\\Users\\titan\\Downloads\\Hydra";

const SETTINGS = {
  downloadDir: DOWNLOAD_DIR,
  maxConcurrent: 3,
  connections: 8,
  speedLimitKbps: 0,
  captureEnabled: true,
  capturePort: 6712,
  ytdlpPath: "",
  autoResume: true,
  askOnDownload: false,
  hunterEnabled: false,
  webEnabled: true,
  webPort: 6790,
  webBind: "0.0.0.0",
  webToken: "ev2026",
  torrentEnabled: true,
};

const APP_INFO = {
  version: "0.2.0",
  downloadDir: DOWNLOAD_DIR,
  statePath: "C:\\Users\\titan\\AppData\\Roaming\\hydra-dl\\downloads.json",
  capture: {
    enabled: true,
    running: true,
    port: 6712,
    url: "http://127.0.0.1:6712",
  },
  web: {
    enabled: true,
    running: true,
    port: 6790,
    bind: "0.0.0.0",
    url: "http://127.0.0.1:6790",
    lanUrls: ["http://192.168.1.42:6790"],
    tokenRequired: true,
  },
  webQr: null,
  ytdlp: {
    available: true,
    path: "C:\\tools\\yt-dlp.exe",
    version: "2026.10.01",
  },
  uptimeSecs: 3720,
};

const DOWNLOADS = [
  {
    id: "dl-http-1",
    url: "https://cdn.example.com/belgesel-4k.mp4",
    filename: "belgesel-4k.mp4",
    dir: DOWNLOAD_DIR,
    kind: "http",
    status: "downloading",
    totalBytes: 734003200,
    downloadedBytes: 268435456,
    segments: [
      { index: 0, start: 0, end: 183500799, done: 183500799 },
      { index: 1, start: 183500800, end: 367001599, done: 84934656 },
      { index: 2, start: 367001600, end: 550502399, done: 0 },
      { index: 3, start: 550502400, end: 734003199, done: 0 },
    ],
    resumable: true,
    speedBps: 5242880,
    progress: 0.3657,
    error: null,
    createdAt: NOW - 60000,
    finishedAt: null,
    connections: 8,
    referer: null,
    mime: "video/mp4",
    filePath: null,
    format: null,
    swarm: null,
    source: "web",
  },
  {
    id: "dl-torrent-2",
    url: "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=arsiv-2026",
    filename: "arsiv-2026.tar.zst",
    dir: DOWNLOAD_DIR,
    kind: "torrent",
    status: "completed",
    totalBytes: 2147483648,
    downloadedBytes: 2147483648,
    segments: [],
    resumable: true,
    speedBps: 0,
    progress: 1,
    error: null,
    createdAt: NOW - 3600000,
    finishedAt: NOW - 600000,
    connections: 0,
    referer: null,
    mime: null,
    filePath: `${DOWNLOAD_DIR}\\arsiv-2026.tar.zst`,
    format: null,
    swarm: { peers: 0, seeders: 0 },
    source: "extension",
  },
  {
    id: "dl-media-3",
    url: "https://video.example.net/ders-12.mp4",
    filename: "ders-12.mp4",
    dir: DOWNLOAD_DIR,
    kind: "media",
    status: "failed",
    totalBytes: null,
    downloadedBytes: 1048576,
    segments: [],
    resumable: false,
    speedBps: 0,
    progress: null,
    error: "Bağlantı zaman aşımına uğradı (HTTP 504)",
    createdAt: NOW - 900000,
    finishedAt: NOW - 880000,
    connections: 1,
    referer: "https://video.example.net/izle/12",
    mime: "video/mp4",
    filePath: `${DOWNLOAD_DIR}\\ders-12.mp4.hydra-tmp`,
    format: "137+140",
    swarm: null,
    source: null,
  },
];

const PROBE_RESULT = {
  ok: true,
  url: "https://example.com/video.mp4",
  finalUrl: "https://cdn.example.com/video.mp4",
  kind: "media",
  title: "Örnek video — 4K belgesel",
  filename: "ornek-video.mp4",
  size: 734003200,
  mime: "video/mp4",
  durationSecs: 754,
  resumable: true,
  needsYtdlp: true,
  formats: [
    {
      id: "direct",
      label: "Doğrudan dosya (mp4)",
      ext: "mp4",
      filesize: 734003200,
      note: "en iyi tek dosya",
    },
    {
      id: "137",
      label: "1080p · mp4",
      ext: "mp4",
      height: 1080,
      filesize: 524288000,
      vcodec: "avc1.640028",
      acodec: "none",
    },
    {
      id: "140",
      label: "Ses · m4a",
      ext: "m4a",
      filesize: 18874368,
      vcodec: "none",
      acodec: "mp4a.40.2",
      note: "video ile birleştirilir",
    },
  ],
  candidates: [
    {
      url: "https://cdn.example.com/video.mp4",
      kind: "media",
      label: "Ana kaynak",
      size: 734003200,
    },
    {
      url: "https://yedek.example.org/video.mp4",
      kind: "media",
      label: "Yedek kaynak",
      size: 734003200,
    },
  ],
  warnings: ["Bu sayfada 2 reklam bağlantısı atlandı."],
};

const calls = {
  list: 0,
  add: [],
  probe: [],
  pause: [],
  resume: [],
  cancel: [],
  remove: [],
  clearFinished: [],
  getSettings: 0,
  saveSettings: [],
  appInfo: 0,
  captureStatus: 0,
  webStatus: 0,
  openDownload: [],
  revealDownload: [],
};

let listHandler = null;

const wait = () => new Promise((resolve) => setTimeout(resolve, 60));

export const api = {
  list: async () => {
    calls.list += 1;
    await wait();
    return [];
  },
  add: async (args) => {
    calls.add.push({ ...args });
    await wait();
    return { ...DOWNLOADS[0], id: `dl-added-${calls.add.length}`, url: args.url };
  },
  probe: async (url, media) => {
    calls.probe.push({ url, media });
    await wait();
    return { ...PROBE_RESULT, url, finalUrl: PROBE_RESULT.finalUrl };
  },
  pause: async (id) => {
    calls.pause.push(id);
  },
  resume: async (id) => {
    calls.resume.push(id);
  },
  cancel: async (id) => {
    calls.cancel.push(id);
  },
  remove: async (id, deleteFile) => {
    calls.remove.push({ id, deleteFile });
  },
  clearFinished: async (deleteFiles) => {
    calls.clearFinished.push({ deleteFiles });
  },
  getSettings: async () => {
    calls.getSettings += 1;
    await wait();
    return { ...SETTINGS };
  },
  saveSettings: async (settings) => {
    calls.saveSettings.push({ ...settings });
    await wait();
    return { ...settings };
  },
  appInfo: async () => {
    calls.appInfo += 1;
    await wait();
    return { ...APP_INFO };
  },
  captureStatus: async () => {
    calls.captureStatus += 1;
    return { ...APP_INFO.capture };
  },
  webStatus: async () => {
    calls.webStatus += 1;
    return { ...APP_INFO.web };
  },
  openDownload: async (id) => {
    calls.openDownload.push(id);
  },
  revealDownload: async (id) => {
    calls.revealDownload.push(id);
  },
  installYtdlp: async () => {
    calls.installYtdlp = (calls.installYtdlp ?? 0) + 1;
    return { available: true, path: "C:/mock/bin/yt-dlp.exe", version: "2026.10.21" };
  },
};

export function onDownloads(handler) {
  listHandler = handler;
  // Anlık durum: kuyruk henüz boş, 3 kayıt `pushList()` ile olay üzerinden gelir.
  handler([]);
  return Promise.resolve(() => {
    listHandler = null;
  });
}

export function onNotice(handler) {
  return Promise.resolve(() => {});
}

// `src/api.ts` her `on*` dinleyicisi için export verir; mock aynı yüzeyi
// sağlamalı, yoksa arayüz import aşamasında patlar.
export function onYtdlpInstall(handler) {
  return Promise.resolve(() => {});
}

window.__hydraCalls = calls;
window.__hydraMock = {
  settings: SETTINGS,
  appInfo: APP_INFO,
  downloads: DOWNLOADS,
  probe: PROBE_RESULT,
  pushList: () => {
    if (!listHandler) return false;
    listHandler(DOWNLOADS.map((item) => ({ ...item })));
    return true;
  },
};
