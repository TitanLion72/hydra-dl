/* Hydra DL — mobil web arayüzü.
 * Derleme yok, bağımlılık yok. Sunucu bu dosyaları kökten servis eder.
 * API sözleşmesi: docs/API.md
 */

const TOKEN_KEY = "hydra.token";
const REQUEST_TIMEOUT = 8000;

const ENDPOINTS = {
  health: "./health",
  downloads: "./downloads",
  events: "./events",
  probe: "./probe",
  add: "./add",
  settings: "./settings",
  ytdlp: "./ytdlp",
  ytdlpInstall: "./ytdlp/install",
  torrentFiles: "./torrent/files",
};

/* ---------------- Kısayollar ---------------- */

const $ = (selector, root = document) => root.querySelector(selector);

const els = {
  conn: $("#conn"),
  connText: $("#connText"),
  settingsBtn: $("#settingsBtn"),

  quickForm: $("#quickForm"),
  pasteUrl: $("#pasteUrl"),

  filters: $(".filters"),
  list: $("#list"),
  skeleton: $("#listSkeleton"),
  empty: $("#emptyState"),
  error: $("#errorState"),
  errorText: $("#errorText"),
  retryBtn: $("#retryBtn"),
  filterEmpty: $("#filterEmpty"),

  views: {
    downloads: $("#view-downloads"),
    new: $("#view-new"),
    settings: $("#view-settings"),
  },
  tabs: Array.from(document.querySelectorAll(".tab")),

  newForm: $("#newForm"),
  newUrl: $("#newUrl"),
  newFilename: $("#newFilename"),
  newMedia: $("#newMedia"),
  newProbe: $("#newProbe"),
  newAdd: $("#newAdd"),
  newStatus: $("#newStatus"),

  settingsForm: $("#settingsForm"),
  settingsLoading: $("#settingsLoading"),
  settingsError: $("#settingsError"),
  setDir: $("#setDir"),
  setMaxConcurrent: $("#setMaxConcurrent"),
  setConnections: $("#setConnections"),
  setSpeed: $("#setSpeed"),
  setAutoResume: $("#setAutoResume"),
  setNotify: $("#setNotify"),
  setCapture: $("#setCapture"),
  setCapturePort: $("#setCapturePort"),
  setWebUrl: $("#setWebUrl"),
  setVersion: $("#setVersion"),
  setUptime: $("#setUptime"),
  setYtdlp: $("#setYtdlp"),
  installYtdlp: $("#installYtdlp"),
  ytdlpStatus: $("#ytdlpStatus"),
  ytdlpHelp: $("#ytdlpHelp"),
  setToken: $("#setToken"),
  settingsStatus: $("#settingsStatus"),
  settingsSave: $("#settingsSave"),
  settingsReload: $("#settingsReload"),

  sheet: $("#sheet"),
  sheetPanel: $("#sheetPanel"),
  sheetTitle: $("#sheetTitle"),
  sheetClose: $("#sheetClose"),
  sheetLoading: $("#sheetLoading"),
  sheetError: $("#sheetError"),
  sheetContent: $("#sheetContent"),
  probeTitle: $("#probeTitle"),
  probeSub: $("#probeSub"),
  probeFacts: $("#probeFacts"),
  probeWarnings: $("#probeWarnings"),
  formatGroup: $("#formatGroup"),
  formatList: $("#formatList"),
  candidateGroup: $("#candidateGroup"),
  candidateList: $("#candidateList"),
  fileGroup: $("#fileGroup"),
  fileSummary: $("#fileSummary"),
  probeSize: null,
  fileList: $("#fileList"),
  filesAll: $("#filesAll"),
  filesNone: $("#filesNone"),
  probeFilename: $("#probeFilename"),
  sheetAdd: $("#sheetAdd"),

  toast: $("#toast"),
};

/* ---------------- Durum ---------------- */

const state = {
  token: "",
  items: [],
  rows: new Map(),
  filter: "all",
  view: "downloads",
  health: null,
  rawSettings: null,
  probe: null,
  probeUrl: "",
  firstLoad: true,
  loaded: false,
  connected: false,
  lastFocused: null,
  renderQueued: false,
  pendingItems: null,
  settingsLoaded: false,
};

/* ---------------- Kimlik doğrulama ---------------- */

function readToken() {
  try {
    const url = new URL(window.location.href);
    const fromUrl = url.searchParams.get("token");
    if (fromUrl) {
      try {
        localStorage.setItem(TOKEN_KEY, fromUrl);
      } catch {
        /* gizli mod: yalnız bu oturum */
      }
      url.searchParams.delete("token");
      try {
        window.history.replaceState(null, "", url.pathname + url.search + url.hash);
      } catch {
        /* file:// altında replaceState kısıtlı olabilir */
      }
      return fromUrl;
    }
  } catch {
    /* URL okunamadıysa localStorage'a düş */
  }
  try {
    return localStorage.getItem(TOKEN_KEY) || "";
  } catch {
    return "";
  }
}

function storeToken(value) {
  state.token = value;
  try {
    if (value) localStorage.setItem(TOKEN_KEY, value);
    else localStorage.removeItem(TOKEN_KEY);
  } catch {
    /* yoksay */
  }
}

/* ---------------- HTTP ---------------- */

function apiError(message, status) {
  const error = new Error(message);
  if (status) error.status = status;
  return error;
}

