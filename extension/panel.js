// Hydra DL — indirme seçenekleri paneli (IDM tarzı).
"use strict";

const H = window.HydraShared;

const KIND_LABELS = {
  file: "dosya",
  direct: "dosya",
  video: "video",
  audio: "ses",
  image: "resim",
  media: "medya",
  torrent: "torrent",
  hls: "HLS akışı",
  dash: "DASH akışı",
  page: "sayfa",
  unknown: "bilinmiyor",
};

// Önizlemede oynatılabilir aday türleri (sunucu sürümüne göre değişebilir).
const PLAYABLE_KINDS = new Set(["file", "video", "audio", "media", "direct"]);

const params = new URLSearchParams(location.search);
const state = {
  url: params.get("url") || "",
  referer: params.get("referer") || "",
  pageTitle: params.get("title") || "",
  probe: null,
  fallback: false,
  blob: false,
  settings: null,
  selectedFormat: "",
  busy: false,
  filenameDirty: false,
  added: false,
};

const el = {
  badge: document.getElementById("kind-badge"),
  preview: document.getElementById("preview"),
  title: document.getElementById("title"),
  source: document.getElementById("source"),
  sizeChip: document.getElementById("size-chip"),
  kindChip: document.getElementById("kind-chip"),
  resumeChip: document.getElementById("resume-chip"),
  candidatesBox: document.getElementById("candidates-box"),
  candidateCount: document.getElementById("candidate-count"),
  candidates: document.getElementById("candidates"),
  ytdlpNote: document.getElementById("ytdlp-note"),
  fallbackNote: document.getElementById("fallback-note"),
  warnings: document.getElementById("warnings"),
  formats: document.getElementById("formats"),
  filename: document.getElementById("filename"),
  dirRow: document.getElementById("dir-row"),
  dir: document.getElementById("dir"),
  download: document.getElementById("download"),
  cancel: document.getElementById("cancel"),
  copy: document.getElementById("copy"),
  message: document.getElementById("message"),
  version: document.getElementById("version"),
  openOptions: document.getElementById("open-options"),
  closePanel: document.getElementById("close-panel"),
  loading: document.getElementById("loading"),
  errorScreen: document.getElementById("error-screen"),
  errorTitle: document.getElementById("error-title"),
  errorDetail: document.getElementById("error-detail"),
  retry: document.getElementById("retry"),
  errorOptions: document.getElementById("error-options"),
};

function isOffline(err) {
  return Boolean(err) && (err.code === "network" || err.code === "timeout");
}

/**
 * Önizleme için güvenli medya adresi.
 *
 * `probe` verisi sayfadan gelir (güvenilmez): şema doğrulanmadan `video.src`'ye
 * verilirse CodeQL haklı olarak XSS/redirect işaretler. Yalnız http(s), blob ve
 * `data:video/…` kabul edilir; geri kalanı boş döner (önizleme çizilmez).
 */
function safeMediaUrl(raw) {
  const value = String(raw || "").trim();
  if (!value) return "";
  let parsed;
  try {
    parsed = new URL(value, location.href);
  } catch {
    return "";
  }
  const scheme = parsed.protocol.toLowerCase();
  if (scheme === "http:" || scheme === "https:" || scheme === "blob:") {
    return parsed.href;
  }
  if (scheme === "data:" && /^data:video\//i.test(value)) {
    return value;
  }
  return "";
}

