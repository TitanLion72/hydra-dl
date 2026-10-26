// Hydra DL — eklenti ve panel arasında paylaşılan yardımcılar.
// Klasik script: module/import yok. Service worker'da importScripts ile yüklenir.
"use strict";

const DEFAULT_SETTINGS = {
  port: 6712,
  askOnDownload: true,
  hunterEnabled: false,
  showPanel: true,
};

const HYDRA_TIMEOUT_MS = 4000;
// /probe bir video sayfasında yt-dlp'yi çağırır; 4 sn yetmez, ayrı ve uzun tutulur.
const HYDRA_PROBE_TIMEOUT_MS = 30000;
const HYDRA_HOST = "127.0.0.1";
const PORT_MIN = 1024;
const PORT_MAX = 65535;

// Firefox MV3, host izinlerini kurulumda otomatik vermez; kullanıcı onayı ister.
// Chrome/Edge'de bu izinler kurulumda gelir, kontrol her zaman `true` döner.
const LOCAL_ORIGINS = ["http://127.0.0.1/*", "http://localhost/*"];

async function hasLocalAccess() {
  try {
    if (!HX.api.permissions?.contains) return true;
    return await HX.api.permissions.contains({ origins: LOCAL_ORIGINS });
  } catch {
    return true;
  }
}

/// Yalnız kullanıcı tıklamasıyla çağrılmalı (tarayıcı jesti gerekir).
async function requestLocalAccess() {
  try {
    if (!HX.api.permissions?.request) return false;
    return await HX.api.permissions.request({ origins: LOCAL_ORIGINS });
  } catch {
    return false;
  }
}

function normalizePort(value) {
  const port = Number(value);
  return Number.isInteger(port) && port >= PORT_MIN && port <= PORT_MAX ? port : DEFAULT_SETTINGS.port;
}

class HydraError extends Error {
  constructor(message, options = {}) {
    super(message);
    this.name = "HydraError";
    this.status = options.status || 0;
    this.code = options.code || "network";
    this.payload = options.payload || null;
  }
}

async function getSettings() {
  try {
    const stored = await HX.api.storage.sync.get(DEFAULT_SETTINGS);
    return {
      ...DEFAULT_SETTINGS,
      ...stored,
      port: normalizePort(stored.port),
      askOnDownload: stored.askOnDownload !== false,
      hunterEnabled: stored.hunterEnabled === true,
      showPanel: stored.showPanel !== false,
    };
  } catch (_err) {
    return { ...DEFAULT_SETTINGS };
  }
}

async function saveSettings(patch) {
  const clean = {};
  if (patch && patch.port !== undefined) {
    clean.port = normalizePort(patch.port);
  }
  for (const key of ["askOnDownload", "hunterEnabled", "showPanel"]) {
    if (patch && patch[key] !== undefined) clean[key] = patch[key] === true;
  }
  try {
    await HX.api.storage.sync.set(clean);
  } catch (_err) {
    // storage kapalıysa sessizce varsayılanlarla devam
  }
  return getSettings();
}

async function hydraBase() {
  const settings = await getSettings();
  return `http://${HYDRA_HOST}:${settings.port}`;
}

async function hydraFetch(path, opts = {}) {
  const base = await hydraBase();
  const controller = new AbortController();
  const timeoutMs = Number(opts.timeoutMs) > 0 ? Number(opts.timeoutMs) : HYDRA_TIMEOUT_MS;
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  const method = opts.method || "GET";
  const headers = { Accept: "application/json", ...(opts.headers || {}) };
  const init = { method, headers, signal: controller.signal, cache: "no-store" };
  if (opts.body !== undefined) {
    headers["Content-Type"] = "application/json";
    init.body = JSON.stringify(opts.body);
  }

  try {
    const response = await fetch(base + path, init);
    const text = await response.text();
    let data = null;
    if (text) {
      try {
        data = JSON.parse(text);
      } catch (_err) {
        data = null;
      }
    }

    if (!response.ok) {
      const detail = data && (data.error || data.message);
      const unsupported = response.status === 404 || response.status === 405;
      throw new HydraError(detail || `Hydra DL ${response.status} döndürdü`, {
        status: response.status,
        code: unsupported ? "unsupported" : "http",
        payload: data,
      });
    }
    if (!data || typeof data !== "object") {
      throw new HydraError("Hydra DL geçersiz yanıt verdi (JSON bekleniyordu)", {
        status: response.status,
        code: "bad-json",
      });
    }
    if (data.ok === false) {
      throw new HydraError(data.error || "Hydra DL isteği reddetti", {
        status: response.status,
        code: "refused",
        payload: data,
      });
    }
    return data;
  } catch (err) {
    if (err instanceof HydraError) throw err;
    if (err && err.name === "AbortError") {
      const limit = timeoutMs < 1000 ? `${timeoutMs} ms` : `${Math.round(timeoutMs / 1000)} sn`;
      throw new HydraError(`Hydra DL yanıt vermedi (${limit} zaman aşımı)`, { code: "timeout" });
    }
    throw new HydraError(
      `Hydra DL'ye ulaşılamadı: ${err && err.message ? err.message : String(err)}`,
      { code: "network" },
    );
  } finally {
    clearTimeout(timer);
  }
}