async function api(path, { method = "GET", body, timeout = REQUEST_TIMEOUT } = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  const headers = { Accept: "application/json" };
  if (state.token) headers["X-Hydra-Token"] = state.token;
  if (body !== undefined) headers["Content-Type"] = "application/json";

  try {
    const response = await fetch(path, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: controller.signal,
      cache: "no-store",
      credentials: "same-origin",
    });

    const text = await response.text();
    let data = null;
    if (text) {
      try {
        data = JSON.parse(text);
      } catch {
        data = null;
      }
    }

    if (!response.ok) {
      if (response.status === 401 || response.status === 403) {
        throw apiError("Erişim anahtarı geçersiz. Ayarlar'dan token gir.", response.status);
      }
      const message =
        data && typeof data.error === "string"
          ? data.error
          : `Sunucu hatası (${response.status})`;
      throw apiError(message, response.status);
    }
    return data;
  } catch (error) {
    if (error && error.name === "AbortError") {
      const seconds = Math.round(timeout / 1000);
      throw apiError(`Sunucu ${seconds} saniyede yanıt vermedi.`);
    }
    if (error instanceof TypeError) {
      throw apiError("Sunucuya ulaşılamadı.");
    }
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

/* ---------------- Biçimlendirme (tr-TR) ---------------- */

const numberFormat = new Intl.NumberFormat("tr-TR", {
  maximumFractionDigits: 1,
  minimumFractionDigits: 0,
});
const percentFormat = new Intl.NumberFormat("tr-TR", { maximumFractionDigits: 0 });

function formatBytes(bytes) {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${numberFormat.format(value)} ${units[unit]}`;
}

function formatSpeed(bps) {
  if (!Number.isFinite(bps) || bps <= 0) return "";
  return `${formatBytes(bps)}/sn`;
}

function formatDuration(seconds) {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  if (seconds < 1) return "1 sn'den az";
  const total = Math.round(seconds);
  if (total < 60) return `${total} sn`;
  const minutes = Math.floor(total / 60);
  const secs = total % 60;
  if (minutes < 60) return secs ? `${minutes} dk ${secs} sn` : `${minutes} dk`;
  const hours = Math.floor(minutes / 60);
  const mins = minutes % 60;
  return mins ? `${hours} sa ${mins} dk` : `${hours} sa`;
}

function formatUptime(seconds) {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  return formatDuration(seconds);
}

function formatPercent(ratio) {
  if (!Number.isFinite(ratio)) return "—";
  return `%${percentFormat.format(Math.max(0, Math.min(1, ratio)) * 100)}`;
}

const STATUS_LABELS = {
  queued: "Bekliyor",
  downloading: "İndiriliyor",
  paused: "Duraklatıldı",
  completed: "Bitti",
  failed: "Hata",
  cancelled: "İptal",
};

const KIND_LABELS = {
  http: "HTTP",
  media: "Medya",
  torrent: "Torrent",
  hls: "HLS",
  file: "Dosya",
  page: "Sayfa",
  unknown: "Bilinmiyor",
};

function statusLabel(status) {
  return STATUS_LABELS[status] || String(status || "Bilinmiyor");
}

function kindLabel(kind) {
  return KIND_LABELS[kind] || String(kind || "").toUpperCase();
}

function progressRatio(download) {
  const total = download.totalBytes;
  const downloaded = download.downloadedBytes;
  if (Number.isFinite(total) && total > 0 && Number.isFinite(downloaded)) {
    return Math.max(0, Math.min(1, downloaded / total));
  }
  if (typeof download.progress === "number" && Number.isFinite(download.progress)) {
    return Math.max(0, Math.min(1, download.progress));
  }
  return null;
}

function etaSeconds(download) {
  const total = download.totalBytes;
  const done = download.downloadedBytes;
  const speed = download.speedBps;
  if (!Number.isFinite(total) || !Number.isFinite(done) || !Number.isFinite(speed) || speed <= 0) {
    return null;
  }
  const left = total - done;
  if (left <= 0) return 0;
  return left / speed;
}

function parseNumberish(value) {
  if (typeof value === "number") return value;
  if (typeof value !== "string") return NaN;
  // type=number alanları nokta ondalık kullanır; kullanıcı virgül yazarsa da kabul et.
  return Number.parseFloat(value.trim().replace(",", "."));
}

/* ---------------- Bağlantı göstergesi ---------------- */

function setConnection(mode, text) {
  const node = els.conn;
  if (!node) return;
  node.classList.remove("is-live", "is-pending", "is-down");
  node.classList.add(`is-${mode}`);
  els.connText.textContent = text;
}

/* ---------------- Görünüm değiştirme ---------------- */

function showView(name) {
  if (!els.views[name]) return;
  state.view = name;
  for (const [key, node] of Object.entries(els.views)) {
    const active = key === name;
    node.hidden = !active;
    if (active) node.removeAttribute("inert");
    else node.setAttribute("inert", "");
  }
  for (const tab of els.tabs) {
    const active = tab.dataset.view === name;
    tab.classList.toggle("is-active", active);
    if (active) tab.setAttribute("aria-current", "page");
    else tab.removeAttribute("aria-current");
  }
  window.scrollTo({ top: 0, behavior: "auto" });
  if (name === "settings" && !state.settingsLoaded) loadSettings();
  if (name === "downloads") renderEmptyStates();
}

/* ---------------- Liste oluşturma (keyed) ---------------- */

const CARD_MARKUP = `
  <div class="card-main">
    <div class="card-top">
      <span class="badge" data-ref="badge"></span>
      <span class="card-kind" data-ref="kind"></span>
    </div>
    <h3 class="card-title" data-ref="title"></h3>
    <div class="bar" data-ref="bar"><div class="bar-fill" data-ref="fill"></div></div>
    <div class="card-meta">
      <span data-ref="progress"></span>
      <span data-ref="speed"></span>
    </div>
    <p class="card-error" data-ref="error" hidden></p>
  </div>
  <div class="card-actions">
    <button class="act" type="button" data-action="pause" data-ref="pause" aria-label="Duraklat">
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="M9 5v14M15 5v14" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" />
      </svg>
      <span>Duraklat</span>
    </button>
    <button class="act" type="button" data-action="resume" data-ref="resume" aria-label="Devam ettir">
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="M8 5.5v13l10-6.5-10-6.5Z" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linejoin="round" />
      </svg>
      <span>Devam</span>
    </button>
    <button class="act act-danger" type="button" data-action="cancel" data-ref="cancel" aria-label="İptal et">
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="M6 6l12 12M18 6 6 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
      </svg>
      <span>İptal</span>
    </button>
    <button class="act" type="button" data-action="copy" data-ref="copy" aria-label="Dosya yolunu kopyala">
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <rect x="9" y="9" width="11" height="11" rx="2.4" fill="none" stroke="currentColor" stroke-width="1.7" />
        <path d="M15 5.5A2.5 2.5 0 0 0 12.5 3H6.5A3.5 3.5 0 0 0 3 6.5v6A2.5 2.5 0 0 0 5.5 15" fill="none" stroke="currentColor" stroke-width="1.7" />
      </svg>
      <span>Kopyala</span>
    </button>
    <button class="act act-danger" type="button" data-action="remove" data-ref="remove" aria-label="Listeden sil">
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="M4.5 7h15M9.5 7V4.8h5V7M6.8 7l.8 12.2h8.8L17.2 7" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span>Sil</span>
    </button>
  </div>
`;

const ACTION_SETS = {
  queued: ["pause", "cancel"],
  downloading: ["pause", "cancel"],
  paused: ["resume", "cancel"],
  completed: ["copy", "remove"],
  failed: ["resume", "remove"],
  cancelled: ["remove"],
};

function createRow(download) {
  const li = document.createElement("li");
  li.className = "card";
  li.dataset.id = download.id;
  li.setAttribute("role", "listitem");
  li.innerHTML = CARD_MARKUP; // yalnızca statik şablon; kullanıcı verisi sonradan textContent ile

  const refs = {};
  for (const node of li.querySelectorAll("[data-ref]")) refs[node.dataset.ref] = node;

  const row = {
    el: li,
    refs,
    memo: {},
    actions: "",
  };
  refs.actions = li.querySelector(".card-actions");
  state.rows.set(download.id, row);
  return row;
}

function setText(node, value) {
  if (!node) return;
  const text = value === null || value === undefined ? "" : String(value);
  if (node.textContent !== text) node.textContent = text;
}

function setValue(node, value) {
  if (!node) return;
  const text = value === null || value === undefined ? "" : String(value);
  if (node.value !== text) node.value = text;
}

function updateRow(row, download) {
  const { refs, memo } = row;

  const badgeLabel = statusLabel(download.status);
  if (memo.badge !== badgeLabel || memo.badgeStatus !== download.status) {
    refs.badge.className = `badge st-${download.status || "queued"}`;
    setText(refs.badge, badgeLabel);
    memo.badge = badgeLabel;
    memo.badgeStatus = download.status;
  }

  let kindText = kindLabel(download.kind);
  if (download.kind === "torrent" && Array.isArray(download.torrentFiles)) {
    const files = download.torrentFiles;
    if (files.length > 1) {
      const picked = files.filter((file) => file.included).length;
      kindText += ` · ${picked}/${files.length} dosya`;
    }
  }
  if (memo.kind !== kindText) {
    setText(refs.kind, kindText);
    memo.kind = kindText;
  }

  const title = download.filename || download.url || "Adsız indirme";
  if (memo.title !== title) {
    setText(refs.title, title);
    refs.title.title = title;
    memo.title = title;
  }

  const ratio = progressRatio(download);
  const pct = ratio === null ? null : Math.round(ratio * 1000) / 10;
  const indeterminate = download.status === "downloading" && ratio === null;

  if (memo.pct !== pct || memo.indeterminate !== indeterminate) {
    refs.bar.classList.toggle("is-indeterminate", indeterminate);
    refs.fill.style.width = `${pct === null ? 0 : pct}%`;
    memo.pct = pct;
    memo.indeterminate = indeterminate;
  }

  const fillClass =
    download.status === "completed"
      ? "bar-fill is-done"
      : download.status === "failed" || download.status === "cancelled"
        ? "bar-fill is-error"
        : download.status === "paused"
          ? "bar-fill is-paused"
          : "bar-fill";
  if (memo.fillClass !== fillClass) {
    refs.fill.className = fillClass;
    memo.fillClass = fillClass;
  }

  const sizeText =
    Number.isFinite(download.totalBytes) && download.totalBytes > 0
      ? `${formatBytes(download.downloadedBytes)} / ${formatBytes(download.totalBytes)}`
      : `${formatBytes(download.downloadedBytes)}`;
  const progressText = indeterminate ? "Boyut bilinmiyor" : `${sizeText} · ${formatPercent(ratio)}`;
  if (memo.progressText !== progressText) {
    setText(refs.progress, progressText);
    memo.progressText = progressText;
  }

  let speedText = "";
  if (download.status === "downloading" && download.retry) {
    const wait = Math.max(1, Math.round(download.retry.waitMs / 1000));
    speedText = `Yeniden deneniyor ${download.retry.attempt}/${download.retry.max} · ${wait} sn`;
  } else if (download.status === "downloading") {
    const speed = formatSpeed(download.speedBps);
    const eta = etaSeconds(download);
    if (speed) speedText = eta === null ? speed : `${speed} · kalan ${formatDuration(eta)}`;
    else speedText = "Hız ölçülüyor…";
  } else if (download.status === "queued") {
    speedText = "Sırada";
  } else if (download.status === "paused") {
    speedText = "Duraklatıldı";
  } else if (download.status === "completed" && download.finishedAt) {
    speedText = new Date(download.finishedAt).toLocaleTimeString("tr-TR", {
      hour: "2-digit",
      minute: "2-digit",
    });
  }
  if (memo.speedText !== speedText) {
    setText(refs.speed, speedText);
    memo.speedText = speedText;
  }
  if (refs.speed) refs.speed.classList.toggle("is-retry", Boolean(download.retry));

  const errorText = download.error ? String(download.error) : "";
  if (memo.errorText !== errorText) {
    refs.error.hidden = !errorText;
    setText(refs.error, errorText);
    memo.errorText = errorText;
  }

  const list = ACTION_SETS[download.status] || ACTION_SETS.queued;
  const signature = list.join(",");
  if (row.actions !== signature) {
    for (const key of ["pause", "resume", "cancel", "copy", "remove"]) {
      const button = refs[key];
      if (!button) continue;
      const visible = list.includes(key);
      button.hidden = !visible;
      const label = download.filename || "indirme";
      if (key === "pause") button.setAttribute("aria-label", `${label} — duraklat`);
      if (key === "resume") button.setAttribute("aria-label", `${label} — devam ettir`);
      if (key === "cancel") button.setAttribute("aria-label", `${label} — iptal et`);
      if (key === "copy") button.setAttribute("aria-label", `${label} — dosya yolunu kopyala`);
      if (key === "remove") button.setAttribute("aria-label", `${label} — listeden sil`);
    }
    row.actions = signature;
  }

  row.el.classList.toggle("is-completed", download.status === "completed");
  row.el.dataset.status = download.status || "";
}

/* ---------------- Filtre ---------------- */

function matchesFilter(download, filter) {
  switch (filter) {
    case "active":
      return download.status === "downloading" || download.status === "paused";
    case "queued":
      return download.status === "queued";
    case "done":
      return download.status === "completed";
    case "problem":
      return download.status === "failed" || download.status === "cancelled";
    default:
      return true;
  }
}

function refreshCounts() {
  const counts = { all: state.items.length, active: 0, queued: 0, done: 0, problem: 0 };
  for (const download of state.items) {
    if (matchesFilter(download, "active")) counts.active += 1;
    if (matchesFilter(download, "queued")) counts.queued += 1;
    if (matchesFilter(download, "done")) counts.done += 1;
    if (matchesFilter(download, "problem")) counts.problem += 1;
  }
  for (const node of document.querySelectorAll("[data-count]")) {
    setText(node, counts[node.dataset.count] ?? 0);
  }
  return counts;
}

function applyFilter() {
  let visible = 0;
  for (const download of state.items) {
    const row = state.rows.get(download.id);
    if (!row) continue;
    const show = matchesFilter(download, state.filter);
    row.el.hidden = !show;
    if (show) visible += 1;
  }
  els.filterEmpty.hidden = !(state.loaded && state.items.length > 0 && visible === 0);
  return visible;
}

function renderEmptyStates() {
  if (!state.loaded) return;
  els.skeleton.hidden = true;
  const failing = !els.error.hidden;
  const noItems = state.items.length === 0;
  els.empty.hidden = failing || !noItems || state.view !== "downloads";
  applyFilter();
}

/* ---------------- Liste güncelleme ---------------- */

function applyDownloads(items) {
  if (!Array.isArray(items)) return;

  const seen = new Set();
  let cursor = els.list.firstElementChild;

  for (const download of items) {
    if (!download || typeof download.id !== "string") continue;
    seen.add(download.id);

    let row = state.rows.get(download.id);
    if (!row) row = createRow(download);
    updateRow(row, download);

    if (row.el === cursor) {
      cursor = cursor.nextElementSibling;
    } else {
      els.list.insertBefore(row.el, cursor);
    }
  }

  for (const [id, row] of state.rows) {
    if (seen.has(id)) continue;
    row.el.remove();
    state.rows.delete(id);
  }

  state.items = items.filter((item) => item && typeof item.id === "string");
  state.loaded = true;
  els.skeleton.hidden = true;
  els.error.hidden = true;
  refreshCounts();
  renderEmptyStates();
  applyFilter();
}

function scheduleDownloads(items) {
  state.pendingItems = items;
  if (state.renderQueued) return;
  state.renderQueued = true;
  requestAnimationFrame(() => {
    state.renderQueued = false;
    const pending = state.pendingItems;
    state.pendingItems = null;
    if (pending) applyDownloads(pending);
  });
}

/* ---------------- İlk yükleme ve hata ---------------- */

function showLoadError(message) {
  state.loaded = true;
  els.skeleton.hidden = true;
  els.error.hidden = false;
  els.empty.hidden = true;
  els.filterEmpty.hidden = true;
  const base = "Aynı Wi-Fi'da mısın? Masaüstü uygulaması açık mı?";
  setText(els.errorText, message ? `${message} ${base}` : base);
}

async function loadDownloads() {
  try {
    const data = await api(ENDPOINTS.downloads);
    if (Array.isArray(data)) applyDownloads(data);
    state.firstLoad = false;
    return true;
  } catch (error) {
    if (state.items.length === 0) showLoadError(error.message);
    return false;
  }
}

async function loadHealth() {
  try {
    const health = await api(ENDPOINTS.health);
    state.health = health && typeof health === "object" ? health : null;
    updateHealthFacts();
    return health;
  } catch {
    return null;
  }
}

/* ---------------- SSE ---------------- */

let eventSource = null;
let reconnectTimer = null;
let retryDelay = 2000;

function setConnected(live) {
  state.connected = live;
  if (live) setConnection("live", "Bağlı");
  else setConnection("pending", "Yeniden bağlanıyor…");
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  if (navigator.onLine === false) {
    setConnection("down", "Çevrimdışı");
    return;
  }
  setConnection("pending", "Yeniden bağlanıyor…");
  const delay = retryDelay;
  retryDelay = Math.min(30000, Math.round(retryDelay * 1.8));
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectEvents();
  }, delay);
}

function closeEvents() {
  if (eventSource) {
    eventSource.onopen = null;
    eventSource.onmessage = null;
    eventSource.onerror = null;
    try {
      eventSource.close();
    } catch {
      /* yoksay */
    }
    eventSource = null;
  }
}

function connectEvents() {
  closeEvents();
  if (typeof EventSource !== "function") {
    setConnection("down", "SSE desteklenmiyor");
    return;
  }

  const query = state.token ? `?token=${encodeURIComponent(state.token)}` : "";
  let source;
  try {
    source = new EventSource(`${ENDPOINTS.events}${query}`);
  } catch {
    scheduleReconnect();
    return;
  }
  eventSource = source;

  source.onopen = () => {
    retryDelay = 2000;
    setConnected(true);
    if (state.items.length === 0) loadDownloads();
  };

  source.onmessage = (event) => {
    retryDelay = 2000;
    setConnected(true);
    try {
      const parsed = JSON.parse(event.data);
      if (Array.isArray(parsed)) scheduleDownloads(parsed);
    } catch {
      /* bozuk olay: yoksay, sonraki olay tam listeyi getirir */
    }
  };

  source.onerror = () => {
    closeEvents();
    state.connected = false;
    if (document.hidden) {
      setConnection("pending", "Beklemede");
      return;
    }
    scheduleReconnect();
  };
}

function reconnectNow() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  if (!state.connected) {
    retryDelay = 2000;
    connectEvents();
  }
}

/* ---------------- İşlemler ---------------- */

function actionsPath(id, action) {
  return `./downloads/${encodeURIComponent(id)}/${action}`;
}

async function copyText(text) {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* yedek yola düş */
  }
  try {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    return ok;
  } catch {
    return false;
  }
}

function pathOf(download) {
  if (!download) return "";
  if (download.filePath) return download.filePath;
  if (download.dir && download.filename) {
    const sep = download.dir.includes("\\") ? "\\" : "/";
    return `${download.dir.replace(/[\\/]+$/, "")}${sep}${download.filename}`;
  }
  return download.filename || "";
}

async function handleAction(action, id, button) {
  if (button.disabled) return;
  const download = state.items.find((item) => item.id === id);
  const name = download ? download.filename || download.url : "İndirme";

  if (action === "cancel" && !window.confirm(`"${name}" iptal edilsin mi?\nParçalar silinir.`)) {
    return;
  }
  if (action === "remove") {
    const message = download
      ? `"${name}" listeden silinsin mi?\nİndirilen dosya diskte kalır.`
      : "Kayıt listeden silinsin mi?";
    if (!window.confirm(message)) return;
  }

  if (action === "copy") {
    const path = pathOf(download);
    if (!path) {
      showToast("Dosya yolu henüz yok.", "error");
      return;
    }
    const ok = await copyText(path);
    showToast(ok ? "Dosya yolu kopyalandı." : "Kopyalanamadı.", ok ? "ok" : "error");
    return;
  }

  button.disabled = true;
  try {
    if (action === "pause") {
      await api(actionsPath(id, "pause"), { method: "POST" });
      showToast("Duraklatıldı.");
    } else if (action === "resume") {
      await api(actionsPath(id, "resume"), { method: "POST" });
      showToast("Devam ediyor.");
    } else if (action === "cancel") {
      await api(actionsPath(id, "cancel"), { method: "POST" });
      showToast("İptal edildi.");
    } else if (action === "remove") {
      await api(`./downloads/${encodeURIComponent(id)}?deleteFile=false`, { method: "DELETE" });
      showToast("Kayıt silindi.");
    }
  } catch (error) {
    showToast(error.message || "İşlem başarısız.", "error");
  } finally {
    button.disabled = false;
  }

  if (action !== "copy") loadDownloads();
}

/* ---------------- Bildirim ---------------- */

let toastTimer = null;

function showToast(message, kind = "ok") {
  const node = els.toast;
  node.textContent = message;
  node.classList.toggle("is-error", kind === "error");
  node.hidden = false;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    node.hidden = true;
  }, 3200);
}

/* ---------------- İnceleme alt sayfası ---------------- */

function openSheet(title = "İnceleme") {
  state.lastFocused = document.activeElement;
  els.sheetTitle.textContent = title;
  els.sheet.hidden = false;
  document.body.style.overflow = "hidden";
  els.sheetPanel.focus({ preventScroll: true });
}

function closeSheet() {
  els.sheet.hidden = true;
  document.body.style.overflow = "";
  state.probe = null;
  els.sheetContent.hidden = true;
  els.sheetError.hidden = true;
  els.sheetLoading.hidden = false;
  if (state.lastFocused && typeof state.lastFocused.focus === "function") {
    state.lastFocused.focus({ preventScroll: true });
  }
}

function sheetLoading() {
  els.sheetLoading.hidden = false;
  els.sheetError.hidden = true;
  els.sheetContent.hidden = true;
  els.sheetAdd.disabled = false;
  els.sheetAdd.textContent = "İndir";
}

function sheetError(message) {
  els.sheetLoading.hidden = true;
  els.sheetContent.hidden = true;
  els.sheetError.hidden = false;
  els.sheetError.textContent = message;
}

function factRow(label, value) {
  const wrap = document.createElement("div");
  wrap.className = "fact";
  const dt = document.createElement("dt");
  dt.textContent = label;
  const dd = document.createElement("dd");
  dd.textContent = value;
  wrap.append(dt, dd);
  return wrap;
}

function formatOption({ id, label, size, note, name, checked, onPick }) {
  const wrap = document.createElement("label");
  wrap.className = "opt";

  const input = document.createElement("input");
  input.type = "radio";
  input.name = name;
  input.value = id;
  input.checked = Boolean(checked);
  input.addEventListener("change", () => {
    if (!input.checked) return;
    onPick(id);
    for (const peer of wrap.parentElement.querySelectorAll(".opt")) {
      peer.classList.toggle("is-selected", peer === wrap);
    }
  });

  const radio = document.createElement("span");
  radio.className = "opt-radio";
  radio.setAttribute("aria-hidden", "true");

  const body = document.createElement("span");
  body.className = "opt-body";
  const labelNode = document.createElement("span");
  labelNode.className = "opt-label";
  labelNode.textContent = label;
  body.appendChild(labelNode);
  if (note) {
    const noteNode = document.createElement("span");
    noteNode.className = "opt-note";
    noteNode.textContent = note;
    body.appendChild(noteNode);
  }

  wrap.append(input, radio, body);
  if (size) {
    const sizeNode = document.createElement("span");
    sizeNode.className = "opt-size";
    sizeNode.textContent = formatBytes(size);
    wrap.appendChild(sizeNode);
  }
  wrap.classList.toggle("is-selected", Boolean(checked));
  return wrap;
}

let probeToken = 0;

function renderProbe(data, originalUrl) {
  els.sheetLoading.hidden = true;
  els.sheetError.hidden = true;
  els.sheetContent.hidden = false;

  const title = data.title || data.filename || originalUrl;
  setText(els.probeTitle, title);
  setText(els.probeSub, data.finalUrl || data.url || originalUrl);

  els.probeFacts.replaceChildren();
  const sizeRow = factRow("Boyut", formatBytes(data.size));
  els.probeFacts.append(
    factRow("Tür", kindLabel(data.kind)),
    factRow("Dosya", data.filename || "—"),
    sizeRow,
    factRow("Format", data.mime || "—"),
    factRow("Devam edebilir", data.resumable ? "Evet" : "Hayır"),
  );
  els.probeSize = sizeRow.querySelector("dd");

  els.probeWarnings.replaceChildren();
  const warnings = Array.isArray(data.warnings) ? data.warnings.filter(Boolean) : [];
  for (const warning of warnings) {
    const li = document.createElement("li");
    li.textContent = String(warning);
    els.probeWarnings.appendChild(li);
  }
  els.probeWarnings.hidden = warnings.length === 0;

  const formats = Array.isArray(data.formats) ? data.formats : [];
  const candidates = Array.isArray(data.candidates) ? data.candidates : [];

  const probe = {
    data,
    originalUrl,
    format: null,
    candidate: null,
  };
  state.probe = probe;

  // Kalite listesi
  els.formatList.replaceChildren();
  els.formatGroup.hidden = formats.length === 0;
  if (formats.length > 0) {
    els.formatList.appendChild(
      formatOption({
        id: "",
        label: "En iyi (otomatik)",
        note: "Sunucu en uygun kaliteyi seçer",
        name: "hydra-format",
        checked: true,
        onPick: (value) => {
          probe.format = value || null;
          probe.candidate = null;
          els.candidateList.querySelectorAll("input").forEach((input) => {
            input.checked = false;
          });
          els.candidateList
            .querySelectorAll(".opt")
            .forEach((node) => node.classList.remove("is-selected"));
        },
      }),
    );

    formats.forEach((format, index) => {
      const piece = [format.ext || "", format.vcodec && format.vcodec !== "none" ? format.vcodec : ""]
        .filter(Boolean)
        .join(" · ");
      const noteParts = [];
      if (format.note) noteParts.push(format.note);
      if (format.acodec && format.acodec === "none") noteParts.push("ses yok");
      if (piece) noteParts.push(piece);
      els.formatList.appendChild(
        formatOption({
          id: format.id || String(index),
          label: format.label || `${format.height || "?"}p`,
          note: noteParts.join(" · "),
          size: format.filesize,
          name: "hydra-format",
          checked: false,
          onPick: (value) => {
            probe.format = value;
            probe.candidate = null;
            els.candidateList.querySelectorAll("input").forEach((input) => {
              input.checked = false;
            });
            els.candidateList
              .querySelectorAll(".opt")
              .forEach((node) => node.classList.remove("is-selected"));
          },
        }),
      );
    });
  }

  // Alternatif kaynaklar
  els.candidateList.replaceChildren();
  const showCandidates =
    candidates.length > 0 && (data.kind === "page" || data.kind === "unknown" || formats.length === 0);
  els.candidateGroup.hidden = !showCandidates;
  if (showCandidates) {
    candidates.forEach((candidate, index) => {
      const url = candidate.url || "";
      if (!url) return;
      let host = "";
      try {
        host = new URL(url, window.location.href).host;
      } catch {
        host = "";
      }
      els.candidateList.appendChild(
        formatOption({
          id: url || String(index),
          label: candidate.label || host || "Kaynak",
          note: [kindLabel(candidate.kind), host].filter(Boolean).join(" · "),
          size: candidate.size,
          name: "hydra-candidate",
          checked: false,
          onPick: (value) => {
            probe.candidate = value;
            probe.format = null;
            els.formatList.querySelectorAll("input").forEach((input) => {
              input.checked = false;
            });
            els.formatList
              .querySelectorAll(".opt")
              .forEach((node) => node.classList.remove("is-selected"));
          },
        }),
      );
    });
  }

  els.probeFilename.value = data.filename || "";
  els.sheetAdd.disabled = false;
  els.sheetAdd.textContent = "İndir";

  // Torrent: dosya listesi + seçim (arka planda getirilir).
  probe.files = null;
  const token = ++probeToken;
  if (data.kind === "torrent") {
    els.fileGroup.hidden = false;
    els.fileSummary.textContent = "Dosya listesi okunuyor… (magnet ise peer beklenir)";
    els.fileList.replaceChildren();
    void loadTorrentFiles(data.url || originalUrl, probe, token);
  } else {
    els.fileGroup.hidden = true;
    els.fileList.replaceChildren();
  }
}

/** Dosya listesini çizer; seçim değiştikçe özeti tazeler. */
function renderFiles(files) {
  els.fileList.replaceChildren();
  const draw = () => {
    const picked = files.filter((file) => file.included);
    const total = picked.reduce((sum, file) => sum + file.length, 0);
    els.fileSummary.textContent =
      picked.length === files.length
        ? `${files.length} dosya · ${formatBytes(total)} (tümü)`
        : `${picked.length}/${files.length} dosya · ${formatBytes(total)} seçili`;
    els.fileList.replaceChildren(
      ...files.map((file) => {
        const label = document.createElement("label");
        label.className = `file-opt${file.included ? " is-on" : ""}`;
        const box = document.createElement("input");
        box.type = "checkbox";
        box.checked = file.included;
        const name = document.createElement("span");
        name.className = "file-name";
        name.textContent = file.path;
        const size = document.createElement("span");
        size.className = "file-size";
        size.textContent = formatBytes(file.length);
        box.addEventListener("change", () => {
          file.included = box.checked;
          label.classList.toggle("is-on", box.checked);
          draw();
        });
        label.append(box, name, size);
        return label;
      }),
    );
  };
  draw();
}

/** Torrent dosya listesini sunucudan alır. */
async function loadTorrentFiles(source, probe, token) {
  try {
    const listing = await api(ENDPOINTS.torrentFiles, {
      method: "POST",
      body: { source },
      timeout: 70_000,
    });
    if (token !== probeToken || state.probe !== probe) return;
    const files = Array.isArray(listing && listing.files) ? listing.files : [];
    if (files.length === 0) {
      els.fileGroup.hidden = true;
      return;
    }
    probe.files = files;
    if (listing.name && !els.probeFilename.value.trim()) els.probeFilename.value = listing.name;
    if (listing.name) setText(els.probeTitle, listing.name);
    // Secilebilir toplam boyut (motorun tam boyutu yerine secim toplami).
    if (els.probeSize && listing.totalBytes) setText(els.probeSize, formatBytes(listing.totalBytes));
    renderFiles(files);
  } catch (error) {
    if (token !== probeToken) return;
    els.fileSummary.textContent = `Dosya listesi alınamadı: ${error.message || "bilinmeyen hata"}`;
  }
}

/** Seçili dosya indeksleri; hepsi seçiliyse `undefined`. */
function selectedFiles(files) {
  if (!Array.isArray(files)) return undefined;
  const picked = files.filter((file) => file.included).map((file) => file.index);
  if (picked.length === 0 || picked.length === files.length) return undefined;
  return picked;
}

async function startProbe(rawUrl, mediaHint) {
  const url = (rawUrl || "").trim();
  if (!url) {
    showToast("Önce bir bağlantı yapıştır.", "error");
    els.pasteUrl.focus();
    return;
  }
  if (!/^[a-zA-Z][a-zA-Z0-9+.-]*:\S+$/.test(url)) {
    showToast("Bağlantı geçersiz görünüyor.", "error");
    return;
  }

  state.probeUrl = url;
  openSheet("İnceleme");
  sheetLoading();
  setText(els.probeTitle, "");

  const body = { url };
  if (mediaHint === "media") body.media = true;
  if (mediaHint === "file") body.media = false;

  try {
    // Sunucu 429/503 dondururse uygulama bekleyip yeniden dener; 8 sn yetmez.
    const data = await api(ENDPOINTS.probe, { method: "POST", body, timeout: 45_000 });
    if (!data || typeof data !== "object") throw new Error("Sunucu boş yanıt verdi.");
    if (data.ok === false) throw new Error(data.error || "Bağlantı incelenemedi.");
    renderProbe(data, url);
  } catch (error) {
    sheetError(error.message || "Bağlantı incelenemedi.");
  }
}

async function addFromProbe() {
  const probe = state.probe;
  if (!probe) return;
  const data = probe.data;

  const button = els.sheetAdd;
  button.disabled = true;
  const previous = button.textContent;
  button.textContent = "Ekleniyor…";

  const body = {};
  if (probe.format) {
    body.url = data.url || probe.originalUrl;
    body.format = probe.format;
  } else if (probe.candidate) {
    body.url = probe.candidate;
  } else {
    body.url = data.finalUrl || data.url || probe.originalUrl;
  }

  const filename = els.probeFilename.value.trim();
  if (filename) body.filename = filename;

  const files = selectedFiles(probe.files);
  if (Array.isArray(probe.files) && probe.files.every((file) => !file.included)) {
    showToast("En az bir dosya seçmelisin.", "error");
    button.disabled = false;
    button.textContent = previous === "Ekleniyor…" ? "İndir" : previous;
    return;
  }
  if (files) body.files = files;

  if (data.kind === "media" || data.needsYtdlp || probe.format) body.media = true;
  else if (data.kind === "file") body.media = false;

  try {
    const result = await api(ENDPOINTS.add, { method: "POST", body });
    if (result && result.ok === false) throw new Error(result.error || "Eklenemedi.");
    const label = result && result.filename ? result.filename : body.url;
    showToast(files ? `Eklendi: ${label} (${files.length} dosya)` : `İndirmeye eklendi: ${label}`);
    closeSheet();
    showView("downloads");
    loadDownloads();
  } catch (error) {
    showToast(error.message || "İndirme eklenemedi.", "error");
  } finally {
    button.disabled = false;
    button.textContent = previous === "Ekleniyor…" ? "İndir" : previous;
  }
}

/* ---------------- Ayarlar ---------------- */

function updateHealthFacts() {
  const health = state.health;
  setText(els.setVersion, health && health.version ? String(health.version) : "—");
  setText(els.setUptime, health ? formatUptime(health.uptimeSecs) : "—");
  const origin = window.location.protocol === "file:" ? "" : window.location.origin;
  setText(els.setWebUrl, origin || "—");
}

function applySettings(settings) {
  const data = settings && typeof settings === "object" ? settings : {};
  setValue(els.setDir, data.downloadDir || "Sistemin İndirilenler klasörü");
  els.setMaxConcurrent.value = String(data.maxConcurrent ?? 3);
  els.setConnections.value = String(data.connections ?? 8);
  els.setSpeed.value = String(data.speedLimitKbps ?? 0);
  els.setCapture.checked = Boolean(data.captureEnabled);
  els.setAutoResume.checked = Boolean(data.autoResume);
  if (els.setNotify) els.setNotify.checked = data.notifyOnFinish !== false;
  setText(els.setCapturePort, data.capturePort ? String(data.capturePort) : "—");
  els.setToken.value = state.token;
}

async function loadSettings(force = false) {
  if (state.settingsLoaded && !force) {
    applySettings(state.rawSettings);
    // yt-dlp başka bir cihazdan kurulmuş olabilir; her açılışta tazelenir.
    await refreshYtdlp();
    return;
  }
  els.settingsLoading.hidden = false;
  els.settingsForm.hidden = true;
  els.settingsError.hidden = true;

  try {
    const settings = await api(ENDPOINTS.settings);
    state.rawSettings = settings && typeof settings === "object" ? settings : {};
    state.settingsLoaded = true;
    applySettings(state.rawSettings);
    updateHealthFacts();
    els.settingsForm.hidden = false;
    els.settingsLoading.hidden = true;
    if (!state.health) loadHealth();
    await refreshYtdlp();
  } catch (error) {
    els.settingsLoading.hidden = true;
    els.settingsError.hidden = false;
    els.settingsError.textContent = `Ayarlar yüklenemedi: ${error.message}`;
  }
}

function setSettingsStatus(message, kind = "") {
  const node = els.settingsStatus;
  node.textContent = message;
  node.classList.toggle("is-error", kind === "error");
  node.classList.toggle("is-ok", kind === "ok");
}

/* ---------------- yt-dlp durumu ve kurulum ---------------- */

function setYtdlpStatus(message, kind = "") {
  const node = els.ytdlpStatus;
  if (!node) return;
  node.textContent = message;
  node.classList.toggle("is-error", kind === "error");
  node.classList.toggle("is-ok", kind === "ok");
}

function applyYtdlpStatus(status) {
  const available = Boolean(status && status.available);
  setText(
    els.setYtdlp,
    available
      ? `${status.version ? `v${status.version}` : "bulundu"} · ${status.path || ""}`.trim()
      : "bulunamadı",
  );
  if (els.installYtdlp) {
    els.installYtdlp.hidden = available;
    els.installYtdlp.disabled = false;
  }
  if (els.ytdlpHelp) els.ytdlpHelp.hidden = available;
}

async function refreshYtdlp() {
  // Sunucu `yt-dlp --version` çalıştırdığı için ilk yanıt ~1 sn gecikebilir;
  // o arada düğme tıklanıp gereksiz yeniden kurulum başlatılmasın.
  if (els.setYtdlp) setText(els.setYtdlp, "kontrol ediliyor…");
  if (els.installYtdlp) els.installYtdlp.disabled = true;
  try {
    const status = await api(ENDPOINTS.ytdlp);
    applyYtdlpStatus(status);
    setYtdlpStatus("");
  } catch (error) {
    setText(els.setYtdlp, "bilinmiyor");
    setYtdlpStatus(error.message || "yt-dlp durumu okunamadı.", "error");
    if (els.installYtdlp) els.installYtdlp.disabled = false;
  }
}

async function installYtdlp() {
  const button = els.installYtdlp;
  if (!button || button.disabled) return;
  button.disabled = true;
  const previous = button.textContent;
  button.textContent = "Kuruluyor…";
  setYtdlpStatus("İndiriliyor, bu 10-20 saniye sürebilir. Sayfayı kapatma.");
  try {
    // Sunucu dosyayı indirip doğruluyor; varsayılan 8 sn yetmez.
    const result = await api(ENDPOINTS.ytdlpInstall, { method: "POST", timeout: 120_000 });
    // Önce durum/ayarlar tazelenir (bu sırada mesaj temizlenir), sonra sonuç yazılır.
    await refreshYtdlp();
    state.settingsLoaded = false;
    await loadSettings(true);
    setYtdlpStatus(
      result && result.path ? `Kuruldu: ${result.path}` : "yt-dlp kuruldu.",
      "ok",
    );
    showToast("yt-dlp kuruldu.");
  } catch (error) {
    setYtdlpStatus(error.message || "yt-dlp kurulamadı.", "error");
    button.textContent = previous;
    button.disabled = false;
  }
}

function clampInt(value, min, max, fallback) {
  const parsed = parseNumberish(value);
  if (!Number.isFinite(parsed)) return fallback;
  return Math.max(min, Math.min(max, Math.round(parsed)));
}

async function saveSettings() {
  if (!state.rawSettings) return;
  const next = { ...state.rawSettings };
  next.maxConcurrent = clampInt(els.setMaxConcurrent.value, 1, 10, 3);
  next.connections = clampInt(els.setConnections.value, 1, 16, 8);
  next.speedLimitKbps = clampInt(els.setSpeed.value, 0, 10_000_000, 0);
  next.captureEnabled = els.setCapture.checked;
  next.autoResume = els.setAutoResume.checked;
  if (els.setNotify) next.notifyOnFinish = els.setNotify.checked;
  if (Object.prototype.hasOwnProperty.call(next, "webToken")) {
    next.webToken = els.setToken.value.trim();
  }

  els.settingsSave.disabled = true;
  setSettingsStatus("Kaydediliyor…");
  try {
    const saved = await api(ENDPOINTS.settings, { method: "PUT", body: next });
    state.rawSettings = saved && typeof saved === "object" ? saved : next;
    storeToken(els.setToken.value.trim());
    applySettings(state.rawSettings);
    setSettingsStatus("Ayarlar kaydedildi.", "ok");
    showToast("Ayarlar kaydedildi.");
    // Token değiştiyse olay akışını yeni anahtarla bağla.
    if (!state.connected) reconnectNow();
  } catch (error) {
    setSettingsStatus(error.message || "Ayarlar kaydedilemedi.", "error");
  } finally {
    els.settingsSave.disabled = false;
  }
}

/* ---------------- Olay bağlama ---------------- */

function bindQuickForm() {
  els.quickForm.addEventListener("submit", (event) => {
    event.preventDefault();
    startProbe(els.pasteUrl.value);
  });
}

function bindFilters() {
  els.filters.addEventListener("click", (event) => {
    const chip = event.target.closest(".chip");
    if (!chip) return;
    state.filter = chip.dataset.filter || "all";
    for (const peer of els.filters.querySelectorAll(".chip")) {
      const active = peer === chip;
      peer.classList.toggle("is-active", active);
      peer.setAttribute("aria-selected", active ? "true" : "false");
    }
    applyFilter();
  });
}

function bindList() {
  els.list.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-action]");
    if (!button) return;
    const card = button.closest(".card");
    if (!card || !card.dataset.id) return;
    handleAction(button.dataset.action, card.dataset.id, button);
  });
}

function bindNewForm() {
  els.newForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const url = els.newUrl.value.trim();
    if (!url) return;
    const mediaValue = els.newMedia.value;
    const body = { url };
    const filename = els.newFilename.value.trim();
    if (filename) body.filename = filename;
    if (mediaValue === "media") body.media = true;
    if (mediaValue === "file") body.media = false;

    els.newAdd.disabled = true;
    els.newStatus.classList.remove("is-error", "is-ok");
    els.newStatus.textContent = "Ekleniyor…";
    try {
      const result = await api(ENDPOINTS.add, { method: "POST", body });
      if (result && result.ok === false) throw new Error(result.error || "Eklenemedi.");
      els.newStatus.classList.add("is-ok");
      els.newStatus.textContent = `Eklendi: ${(result && result.filename) || url}`;
      els.newUrl.value = "";
      els.newFilename.value = "";
      showToast("İndirmeye eklendi.");
      showView("downloads");
      loadDownloads();
    } catch (error) {
      els.newStatus.classList.add("is-error");
      els.newStatus.textContent = error.message || "İndirme eklenemedi.";
    } finally {
      els.newAdd.disabled = false;
    }
  });

  els.newProbe.addEventListener("click", () => {
    const url = els.newUrl.value.trim();
    if (!url) {
      els.newUrl.focus();
      showToast("Önce bir bağlantı gir.", "error");
      return;
    }
    startProbe(url, els.newMedia.value);
  });
}

function bindSettings() {
  els.settingsForm.addEventListener("submit", (event) => {
    event.preventDefault();
    saveSettings();
  });
  els.settingsReload.addEventListener("click", () => {
    state.settingsLoaded = false;
    loadSettings(true);
    loadHealth();
    setSettingsStatus("");
  });
  if (els.installYtdlp) {
    els.installYtdlp.addEventListener("click", installYtdlp);
  }
}

function bindSheet() {
  els.sheetClose.addEventListener("click", closeSheet);
  els.sheet.addEventListener("click", (event) => {
    if (event.target === els.sheet) closeSheet();
  });
  els.sheetAdd.addEventListener("click", addFromProbe);

  // Torrent dosya listesinde toplu seçim.
  const setAll = (value) => {
    const files = state.probe && state.probe.files;
    if (!Array.isArray(files)) return;
    for (const file of files) file.included = value;
    renderFiles(files);
  };
  if (els.filesAll) els.filesAll.addEventListener("click", () => setAll(true));
  if (els.filesNone) els.filesNone.addEventListener("click", () => setAll(false));

  document.addEventListener("keydown", (event) => {
    if (els.sheet.hidden) return;
    if (event.key === "Escape") {
      event.preventDefault();
      closeSheet();
      return;
    }
    if (event.key !== "Tab") return;
    const focusables = Array.from(
      els.sheetPanel.querySelectorAll(
        'button:not([disabled]):not([hidden]), input:not([disabled]):not([type="hidden"]), [href], select, textarea',
      ),
    ).filter((node) => node.offsetParent !== null);
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  });
}

function bindNav() {
  for (const tab of els.tabs) {
    tab.addEventListener("click", () => showView(tab.dataset.view || "downloads"));
  }
  els.settingsBtn.addEventListener("click", () => showView("settings"));
  els.retryBtn.addEventListener("click", async () => {
    els.error.hidden = true;
    els.skeleton.hidden = false;
    setConnection("pending", "Bağlanıyor…");
    await loadHealth();
    const ok = await loadDownloads();
    if (!ok) {
      els.skeleton.hidden = true;
      showLoadError();
    }
    reconnectNow();
  });
}

function bindLifecycle() {
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) return;
    if (!state.connected) reconnectNow();
    loadHealth();
  });

  window.addEventListener("online", () => {
    reconnectNow();
    loadDownloads();
  });

  window.addEventListener("offline", () => {
    closeEvents();
    state.connected = false;
    setConnection("down", "Çevrimdışı");
  });
}

/* ---------------- PWA ---------------- */

function registerServiceWorker() {
  if (!("serviceWorker" in navigator)) return;
  if (window.location.protocol === "file:") return; // SW file:// altında çalışmaz
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("./sw.js").catch(() => {
      /* sunucu yoksa / kapsam dışıysa sessizce yoksay */
    });
  });
}

/* ---------------- Başlat ---------------- */

async function boot() {
  state.token = readToken();
  bindQuickForm();
  bindFilters();
  bindList();
  bindNewForm();
  bindSettings();
  bindSheet();
  bindNav();
  bindLifecycle();

  setConnection("pending", "Bağlanıyor…");
  const ok = await loadDownloads();
  if (!ok) {
    els.skeleton.hidden = true;
    showLoadError();
  }
  loadHealth();
  connectEvents();
  registerServiceWorker();
}

boot().catch(() => {
  showLoadError("Arayüz başlatılamadı.");
});