function looksVideo(probe) {
  const mime = String((probe && probe.mime) || "").toLowerCase();
  if (mime.startsWith("video/")) return true;
  const url = String((probe && probe.finalUrl) || state.url || "");
  return /\.(mp4|webm|mkv|mov|m4v|ogv)(\?|#|$)/i.test(url);
}

function setMessage(text, tone) {
  el.message.textContent = text || "";
  el.message.className = "message" + (tone ? ` ${tone}` : "");
}

function setBusy(busy) {
  // Eklendikten sonra düğme "İndir"e dönmemeli: ikinci tıklama aynı indirmeyi
  // tekrar kuyruğa sokuyordu.
  if (state.added) {
    el.download.disabled = true;
    el.download.textContent = "Eklendi";
    return;
  }
  el.download.disabled = busy;
  el.download.textContent = busy ? "Ekleniyor…" : "İndir";
}

function showLoading(show) {
  el.loading.hidden = !show;
}

function showError(err) {
  showLoading(false);
  el.errorTitle.textContent = isOffline(err)
    ? "Hydra DL'ye ulaşılamadı"
    : "Bağlantı incelenemedi";
  el.errorDetail.textContent = err && err.message ? err.message : "Bilinmeyen hata.";
  el.errorScreen.hidden = false;
}

function hideError() {
  el.errorScreen.hidden = true;
}

async function closeTab() {
  try {
    const tab = await HX.api.tabs.getCurrent();
    if (tab && tab.id !== undefined && tab.id !== null) {
      await HX.api.tabs.remove(tab.id);
    } else {
      window.close();
    }
  } catch (_err) {
    window.close();
  }
}

function renderPreview(probe) {
  el.preview.textContent = "";
  const candidates = Array.isArray(probe && probe.candidates) ? probe.candidates : [];
  const fileCandidate = candidates.find(
    (c) => c && c.url && PLAYABLE_KINDS.has(String(c.kind || "file").toLowerCase()),
  );
  const safeVideoSource = looksVideo(probe)
    ? safeMediaUrl(fileCandidate?.url || (probe && probe.finalUrl) || state.url || "")
    : "";

  if (safeVideoSource) {
    const video = document.createElement("video");
    video.controls = true;
    video.preload = "metadata";
    video.src = safeVideoSource;
    if (state.referer) video.setAttribute("data-referer", state.referer);

    const hint = document.createElement("div");
    hint.className = "kind-hint";
    hint.textContent = "oynatma önizlemesi";

    video.addEventListener("error", () => {
      hint.textContent = "önizleme yüklenemedi · indirme yine de çalışır";
    });

    el.preview.append(video, hint);
    return;
  }

  const mime = String((probe && probe.mime) || "").toLowerCase();
  const name = String((probe && probe.filename) || state.url);
  let glyph = "📄";
  if (mime.startsWith("audio/") || /\.(mp3|m4a|opus|flac|wav|ogg)(\?|#|$)/i.test(name)) {
    glyph = "🎵";
  } else if (mime.startsWith("image/") || /\.(png|jpe?g|gif|webp|avif|svg|bmp)(\?|#|$)/i.test(name)) {
    glyph = "🖼";
  } else if (mime.startsWith("video/") || /\.(mp4|webm|mkv|mov)(\?|#|$)/i.test(name)) {
    glyph = "🎬";
  } else if (/\.(zip|rar|7z|iso|tar|gz)(\?|#|$)/i.test(name)) {
    glyph = "🗜";
  } else if (probe && (probe.kind === "page" || probe.needsYtdlp)) {
    glyph = "🌐";
  }

  const icon = document.createElement("div");
  icon.className = "icon";
  icon.textContent = glyph;
  el.preview.appendChild(icon);

  const hint = document.createElement("div");
  hint.className = "kind-hint";
  hint.textContent = probe ? `önizleme yok · ${KIND_LABELS[probe.kind] || "dosya"}` : "önizleme yok";
  el.preview.appendChild(hint);
}

function formatName(format) {
  const base = format.label || (format.height ? `${format.height}p` : `format ${format.id || "?"}`);
  const parts = [base];
  const lower = base.toLowerCase();
  if (format.ext && !lower.includes(format.ext.toLowerCase())) parts.push(format.ext);
  if (format.filesize && !/(\d\s*(b|kb|mb|gb|tb))\b/i.test(base)) {
    parts.push(H.formatBytes(format.filesize));
  }
  return parts.join(" · ");
}

function formatNote(format) {
  const bits = [];
  if (format.vcodec && format.vcodec !== "none") bits.push(format.vcodec);
  if (format.acodec && format.acodec !== "none") bits.push(`ses: ${format.acodec}`);
  else if (format.acodec === "none") bits.push("sessiz");
  if (format.note) bits.push(format.note);
  return bits.join(" · ");
}

function renderFormats(probe) {
  el.formats.textContent = "";
  const formats = Array.isArray(probe && probe.formats) ? probe.formats.filter(Boolean) : [];
  // Kalite listesi yalnız yt-dlp ile gelir; düz dosyada tek satır gösterilir.
  const qualityList = probe && probe.needsYtdlp === true ? formats : [];

  if (!qualityList.length) {
    state.selectedFormat = "";
    const first = formats[0];
    const noteText =
      formats.length && first.label ? `${first.label} · kalite seçimi yok` : "kalite seçimi yok";
    el.formats.appendChild(
      buildFormatRow({
        id: "",
        name: "Orijinal dosya",
        note: noteText,
        size: (first && first.filesize) || (probe && probe.size),
      }),
    );
    return;
  }

  qualityList.forEach((format, index) => {
    const row = buildFormatRow({
      id: String(format.id ?? ""),
      name: formatName(format),
      note: formatNote(format),
      size: format.filesize,
      checked: index === 0,
    });
    el.formats.appendChild(row);
  });
  state.selectedFormat = String(qualityList[0].id ?? "");
}

function buildFormatRow(options) {
  const label = document.createElement("label");
  label.className = "format";

  const radio = document.createElement("input");
  radio.type = "radio";
  radio.name = "hydra-format";
  radio.value = options.id;
  radio.checked = options.checked === true;

  const main = document.createElement("span");
  main.className = "main";
  const name = document.createElement("span");
  name.className = "name";
  name.textContent = options.name;
  main.appendChild(name);
  if (options.note) {
    const note = document.createElement("span");
    note.className = "note";
    note.textContent = options.note;
    main.appendChild(note);
  }

  const size = document.createElement("span");
  size.className = "size";
  size.textContent = H.formatBytes(options.size);

  label.append(radio, main, size);
  return label;
}

function renderCandidates(probe) {
  const candidates = Array.isArray(probe && probe.candidates) ? probe.candidates.filter(Boolean) : [];
  if (!candidates.length) {
    el.candidatesBox.hidden = true;
    return;
  }
  el.candidates.textContent = "";
  candidates.forEach((candidate, index) => {
    const row = document.createElement("div");
    row.className = "candidate";

    const info = document.createElement("div");
    info.className = "info-text";
    const label = document.createElement("span");
    label.className = "label";
    label.textContent = candidate.label || `Kaynak ${index + 1}`;
    const url = document.createElement("span");
    url.className = "url";
    url.textContent = candidate.url || "";
    info.append(label, url);

    const size = document.createElement("span");
    size.className = "size";
    size.textContent = H.formatBytes(candidate.size);

    const use = document.createElement("button");
    use.type = "button";
    use.className = "mini";
    use.textContent = "Bunu indir";
    use.dataset.index = String(index);

    row.append(info, size, use);
    el.candidates.appendChild(row);
  });
  el.candidateCount.textContent = String(candidates.length);
  el.candidatesBox.hidden = false;
}

function renderMeta(probe) {
  const titleText =
    (probe && probe.title) || state.pageTitle || (probe && probe.filename) || H.guessFilename(state.url);
  el.title.textContent = titleText;

  const finalUrl = (probe && probe.finalUrl) || state.url;
  const host = H.siteHost(finalUrl);
  const redirected = probe && probe.finalUrl && probe.finalUrl !== state.url;
  el.source.textContent = redirected ? `${host} · ${finalUrl}` : `${host} · ${state.url}`;

  const kind = probe ? probe.kind : "unknown";
  el.badge.textContent = KIND_LABELS[kind] || kind;
  el.kindChip.textContent = `tür: ${KIND_LABELS[kind] || kind}`;
  el.resumeChip.hidden = !(probe && probe.resumable);

  if (!state.filenameDirty) {
    el.filename.value = (probe && probe.filename) || H.guessFilename(finalUrl);
  }
}

function formatDuration(seconds) {
  const total = Number(seconds);
  if (!Number.isFinite(total) || total <= 0) return "";
  const secs = Math.round(total);
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  const two = (n) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}

function refreshSizeChip() {
  const probe = state.probe;
  const formats = Array.isArray(probe && probe.formats) ? probe.formats : [];
  const chosen = formats.find((f) => String(f.id ?? "") === state.selectedFormat);
  const size = (chosen && chosen.filesize) || (probe && probe.size);
  const duration = probe ? formatDuration(probe.durationSecs) : "";
  const parts = [`boyut: ${H.formatBytes(size)}`];
  if (duration) parts.push(`süre: ${duration}`);
  el.sizeChip.textContent = parts.join(" · ");
}

function render(probe) {
  renderPreview(probe);
  renderMeta(probe);
  renderFormats(probe);
  renderCandidates(probe);
  refreshSizeChip();

  el.ytdlpNote.hidden = !(probe && probe.kind === "page" && probe.needsYtdlp === true);

  if (state.fallback) {
    el.fallbackNote.hidden = false;
    if (!state.blob) {
      el.fallbackNote.textContent =
        "Bu Hydra DL sürümü ayrıntılı bilgi döndürmedi (/probe yok). Dosya yine de kuyruğa eklenebilir; kalite listesi boş görünüyor.";
    }
  } else {
    el.fallbackNote.hidden = true;
  }

  const warnings = Array.isArray(probe && probe.warnings) ? probe.warnings.filter(Boolean) : [];
  if (warnings.length) {
    el.warnings.hidden = false;
    el.warnings.textContent = warnings.join(" · ");
  } else {
    el.warnings.hidden = true;
  }
}

async function loadSettings() {
  try {
    const data = await H.hydraSettings();
    const settings = (data && data.settings) || data;
    if (settings && typeof settings.downloadDir === "string" && settings.downloadDir.trim()) {
      state.settings = settings;
      el.dir.textContent = settings.downloadDir;
      el.dirRow.hidden = false;
      return;
    }
    el.dirRow.hidden = true;
  } catch (_err) {
    // /settings yoksa klasör satırını gizle
    el.dirRow.hidden = true;
  }
}

async function load(targetUrl) {
  hideError();
  setMessage("");
  state.url = targetUrl;

  if (!/^(https?|ftp|magnet|blob):/i.test(targetUrl)) {
    showError({ message: "Adres yok ya da indirilebilir bir bağlantı değil.", code: "bad-url" });
    return;
  }

  showLoading(true);
  state.filenameDirty = false;
  state.added = false;
  el.cancel.textContent = "İptal";
  setBusy(false);
  state.probe = null;
  state.fallback = false;
  state.blob = /^blob:/i.test(targetUrl);
  render(null);

  // blob: adresleri sayfa bağlamına aittir; sunucu çözmeyi deneyecek.
  if (state.blob) {
    showLoading(false);
    state.fallback = true;
    el.fallbackNote.hidden = false;
    el.fallbackNote.textContent =
      "Bu medya blob: adresiyle oynatılıyor; Hydra bağlantıyı referer ile çözmeyi deneyecek. İndirme başarısız olursa sayfadaki gerçek dosya bağlantısını avcı moduyla seç.";
    return;
  }

  try {
    const probe = await H.hydraProbe(targetUrl);
    state.probe = probe;
    render(probe);
  } catch (err) {
    if (err && (err.code === "unsupported" || err.status === 422)) {
      // Sunucu sayfayı inceleyemedi: yine de doğrudan kuyruğa eklenebilir.
      state.fallback = true;
      state.probe = null;
      render(null);
    } else {
      showError(err);
      return;
    }
  } finally {
    showLoading(false);
  }

  if (state.probe && state.probe.finalUrl && state.probe.finalUrl !== targetUrl) {
    const formats = Array.isArray(state.probe.formats) ? state.probe.formats : [];
    if (formats.length && formats[0].height) {
      setMessage(`En iyi kalite seçildi (${formats[0].height}p).`, "");
    }
  }
}

async function submit() {
  if (state.busy) return;
  const probe = state.probe;
  const target = (probe && probe.finalUrl) || state.url;
  if (!target) {
    setMessage("İndirilecek adres bulunamadı.", "err");
    return;
  }

  const payload = {
    url: target,
    filename: el.filename.value.trim() || undefined,
    referer: state.referer || undefined,
  };

  // format yalnız gerçek bir yt-dlp kimliği varsa gönderilir; düz dosyada alan hiç konmaz.
  if (probe && probe.needsYtdlp === true && state.selectedFormat) {
    payload.format = state.selectedFormat;
  }
  // media yalnız sunucu içeriği medya olarak sınıflandırdıysa gönderilir.
  if (probe && probe.kind === "media") {
    payload.media = true;
  }

  if (state.settings && state.settings.downloadDir) payload.dir = state.settings.downloadDir;

  state.busy = true;
  setBusy(true);
  setMessage("");
  try {
    const data = await H.hydraAdd(payload);
    state.added = true;
    setMessage(`Kuyruğa eklendi: ${data.filename || H.guessFilename(target)}`, "ok");
    // Panel açık kalır. Eskiden 1.1 sn sonra sekme kendini kapatıyordu; kullanıcı
    // "indire basınca sayfa gidiyor" diye görüyordu ve sonucu okuyamıyordu.
    el.cancel.textContent = "Kapat";
    el.download.textContent = "Eklendi";
    el.download.disabled = true;
  } catch (err) {
    if (isOffline(err)) {
      showError(err);
    } else {
      setMessage(`Eklenemedi: ${err.message}`, "err");
    }
  } finally {
    state.busy = false;
    setBusy(false);
  }
}

el.formats.addEventListener("change", (event) => {
  const input = event.target;
  if (!(input instanceof HTMLInputElement) || input.name !== "hydra-format") return;
  state.selectedFormat = input.value;
  refreshSizeChip();
});

el.candidates.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-index]");
  if (!button || !state.probe) return;
  const index = Number(button.dataset.index);
  const candidates = Array.isArray(state.probe.candidates) ? state.probe.candidates : [];
  const candidate = candidates[index];
  if (!candidate || !candidate.url) return;
  void load(candidate.url);
});

el.download.addEventListener("click", () => void submit());

el.cancel.addEventListener("click", () => closeTab());

el.copy.addEventListener("click", async () => {
  const target = (state.probe && state.probe.finalUrl) || state.url;
  try {
    await navigator.clipboard.writeText(target);
    setMessage("Bağlantı panoya kopyalandı.", "ok");
  } catch (_err) {
    setMessage("Kopyalanamadı; adresi elle kopyalayabilirsin.", "err");
  }
});

el.retry.addEventListener("click", () => void load(state.url));

el.openOptions.addEventListener("click", () => HX.api.runtime.openOptionsPage());
el.errorOptions.addEventListener("click", () => HX.api.runtime.openOptionsPage());

el.closePanel.addEventListener("click", async () => {
  await H.saveSettings({ showPanel: false });
  closeTab();
});

el.filename.addEventListener("input", () => {
  state.filenameDirty = true;
});

el.filename.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    void submit();
  }
});

function init() {
  el.version.textContent = `Hydra DL eklentisi v${HX.api.runtime.getManifest().version}`;
  void loadSettings();
  void load(state.url);
}

init();