async function hydraHealth() {
  return hydraFetch("/health");
}

async function hydraProbe(url, media) {
  const body = { url };
  if (media !== undefined && media !== null) body.media = media === true;
  return hydraFetch("/probe", { method: "POST", body, timeoutMs: HYDRA_PROBE_TIMEOUT_MS });
}

async function hydraAdd(payload) {
  const body = { url: payload.url };
  if (payload.format) body.format = payload.format;
  if (payload.filename) body.filename = payload.filename;
  if (payload.dir) body.dir = payload.dir;
  if (payload.referer) body.referer = payload.referer;
  if (payload.media !== undefined) body.media = payload.media === true;

  try {
    return await hydraFetch("/add", { method: "POST", body });
  } catch (err) {
    // Yalnız "uç yok" (404/405) durumunda eski GET biçimine düş: 400 gibi
    // gerçek reddetmeleri tekrar denemek çift ekleme riski taşır.
    const retryable =
      err instanceof HydraError && (err.code === "unsupported" || err.status === 404 || err.status === 405);
    if (!retryable) throw err;
    return hydraAddLegacy(payload);
  }
}

// Eski yakalama sunucusu yalnız GET /add?url=... destekliyor.
async function hydraAddLegacy(payload) {
  const params = new URLSearchParams({ url: payload.url });
  if (payload.format) params.set("format", payload.format);
  if (payload.filename) params.set("filename", payload.filename);
  if (payload.dir) params.set("dir", payload.dir);
  if (payload.referer) params.set("referer", payload.referer);
  if (payload.media !== undefined) params.set("media", payload.media ? "true" : "false");
  return hydraFetch(`/add?${params.toString()}`);
}

async function hydraSettings() {
  return hydraFetch("/settings");
}

function formatBytes(value) {
  const n = Number(value);
  if (!Number.isFinite(n) || n <= 0) return "bilinmiyor";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = n;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  const digits = size >= 100 || unit === 0 ? 0 : size >= 10 ? 1 : 2;
  return `${size.toFixed(digits).replace(".", ",")} ${units[unit]}`;
}

function notify(title, message) {
  try {
    // Firefox `browser.notifications.create` promise döner, Chrome callback bekler;
    // promise'ı yutmak ikisinde de güvenli.
    const result = HX.api.notifications.create({
      type: "basic",
      iconUrl: "icons/icon128.png",
      title: title || "Hydra DL",
      message: String(message || "").slice(0, 300),
    });
    if (result && typeof result.catch === "function") result.catch(() => {});
  } catch (_err) {
    // bildirim izni yoksa yut
  }
}

function siteHost(url) {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch (_err) {
    return "";
  }
}

function guessFilename(url) {
  try {
    const parsed = new URL(url);
    const last = decodeURIComponent(parsed.pathname.split("/").filter(Boolean).pop() || "");
    if (last && /\.[a-z0-9]{2,5}$/i.test(last)) return last;
  } catch (_err) {
    // yut
  }
  return "indirme";
}

if (typeof window !== "undefined") {
  window.HydraShared = {
    DEFAULT_SETTINGS,
    PORT_MIN,
    PORT_MAX,
    HydraError,
    getSettings,
    saveSettings,
    normalizePort,
    hydraBase,
    hydraFetch,
    hydraHealth,
    hydraProbe,
    hydraAdd,
    hydraSettings,
    formatBytes,
    notify,
    siteHost,
    guessFilename,
    hasLocalAccess,
    requestLocalAccess,
  };
}
