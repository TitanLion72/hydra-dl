import "./styles.css";
import "./sheet.css";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  onDownloads,
  onNotice,
  onYtdlpInstall,
  type AppInfo,
  type Download,
  type InstallProgress,
  type Notice,
  type ProbeResult,
  type Settings,
  type Status,
  type TorrentFile,
  type TorrentListing,
} from "./api";

type Filter = "all" | "active" | "queued" | "done" | "failed";
type Sort = "new" | "old" | "name" | "size" | "status";

const SORT_KEY = "hydra.sort";
const SORT_OPTIONS: Sort[] = ["new", "old", "name", "size", "status"];

const $ = <T extends Element = HTMLElement>(selector: string): T => {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`Element yok: ${selector}`);
  return found;
};

const list = $<HTMLUListElement>("#list");
const empty = $("#empty");
const urlInput = $<HTMLInputElement>("#url-input");
const kindSelect = $<HTMLSelectElement>("#kind-select");
const addForm = $<HTMLFormElement>("#add-form");
const addBtn = $<HTMLButtonElement>("#add-btn");
const addHint = $("#add-hint");
const throughput = $("#throughput");
const filters = $("#filters");
const searchInput = $<HTMLInputElement>("#search-input");
const sortSelect = $<HTMLSelectElement>("#sort-select");
const capturePill = $<HTMLButtonElement>("#capture-pill");
const captureDot = $("#capture-dot");
const captureLabel = $("#capture-label");
const captureFooter = $("#capture-footer");
const footerInfo = $("#footer-info");
const overlay = $("#overlay");
const drawer = $<HTMLElement>("#settings-drawer");
const toasts = $("#toasts");

let downloads: Download[] = [];
let filter: Filter = "all";
let query = "";
let sort: Sort = readSort();
let settings: Settings | null = null;
let info: AppInfo | null = null;
let adding = false;

interface RowRefs {
  root: HTMLLIElement;
  icon: HTMLDivElement;
  name: HTMLElement;
  badge: HTMLElement;
  host: HTMLElement;
  amount: HTMLElement;
  eta: HTMLElement;
  segInfo: HTMLElement;
  fill: HTMLElement;
  speed: HTMLElement;
  error: HTMLElement;
  actions: HTMLElement;
  actionsKey: string;
}

const ICONS: Record<string, string> = {
  video: '<path d="m10 8 6 4-6 4V8Z"/><rect x="3" y="4" width="18" height="16" rx="3"/>',
  audio:
    '<path d="M9 18V6l10-2v12"/><circle cx="6" cy="18" r="3"/><circle cx="16" cy="16" r="3"/>',
  image:
    '<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="9" r="1.5"/><path d="m4 17 5-5 4 4 3-3 4 4"/>',
  archive:
    '<path d="M4 7h16v13H4z"/><path d="M2 4h20v3H2zM10 11h4v3h-4z"/>',
  app: '<path d="M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z"/>',
  disk: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="2.5"/>',
  pdf: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/><path d="M9 13h6M9 17h6"/>',
  text: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4M9 12h6M9 16h6"/>',
  file: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/>',
  media:
    '<path d="M4 6h16v12H4z"/><path d="M10 9.5v5l4-2.5-4-2.5Z"/>',
  torrent:
    '<circle cx="12" cy="12" r="9"/><path d="M12 7v6m0 0-2.5-2.5M12 13l2.5-2.5"/><path d="M7.5 16.5a6 6 0 0 0 9 0"/>',
  hls: '<path d="M4 6h16v12H4z"/><path d="M8 10v4M12 10v4M16 10v4"/>',
};

const STATUS_TEXT: Record<Status, string> = {
  queued: "Bekliyor",
  downloading: "İndiriliyor",
  paused: "Duraklatıldı",
  completed: "Tamamlandı",
  failed: "Hata",
  cancelled: "İptal",
};

interface Action {
  action: string;
  title: string;
  icon: string;
  tone?: "danger" | "accent";
}

const ACTION_ICONS = {
  pause: '<path d="M9 6v12M15 6v12"/>',
  play: '<path d="m8 5 11 7-11 7V5Z"/>',
  cancel: '<path d="M6 6l12 12M18 6 6 18"/>',
  retry: '<path d="M4 12a8 8 0 1 0 3-6.2"/><path d="M4 4v5h5"/>',
  open: '<path d="M14 4h6v6"/><path d="M20 4 10 14"/><path d="M19 14v5H5V5h5"/>',
  reveal: '<path d="M3 7h6l2 2h10v10H3z"/>',
  remove: '<path d="M5 5l14 14M19 5 5 19"/>',
  trash: '<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/>',
  files: '<path d="M8 6h13M8 12h13M8 18h13"/><path d="M3.5 6h.01M3.5 12h.01M3.5 18h.01"/>',
};

/** Torrent'te secilebilir dosya listesi var mi? */
function hasFileSelection(item: Download): boolean {
  return item.kind === "torrent" && (item.torrentFiles?.length ?? 0) > 1;
}

function actionsFor(item: Download): Action[] {
  let actions: Action[];
  switch (item.status) {
    case "downloading":
    case "queued":
      actions = [
        { action: "pause", title: "Duraklat", icon: ACTION_ICONS.pause },
        { action: "cancel", title: "İptal et", icon: ACTION_ICONS.cancel, tone: "danger" },
        { action: "remove", title: "Listeden çıkar", icon: ACTION_ICONS.remove },
      ];
      break;
    case "paused":
    case "cancelled":
      actions = [
        { action: "resume", title: "Devam et", icon: ACTION_ICONS.play, tone: "accent" },
        { action: "remove", title: "Listeden çıkar", icon: ACTION_ICONS.remove },
      ];
      break;
    case "failed":
      actions = [
        { action: "resume", title: "Yeniden dene", icon: ACTION_ICONS.retry, tone: "accent" },
        { action: "reveal", title: "Klasörü aç", icon: ACTION_ICONS.reveal },
        { action: "remove", title: "Listeden çıkar", icon: ACTION_ICONS.remove },
      ];
      break;
    case "completed":
      actions = [
        { action: "open", title: "Dosyayı aç", icon: ACTION_ICONS.open, tone: "accent" },
        { action: "reveal", title: "Klasörde göster", icon: ACTION_ICONS.reveal },
        { action: "delete", title: "Dosyayla birlikte sil", icon: ACTION_ICONS.trash, tone: "danger" },
      ];
      break;
  }
  if (hasFileSelection(item) && item.status !== "completed") {
    actions.unshift({ action: "files", title: "Dosyaları seç", icon: ACTION_ICONS.files });
  }
  return actions;
}

function createRow(item: Download): RowRefs {
  const root = document.createElement("li");
  root.className = "row";
  root.dataset.id = item.id;
  root.innerHTML = `
    <div class="type-icon"></div>
    <div class="row-main">
      <div class="row-top">
        <span class="name"></span>
        <span class="badge"></span>
      </div>
      <div class="row-sub">
        <span class="host"></span>
        <span class="sep"></span>
        <span class="amount"></span>
        <span class="eta"></span>
        <span class="seg-info"></span>
      </div>
      <div class="bar"><div class="fill"></div></div>
      <div class="row-error"></div>
    </div>
    <div class="row-side">
      <span class="speed"></span>
      <div class="actions"></div>
    </div>`;

  const refs: RowRefs = {
    root,
    icon: root.querySelector(".type-icon") as HTMLDivElement,
    name: root.querySelector(".name") as HTMLElement,
    badge: root.querySelector(".badge") as HTMLElement,
    host: root.querySelector(".host") as HTMLElement,
    amount: root.querySelector(".amount") as HTMLElement,
    eta: root.querySelector(".eta") as HTMLElement,
    segInfo: root.querySelector(".seg-info") as HTMLElement,
    fill: root.querySelector(".fill") as HTMLElement,
    speed: root.querySelector(".speed") as HTMLElement,
    error: root.querySelector(".row-error") as HTMLElement,
    actions: root.querySelector(".actions") as HTMLElement,
    actionsKey: "",
  };
  updateRow(refs, item);
  return refs;
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url.slice(0, 40);
  }
}

function updateRow(refs: RowRefs, item: Download) {
  const frac = clampFraction(item);
  refs.root.dataset.status = item.status;
  refs.name.textContent = item.filename;
  refs.name.title = item.filename;

  const mime =
    item.kind === "torrent" ? "torrent" : item.kind === "media" ? "media" : item.mime ?? "file";
  const iconKey = ICONS[mime] ? mime : mime.split("/")[0];
  refs.icon.innerHTML = `<svg viewBox="0 0 24 24">${ICONS[iconKey] ?? ICONS.file}</svg>`;
  refs.icon.dataset.kind = iconKey;

  refs.badge.textContent = STATUS_TEXT[item.status];
  refs.badge.className = `badge b-${item.status}`;
  refs.host.textContent = hostOf(item.url);
  refs.host.title = item.url;

  const total = item.totalBytes;
  const parts = total ? `${formatBytes(item.downloadedBytes)} / ${formatBytes(total)}` : formatBytes(item.downloadedBytes);
  refs.amount.textContent = total
    ? `${parts} · %${Math.round(frac * 100)}`
    : parts;

  if (item.status === "downloading" && item.speedBps > 0 && total) {
    const remaining = Math.max(0, total - item.downloadedBytes);
    refs.eta.textContent = `${formatEta(remaining / item.speedBps)} kaldı`;
    refs.eta.hidden = false;
  } else {
    refs.eta.hidden = true;
  }

  const segCount = item.segments.length;
  if (item.kind === "torrent" && item.torrentFiles?.length) {
    const files = item.torrentFiles;
    const picked = files.filter((f) => f.included).length;
    const peers = item.swarm?.peers ?? 0;
    refs.segInfo.textContent =
      picked === files.length
        ? `· ${files.length} dosya · ${peers} peer`
        : `· ${picked}/${files.length} dosya · ${peers} peer`;
    refs.segInfo.hidden = false;
  } else if (item.kind === "torrent" && item.status === "downloading") {
    const peers = item.swarm?.peers ?? 0;
    refs.segInfo.textContent = `· ${peers} peer`;
    refs.segInfo.hidden = false;
  } else if (item.status === "downloading" && segCount > 1) {
    refs.segInfo.textContent = `· ${segCount} bağlantı`;
    refs.segInfo.hidden = false;
  } else if (item.kind === "media" && item.status === "completed") {
    refs.segInfo.textContent = "· yt-dlp";
    refs.segInfo.hidden = false;
  } else {
    refs.segInfo.hidden = true;
  }

  refs.fill.style.width = `${(frac * 100).toFixed(2)}%`;
  const indeterminate = !total && item.status === "downloading" && item.downloadedBytes > 0;
  refs.root.classList.toggle("indeterminate", indeterminate);
  if (indeterminate) refs.fill.style.width = "100%";
  if (item.status === "downloading" && item.retry) {
    const wait = Math.max(1, Math.round(item.retry.waitMs / 1000));
    refs.speed.textContent = `Yeniden deneniyor ${item.retry.attempt}/${item.retry.max} · ${wait} sn`;
  } else {
    refs.speed.textContent =
      item.status === "downloading" && item.speedBps > 0 ? `${formatBytes(item.speedBps)}/s` : "";
  }
  refs.speed.classList.toggle("is-retry", Boolean(item.retry));
  refs.error.textContent = item.error ?? "";
  refs.error.hidden = !item.error;

  const key = `${item.status}:${item.filePath ?? ""}:${item.torrentFiles?.length ?? 0}`;
  if (key !== refs.actionsKey) {
    refs.actionsKey = key;
    refs.actions.innerHTML = actionsFor(item)
      .map(
        (a) =>
          `<button class="icon-only ${a.tone ?? ""}" data-action="${a.action}" title="${a.title}" aria-label="${a.title}"><svg viewBox="0 0 24 24">${a.icon}</svg></button>`,
      )
      .join("");
  }
}

function clampFraction(item: Download): number {
  if (item.totalBytes && item.totalBytes > 0) {
    return Math.min(1, Math.max(0, item.downloadedBytes / item.totalBytes));
  }
  if (item.status === "completed") return 1;
  return Math.min(1, Math.max(0, item.progress ?? 0));
}

function readSort(): Sort {
  try {
    const stored = localStorage.getItem(SORT_KEY);
    if (stored && SORT_OPTIONS.includes(stored as Sort)) return stored as Sort;
  } catch {
    // localStorage kapalı olabilir; varsayılana düş.
  }
  return "new";
}

function writeSort(next: Sort) {
  try {
    localStorage.setItem(SORT_KEY, next);
  } catch {
    // sessiz: tercih kaydedilemezse sıralama yine çalışır
  }
}

const normalizeQuery = (value: string) => value.trim().toLocaleLowerCase("tr");

function queryMatches(item: Download): boolean {
  const needle = normalizeQuery(query);
  if (needle.length === 0) return true;
  const haystack = `${item.filename} ${item.url}`.toLocaleLowerCase("tr");
  return haystack.includes(needle);
}

function matches(item: Download): boolean {
  if (!queryMatches(item)) return false;
  switch (filter) {
    case "all":
      return true;
    case "active":
      return item.status === "downloading";
    case "queued":
      return item.status === "queued" || item.status === "paused";
    case "done":
      return item.status === "completed";
    case "failed":
      return item.status === "failed" || item.status === "cancelled";
  }
}

const rows = new Map<string, RowRefs>();

/** Sıralama tercihi; eşitlikte eski kayıt üstte kalsın diye createdAt ikinci anahtar. */
function sortDownloads(items: Download[]): Download[] {
  const copy = [...items];
  const newestFirst = (a: Download, b: Download) => b.createdAt - a.createdAt;
  switch (sort) {
    case "old":
      copy.sort((a, b) => a.createdAt - b.createdAt);
      break;
    case "name":
      copy.sort(
        (a, b) => a.filename.localeCompare(b.filename, "tr") || newestFirst(a, b),
      );
      break;
    case "size":
      copy.sort(
        (a, b) => (b.totalBytes ?? -1) - (a.totalBytes ?? -1) || newestFirst(a, b),
      );
      break;
    case "status":
      copy.sort(
        (a, b) => a.status.localeCompare(b.status, "tr") || newestFirst(a, b),
      );
      break;
    case "new":
    default:
      copy.sort(newestFirst);
      break;
  }
  return copy;
}

function render() {
  const visible = sortDownloads(downloads.filter(matches));
  const visibleIds = new Set(visible.map((d) => d.id));
  for (const [id, refs] of rows) {
    if (!visibleIds.has(id)) {
      refs.root.remove();
      rows.delete(id);
    }
  }
  visible.forEach((item, index) => {
    let refs = rows.get(item.id);
    if (!refs) {
      refs = createRow(item);
      rows.set(item.id, refs);
    } else {
      updateRow(refs, item);
    }
    const current = list.children[index];
    if (current !== refs.root) list.insertBefore(refs.root, current ?? null);
  });

  empty.hidden = visible.length > 0;
  if (visible.length === 0 && downloads.length > 0) {
    if (normalizeQuery(query).length > 0) {
      empty.querySelector("h2")!.textContent = "Aramanla eşleşen kayıt yok";
      const p = empty.querySelector("p")!;
      p.textContent = `"${query.trim()}" için sonuç bulunamadı. Aramayı temizleyip başka bir şey deneyebilirsin.`;
    } else {
      empty.querySelector("h2")!.textContent = "Bu filtrede kayıt yok";
      empty.querySelector("p")!.textContent =
        "Başka bir sekmeye geçebilir ya da yeni bir bağlantı ekleyebilirsin.";
    }
  } else {
    empty.querySelector("h2")!.textContent = "Henüz indirme yok";
    empty.querySelector("p")!.innerHTML =
      "Bir bağlantı yapıştırıp <b>Ekle</b>'ye bas. Hydra dosyayı parçalara bölüp paralel çeker; kapatıp açsan bile kaldığı yerden devam eder.";
  }

  updateCounts();
  updateThroughput();
  updateTitle();
}

function updateCounts() {
  // Sayımlar aramayı dikkate alır: arama açıkken "Tümü 1" gibi daralır.
  const scoped = downloads.filter(queryMatches);
  const count = (fn: (d: Download) => boolean) => scoped.filter(fn).length;
  $("#count-all").textContent = String(scoped.length);
  $("#count-active").textContent = String(count((d) => d.status === "downloading"));
  $("#count-queued").textContent = String(
    count((d) => d.status === "queued" || d.status === "paused"),
  );
  $("#count-done").textContent = String(count((d) => d.status === "completed"));
  $("#count-failed").textContent = String(
    count((d) => d.status === "failed" || d.status === "cancelled"),
  );
}

function updateThroughput() {
  const active = downloads.filter((d) => d.status === "downloading");
  const speed = active.reduce((sum, d) => sum + d.speedBps, 0);
  throughput.textContent =
    active.length > 0
      ? `${active.length} aktif · ${formatBytes(speed)}/s`
      : downloads.length > 0
        ? "boşta"
        : "";
}

function updateTitle() {
  const active = downloads.filter((d) => d.status === "downloading").length;
  document.title = active > 0 ? `Hydra DL — ${active} indirme` : "Hydra DL";
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes)) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

function formatEta(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "az kaldı";
  if (seconds < 60) return `${Math.ceil(seconds)} sn`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} dk`;
  const hours = Math.floor(minutes / 60);
  return `${hours} sa ${minutes % 60} dk`;
}

function toast(message: string, level: Notice["level"] = "info") {
  const node = document.createElement("div");
  node.className = `toast ${level}`;
  node.textContent = message;
  toasts.appendChild(node);
  setTimeout(() => {
    node.classList.add("out");
    setTimeout(() => node.remove(), 250);
  }, 4200);
}

async function addFromForm() {
  const url = urlInput.value.trim();
  if (!url || adding) return;
  adding = true;
  addBtn.disabled = true;
  addBtn.classList.add("busy");
  const kind = kindSelect.value;
  try {
    const media = kind === "file" ? false : kind === "media" ? true : undefined;
    // Kisa yoldan ekleme: kullanici format secmezse en iyi kaliteyle iner.
    // Panel istenirse "Seçenekler" düğmesi probe akışını açar.
    await api.add({ url, ...(media === undefined ? {} : { media }) });
    urlInput.value = "";
    toast("Kuyruğa eklendi", "success");
  } catch (err) {
    toast(String(err), "error");
  } finally {
    adding = false;
    addBtn.disabled = false;
    addBtn.classList.remove("busy");
    urlInput.focus();
  }
}

// ---------------------------------------------------------------------------
// İnceleme paneli (format / kalite seçimi)
// ---------------------------------------------------------------------------

const probeSheet = $<HTMLElement>("#probe-sheet");
const probeTitle = $("#probe-title");
const probeSub = $("#probe-sub");
const probeSummary = $("#probe-summary");
const probeFormats = $("#probe-formats");
const probeExtra = $("#probe-extra");
const probeFilename = $<HTMLInputElement>("#probe-filename");
const probeDir = $<HTMLInputElement>("#probe-dir");

let probeState: ProbeResult | null = null;
let probeSelected: string | null = null;
let probing = false;
/** Probe edilen torrent'in dosya listesi ve secimi. */
let probeFiles: TorrentFile[] | null = null;
let probeFilesToken = 0;

const probeFilesBox = $<HTMLElement>("#probe-files");
const filesSheet = $<HTMLElement>("#files-sheet");
const filesList = $<HTMLElement>("#files-list");
const filesSub = $("#files-sub");
let filesEditId: string | null = null;
let filesEditSelection: TorrentFile[] = [];

/** Dosya listesini onay kutulariyla cizer; secim degistikce `onChange` cagrilir. */
function renderFileList(
  container: HTMLElement,
  files: TorrentFile[],
  onChange?: (files: TorrentFile[]) => void,
): void {
  /** Basligi secime gore gunceller. */
  const updateHead = (head: HTMLElement) => {
    const picked = files.filter((f) => f.included);
    const total = picked.reduce((sum, f) => sum + f.length, 0);
    const label = head.querySelector("strong");
    if (label) label.textContent = `${picked.length}/${files.length} dosya`;
    const amount = head.querySelector(".files-amount");
    if (amount) {
      amount.textContent =
        picked.length === files.length
          ? `· ${formatBytes(total)} (tümü)`
          : `· ${formatBytes(total)} seçili`;
    }
  };

  const head = document.createElement("div");
  head.className = "files-head";
  head.innerHTML = `<strong></strong><span class="files-amount"></span>
    <span class="files-actions">
      <button class="ghost" data-files="all" type="button">Tümü</button>
      <button class="ghost" data-files="none" type="button">Hiçbiri</button>
    </span>`;
  container.replaceChildren(head);
  updateHead(head);

  for (const file of files) {
    const row = document.createElement("label");
    row.className = `file-row${file.included ? " on" : ""}`;
    row.innerHTML = `
      <input type="checkbox" ${file.included ? "checked" : ""} />
      <span class="file-name" title="${escapeAttr(file.path)}">${escapeHtml(file.path)}</span>
      <span class="file-size">${formatBytes(file.length)}</span>`;
    const box = row.querySelector("input") as HTMLInputElement;
    box.addEventListener("change", () => {
      file.included = box.checked;
      row.classList.toggle("on", box.checked);
      updateHead(head);
      onChange?.(files);
    });
    container.append(row);
  }

  head.querySelector<HTMLButtonElement>('[data-files="all"]')?.addEventListener("click", () => {
    for (const file of files) file.included = true;
    renderFileList(container, files, onChange);
    onChange?.(files);
  });
  head.querySelector<HTMLButtonElement>('[data-files="none"]')?.addEventListener("click", () => {
    for (const file of files) file.included = false;
    renderFileList(container, files, onChange);
    onChange?.(files);
  });
}

/** Secili dosya indeksleri; hepsi seciliyse `undefined` (motor tumunu indirir). */
function selectedIndexes(files: TorrentFile[]): number[] | undefined {
  const picked = files.filter((f) => f.included).map((f) => f.index);
  if (picked.length === 0 || picked.length === files.length) return undefined;
  return picked;
}

let probeCloseTimer: ReturnType<typeof setTimeout> | null = null;

function closeProbe() {
  probeSheet.classList.remove("open");
  probeCloseTimer = setTimeout(() => {
    probeSheet.hidden = true;
    probeCloseTimer = null;
  }, 200);
  overlay.hidden = true;
}

function openProbe(result: ProbeResult) {
  probeState = result;
  probeSelected = result.formats[0]?.id ?? null;
  probeTitle.textContent = result.title || "İndirme seçenekleri";
  probeSub.textContent = hostOf(result.finalUrl || result.url);
  probeFilename.value = result.filename ?? "";
  probeDir.value = settings?.downloadDir ?? info?.downloadDir ?? "";

  const bits: string[] = [];
  if (result.size) bits.push(formatBytes(result.size));
  if (result.kind) bits.push(kindLabel(result.kind));
  if (result.resumable) bits.push("devam ettirilebilir");
  if (result.durationSecs) bits.push(formatDuration(result.durationSecs));
  probeSummary.textContent = bits.join(" · ");
  probeSummary.hidden = bits.length === 0;

  const rows: string[] = [];
  for (const format of result.formats) {
    const checked = format.id === probeSelected ? "checked" : "";
    rows.push(
      `<label class="format-row${checked ? " selected" : ""}">
        <input type="radio" name="probe-format" value="${escapeAttr(format.id)}" ${checked} />
        <span class="format-label">${escapeHtml(format.label)}</span>
        ${format.note ? `<span class="format-note">${escapeHtml(format.note)}</span>` : ""}
      </label>`,
    );
  }
  if (rows.length === 0) {
    rows.push(
      `<p class="hint">${
        result.kind === "torrent"
          ? "Torrent tek parça olarak iner; hangi dosyaların ineceğini aşağıdan seçersin."
          : "Bu bağlantı için kalite listesi yok; dosya olduğu gibi indirilecek."
      }</p>`,
    );
  }
  probeFormats.innerHTML = rows.join("");

  const extra: string[] = [];
  if (result.candidates.length > 1) {
    extra.push(
      `<details class="candidates"><summary>Diğer kaynaklar (${result.candidates.length})</summary>${result.candidates
        .map(
          (candidate) =>
            `<div class="candidate"><span>${escapeHtml(candidate.label)}</span><code title="${escapeAttr(candidate.url)}">${escapeHtml(shortenUrl(candidate.url))}</code></div>`,
        )
        .join("")}</details>`,
    );
  }
  for (const warning of result.warnings) {
    extra.push(`<p class="hint warn">${escapeHtml(warning)}</p>`);
  }
  if (result.needsYtdlp) {
    extra.push(`<p class="hint">Kalite listesi yt-dlp'den alındı.</p>`);
  }
  probeExtra.innerHTML = extra.join("");

  // Torrent: dosya listesi ve secim (arka planda getirilir).
  probeFiles = null;
  const token = ++probeFilesToken;
  if (result.kind === "torrent") {
    probeFilesBox.hidden = false;
    probeFilesBox.innerHTML = `<p class="hint">Torrent dosyaları okunuyor… (magnet ise peer beklenir)</p>`;
    void loadTorrentFiles(result.url, token);
  } else {
    probeFilesBox.hidden = true;
    probeFilesBox.replaceChildren();
  }

  probeSheet.hidden = false;
  overlay.hidden = false;
  if (probeCloseTimer) {
    clearTimeout(probeCloseTimer);
    probeCloseTimer = null;
  }
  requestAnimationFrame(() => probeSheet.classList.add("open"));
  $("#probe-download").focus();
}

/** Probe penceresi icin torrent dosya listesini getirir. */
async function loadTorrentFiles(source: string, token: number) {
  try {
    const listing: TorrentListing = await api.listTorrentFiles(source);
    if (token !== probeFilesToken) return;
    probeFiles = listing.files;
    if (listing.files.length === 0) {
      probeFilesBox.hidden = true;
      return;
    }
    if (probeState?.kind === "torrent") {
      const newTotal = listing.files.reduce((sum, f) => sum + f.length, 0);
      const bits: string[] = [];
      if (newTotal) bits.push(formatBytes(newTotal));
      bits.push(`${listing.files.length} dosya`);
      bits.push("torrent");
      if (probeState.resumable) bits.push("devam ettirilebilir");
      probeSummary.textContent = bits.join(" · ");
      probeSummary.hidden = false;
      if (listing.name) probeTitle.textContent = listing.name;
      if (!probeFilename.value.trim()) probeFilename.value = listing.name;
    }
    renderFileList(probeFilesBox, listing.files);
  } catch (err) {
    if (token !== probeFilesToken) return;
    probeFilesBox.hidden = false;
    probeFilesBox.innerHTML = `<p class="hint warn">Dosya listesi alınamadı: ${escapeHtml(String(err))}</p>`;
  }
}

probeFormats.addEventListener("change", (event) => {
  const input = event.target as HTMLInputElement;
  if (input.name !== "probe-format") return;
  probeSelected = input.value;
  for (const row of probeFormats.querySelectorAll(".format-row")) {
    row.classList.toggle("selected", row.contains(input));
  }
});

$("#probe-close").addEventListener("click", closeProbe);
$("#probe-cancel").addEventListener("click", closeProbe);

$("#probe-download").addEventListener("click", async () => {
  if (!probeState) return;
  const button = $<HTMLButtonElement>("#probe-download");
  button.disabled = true;
  try {
    const format = probeSelected;
    const files = probeFiles ? selectedIndexes(probeFiles) : undefined;
    if (probeFiles && probeFiles.every((f) => !f.included)) {
      toast("En az bir dosya seçmelisin", "error");
      return;
    }
    await api.add({
      url: probeState.url,
      filename: probeFilename.value.trim() || undefined,
      media: probeState.kind === "media" || probeState.kind === "hls" || probeState.kind === "dash"
        ? true
        : undefined,
      format: format && format !== "direct" ? format : undefined,
      files,
    });
    toast(files ? `${files.length} dosya indirilecek` : "Kuyruğa eklendi", "success");
    closeProbe();
    urlInput.value = "";
  } catch (err) {
    toast(String(err), "error");
  } finally {
    button.disabled = false;
  }
});

/* ---------------- Eklenmis torrent'in dosyalarini degistirme ---------------- */

let filesCloseTimer: ReturnType<typeof setTimeout> | null = null;

function openFilesSheet(item: Download) {
  if (!item.torrentFiles?.length) return;
  filesEditId = item.id;
  filesEditSelection = item.torrentFiles.map((f) => ({ ...f }));
  filesSub.textContent = `${item.filename} · ${hostOf(item.url)}`;
  renderFileList(filesList, filesEditSelection);
  filesSheet.hidden = false;
  overlay.hidden = false;
  if (filesCloseTimer) {
    clearTimeout(filesCloseTimer);
    filesCloseTimer = null;
  }
  requestAnimationFrame(() => filesSheet.classList.add("open"));
  $("#files-apply").focus();
}

function closeFilesSheet() {
  filesSheet.classList.remove("open");
  filesCloseTimer = setTimeout(() => {
    filesSheet.hidden = true;
    filesCloseTimer = null;
  }, 200);
  overlay.hidden = true;
  filesEditId = null;
}

$("#files-close").addEventListener("click", closeFilesSheet);
$("#files-cancel").addEventListener("click", closeFilesSheet);

$("#files-apply").addEventListener("click", async () => {
  const id = filesEditId;
  if (!id) return;
  const picked = filesEditSelection.filter((f) => f.included).map((f) => f.index);
  if (picked.length === 0) {
    toast("En az bir dosya seçmelisin", "error");
    return;
  }
  const button = $<HTMLButtonElement>("#files-apply");
  button.disabled = true;
  try {
    await api.setTorrentFiles(id, picked);
    toast(`${picked.length} dosya seçildi`, "success");
    closeFilesSheet();
  } catch (err) {
    toast(String(err), "error");
  } finally {
    button.disabled = false;
  }
});

async function probeAndOpen() {
  const url = urlInput.value.trim();
  if (!url || probing) return;
  probing = true;
  addBtn.classList.add("busy");
  try {
    if (!settings) settings = await api.getSettings();
    const kind = kindSelect.value;
    const result = await api.probe(url, kind === "file" ? false : kind === "media" ? true : undefined);
    openProbe(result);
  } catch (err) {
    toast(String(err), "error");
  } finally {
    probing = false;
    addBtn.classList.remove("busy");
  }
}

function kindLabel(kind: string): string {
  switch (kind) {
    case "file":
      return "dosya";
    case "media":
      return "medya";
    case "torrent":
      return "torrent";
    case "hls":
      return "HLS akışı";
    case "dash":
      return "DASH akışı";
    case "page":
      return "sayfa";
    default:
      return kind;
  }
}

function formatDuration(seconds: number): string {
  const total = Math.round(seconds);
  const minutes = Math.floor(total / 60);
  const rest = total % 60;
  return `${minutes}:${String(rest).padStart(2, "0")}`;
}

function shortenUrl(url: string): string {
  try {
    const parsed = new URL(url);
    const name = parsed.pathname.split("/").filter(Boolean).pop() ?? parsed.hostname;
    return `${parsed.hostname}/…/${name}`.slice(0, 64);
  } catch {
    return url.slice(0, 64);
  }
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function escapeAttr(value: string): string {
  return escapeHtml(value).replaceAll("'", "&#39;");
}

addForm.addEventListener("submit", (event) => {
  event.preventDefault();
  void addFromForm();
});

$("#probe-btn").addEventListener("click", () => void probeAndOpen());

list.addEventListener("click", async (event) => {
  const target = event.target as HTMLElement;
  const button = target.closest<HTMLButtonElement>("button[data-action]");
  if (!button) return;
  const row = button.closest<HTMLElement>(".row");
  const id = row?.dataset.id;
  if (!id) return;
  const action = button.dataset.action;
  button.disabled = true;
  try {
    switch (action) {
      case "pause":
        await api.pause(id);
        break;
      case "resume":
        await api.resume(id);
        break;
      case "cancel":
        await api.cancel(id);
        toast("İndirme iptal edildi", "info");
        break;
      case "remove":
        await api.remove(id, false);
        break;
      case "delete":
        if (confirm("Dosya diskten de silinsin mi?")) {
          await api.remove(id, true);
          toast("Dosya ve kayıt silindi", "info");
        }
        break;
      case "open":
        await api.openDownload(id);
        break;
      case "reveal":
        await api.revealDownload(id);
        break;
      case "files": {
        const item = downloads.find((d) => d.id === id);
        if (item) openFilesSheet(item);
        break;
      }
    }
  } catch (err) {
    toast(String(err), "error");
  } finally {
    button.disabled = false;
  }
});

function resetRows() {
  for (const refs of rows.values()) refs.root.remove();
  rows.clear();
}

filters.addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-filter]");
  if (!button) return;
  filter = button.dataset.filter as Filter;
  for (const chip of filters.querySelectorAll("button")) {
    chip.classList.toggle("active", chip === button);
  }
  resetRows();
  render();
});

// Arama: yazdıkça süzer (kayıt sayısı yüzlerle sınırlı, debounce gereksiz).
searchInput.addEventListener("input", () => {
  query = searchInput.value;
  resetRows();
  render();
});

// Sıralama: tercih kalıcı (localStorage), satırlar yeniden dizilir.
sortSelect.value = sort;
sortSelect.addEventListener("change", () => {
  sort = sortSelect.value as Sort;
  writeSort(sort);
  render();
});

$("#clear-btn").addEventListener("click", async () => {
  try {
    await api.clearFinished(false);
    toast("Biten kayıtlar temizlendi", "info");
  } catch (err) {
    toast(String(err), "error");
  }
});

$("#paste-btn").addEventListener("click", async () => {
  try {
    const text = await navigator.clipboard.readText();
    if (text.trim()) {
      urlInput.value = text.trim();
      urlInput.focus();
    }
  } catch {
    urlInput.focus();
    toast("Panoya erişilemedi — Ctrl+V ile yapıştır", "info");
  }
});

function captureText(state: { enabled: boolean; running: boolean; url: string | null }): string {
  if (!state.enabled) return "yakalama kapalı";
  if (!state.running) return "yakalama bekliyor";
  return `yakalama :${state.url?.split(":").pop() ?? ""}`;
}

function updateCapture(info: AppInfo | null) {
  if (!info) return;
  const state = info.capture;
  captureDot.className = `status-dot ${state.running ? "on" : state.enabled ? "warn" : "off"}`;
  captureLabel.textContent = captureText(state);
  captureFooter.textContent = state.running && state.url ? `Eklenti: ${state.url}/add` : "";
  capturePill.title = state.url ? `${state.url}/health` : "Yakalama sunucusu kapalı";
}

async function loadInfo() {
  try {
    info = await api.appInfo();
    updateCapture(info);
    footerInfo.textContent = `Hydra DL v${info.version} · ${info.downloadDir}`;
  } catch (err) {
    console.error(err);
  }
}

// ---------------------------------------------------------------------------
// Ayarlar
// ---------------------------------------------------------------------------

const setDir = $<HTMLInputElement>("#set-dir");
const setConcurrent = $<HTMLInputElement>("#set-concurrent");
const setConnections = $<HTMLInputElement>("#set-connections");
const setSpeed = $<HTMLInputElement>("#set-speed");
const setAutoResume = $<HTMLInputElement>("#set-autoresume");
const setNotify = $<HTMLInputElement>("#set-notify");
const setProxy = $<HTMLInputElement>("#set-proxy");
const setCapture = $<HTMLInputElement>("#set-capture");
const setCapturePort = $<HTMLInputElement>("#set-capture-port");
const captureUrl = $<HTMLInputElement>("#capture-url");
const setYtdlp = $<HTMLInputElement>("#set-ytdlp");
const setAsk = $<HTMLInputElement>("#set-ask");
const setHunter = $<HTMLInputElement>("#set-hunter");
const setWeb = $<HTMLInputElement>("#set-web");
const setWebPort = $<HTMLInputElement>("#set-web-port");
const setWebToken = $<HTMLInputElement>("#set-web-token");
const setTorrent = $<HTMLInputElement>("#set-torrent");
const webUrl = $<HTMLInputElement>("#web-url");
const qrRow = $("#qr-row");
const webQr = $<HTMLImageElement>("#web-qr");
const ytdlpStatus = $("#ytdlp-status");
const ytdlpInstallStatus = $("#ytdlp-install-status");
const installYtdlpBtn = $<HTMLButtonElement>("#install-ytdlp");
const notifyTestBtn = $<HTMLButtonElement>("#notify-test");
const aboutInfo = $("#about-info");
const stateInfo = $("#state-info");

function bindSettingsForm(source: Settings, appInfo: AppInfo | null) {
  setDir.value = source.downloadDir;
  setConcurrent.value = String(source.maxConcurrent);
  setConnections.value = String(source.connections);
  setSpeed.value = String(source.speedLimitKbps);
  setAutoResume.checked = source.autoResume;
  setNotify.checked = source.notifyOnFinish;
  setProxy.value = source.proxyUrl ?? "";
  setCapture.checked = source.captureEnabled;
  setCapturePort.value = String(source.capturePort);
  captureUrl.value =
    appInfo?.capture.url ?? `http://127.0.0.1:${source.capturePort}`;
  setAsk.checked = source.askOnDownload;
  setHunter.checked = source.hunterEnabled;
  setWeb.checked = source.webEnabled;
  setWebPort.value = String(source.webPort);
  setWebToken.value = source.webToken;
  setTorrent.checked = source.torrentEnabled;
  const web = appInfo?.web;
  webUrl.value = web?.url ?? `http://127.0.0.1:${source.webPort}`;
  if (appInfo?.webQr) {
    webQr.src = appInfo.webQr;
    qrRow.hidden = false;
  } else {
    qrRow.hidden = true;
  }
  setYtdlp.value = source.ytdlpPath;
  const availability = appInfo?.ytdlp;
  ytdlpStatus.textContent = availability?.available
    ? `Bulundu: ${availability.path}${availability.version ? ` (v${availability.version})` : ""}`
    : "yt-dlp bulunamadı — video indirmeleri için gerekli.";
  ytdlpStatus.className = `hint ${availability?.available ? "ok" : "warn"}`;
  aboutInfo.textContent = appInfo
    ? `Sürüm ${appInfo.version} · çalışma süresi ${formatUptime(appInfo.uptimeSecs)}`
    : "";
  stateInfo.textContent = appInfo ? `Durum dosyası: ${appInfo.statePath}` : "";
}

function formatUptime(seconds: number): string {
  if (seconds < 60) return `${seconds} sn`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} dk`;
  return `${Math.floor(minutes / 60)} sa ${minutes % 60} dk`;
}

async function openSettings() {
  try {
    const [loaded, loadedInfo] = await Promise.all([api.getSettings(), api.appInfo()]);
    settings = loaded;
    info = loadedInfo;
    bindSettingsForm(loaded, loadedInfo);
    updateCapture(loadedInfo);
    overlay.hidden = false;
    drawer.hidden = false;
    requestAnimationFrame(() => drawer.classList.add("open"));
  } catch (err) {
    toast(String(err), "error");
  }
}

function closeSettings() {
  drawer.classList.remove("open");
  overlay.hidden = true;
  setTimeout(() => {
    drawer.hidden = true;
  }, 220);
}

async function saveSettings() {
  if (!settings) return;
  const next: Settings = {
    ...settings,
    downloadDir: setDir.value.trim(),
    maxConcurrent: clampNumber(setConcurrent.value, 1, 10, 3),
    connections: clampNumber(setConnections.value, 1, 16, 8),
    speedLimitKbps: clampNumber(setSpeed.value, 0, 10_000_000, 0),
    autoResume: setAutoResume.checked,
    notifyOnFinish: setNotify.checked,
    proxyUrl: setProxy.value.trim(),
    captureEnabled: setCapture.checked,
    capturePort: clampNumber(setCapturePort.value, 1024, 65535, 6712),
    ytdlpPath: setYtdlp.value.trim(),
    askOnDownload: setAsk.checked,
    hunterEnabled: setHunter.checked,
    webEnabled: setWeb.checked,
    webPort: clampNumber(setWebPort.value, 1024, 65535, 6790),
    webToken: setWebToken.value.trim().replace(/[^A-Za-z0-9]/g, ""),
    torrentEnabled: setTorrent.checked,
  };
  try {
    settings = await api.saveSettings(next);
    await loadInfo();
    bindSettingsForm(settings, info);
    toast("Ayarlar kaydedildi", "success");
    closeSettings();
  } catch (err) {
    toast(String(err), "error");
  }
}

function clampNumber(raw: string, min: number, max: number, fallback: number): number {
  const value = Number.parseInt(raw, 10);
  if (!Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, value));
}

$("#settings-btn").addEventListener("click", () => void openSettings());
capturePill.addEventListener("click", () => void openSettings());
$("#settings-close").addEventListener("click", closeSettings);
$("#cancel-settings").addEventListener("click", closeSettings);
overlay.addEventListener("click", closeSettings);
$("#save-settings").addEventListener("click", () => void saveSettings());

$("#gen-token").addEventListener("click", () => {
  const bytes = new Uint8Array(6);
  crypto.getRandomValues(bytes);
  setWebToken.value = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
});

$("#copy-web").addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(webUrl.value);
    toast("Web adresi kopyalandı", "success");
  } catch {
    webUrl.select();
    toast("Kopyalanamadı — elle seçip Ctrl+C kullan", "info");
  }
});

// ---------------------------------------------------------------------------
// yt-dlp kurulumu
// ---------------------------------------------------------------------------

function renderInstallProgress(progress: InstallProgress) {
  if (progress.stage === "downloading") {
    const total = progress.total;
    if (total && total > 0) {
      const percent = Math.round((progress.downloaded / total) * 100);
      ytdlpInstallStatus.textContent = `İndiriliyor… %${percent} (${formatBytes(progress.downloaded)} / ${formatBytes(total)})`;
    } else {
      ytdlpInstallStatus.textContent = `İndiriliyor… ${formatBytes(progress.downloaded)}`;
    }
    ytdlpInstallStatus.className = "hint";
    return;
  }
  if (progress.stage === "verifying") {
    ytdlpInstallStatus.textContent = "Doğrulanıyor…";
    ytdlpInstallStatus.className = "hint";
    return;
  }
  if (progress.stage === "done") {
    ytdlpInstallStatus.textContent = "Kuruldu ve doğrulandı.";
    ytdlpInstallStatus.className = "hint ok";
    return;
  }
  if (progress.stage === "failed") {
    ytdlpInstallStatus.textContent = progress.error ?? "Kurulum başarısız";
    ytdlpInstallStatus.className = "hint warn";
  }
}

installYtdlpBtn.addEventListener("click", async () => {
  installYtdlpBtn.disabled = true;
  ytdlpInstallStatus.textContent = "Başlatılıyor…";
  ytdlpInstallStatus.className = "hint";
  try {
    const status = await api.installYtdlp();
    if (status.available) {
      toast("yt-dlp kuruldu", "success");
      await loadInfo();
      if (settings) bindSettingsForm(settings, info);
    }
  } catch (err) {
    ytdlpInstallStatus.textContent = String(err);
    ytdlpInstallStatus.className = "hint warn";
    toast(String(err), "error");
  } finally {
    installYtdlpBtn.disabled = false;
  }
});

notifyTestBtn.addEventListener("click", async () => {
  notifyTestBtn.disabled = true;
  try {
    await api.notifyTest();
    toast("Test bildirimi gönderildi", "success");
  } catch (err) {
    toast(`Bildirim gönderilemedi: ${String(err)}`, "error");
  } finally {
    notifyTestBtn.disabled = false;
  }
});

$("#browse-btn").addEventListener("click", async () => {
  try {
    const chosen = await open({
      directory: true,
      multiple: false,
      defaultPath: setDir.value || undefined,
      title: "İndirme klasörü seç",
    });
    if (typeof chosen === "string") setDir.value = chosen;
  } catch (err) {
    toast(String(err), "error");
  }
});

$("#copy-capture").addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(captureUrl.value);
    toast("Adres kopyalandı", "success");
  } catch {
    captureUrl.select();
    toast("Kopyalanamadı — elle seçip Ctrl+C kullan", "info");
  }
});

setCapture.addEventListener("change", () => {
  captureUrl.value = `http://127.0.0.1:${setCapturePort.value || 6712}`;
});

setCapturePort.addEventListener("input", () => {
  if (setCapture.checked) {
    captureUrl.value = `http://127.0.0.1:${setCapturePort.value || 6712}`;
  }
});

setWebPort.addEventListener("input", () => {
  webUrl.value = `http://127.0.0.1:${setWebPort.value || 6790}`;
});

// ---------------------------------------------------------------------------
// Kısayollar
// ---------------------------------------------------------------------------

window.addEventListener("keydown", (event) => {
  const target = event.target as HTMLElement | null;
  const typing =
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target?.isContentEditable === true;

  if (event.key === "Escape") {
    if (!probeSheet.hidden) {
      closeProbe();
      return;
    }
    if (!drawer.hidden) closeSettings();
    if (document.activeElement === searchInput && searchInput.value.length > 0) {
      searchInput.value = "";
      query = "";
      resetRows();
      render();
    }
  }
  // Ctrl+F ya da tek başına "/" → geçmişte arama kutusuna git.
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
    event.preventDefault();
    searchInput.focus();
    searchInput.select();
    return;
  }
  if (event.key === "/" && !typing) {
    event.preventDefault();
    searchInput.focus();
    return;
  }
  if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
    event.preventDefault();
    void probeAndOpen();
  }
  if ((event.ctrlKey || event.metaKey) && (event.key === "," || event.key === "ö")) {
    event.preventDefault();
    void openSettings();
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "l") {
    event.preventDefault();
    urlInput.focus();
    urlInput.select();
  }
});

// ---------------------------------------------------------------------------
// Baslat
// ---------------------------------------------------------------------------

async function bootstrap() {
  await Promise.all([
    onDownloads((items) => {
      downloads = items;
      render();
    }),
    onNotice((notice) => toast(notice.message, notice.level)),
    onYtdlpInstall(renderInstallProgress),
  ]);

  try {
    downloads = await api.list();
    render();
  } catch (err) {
    toast(`Liste alınamadı: ${err}`, "error");
  }
  await loadInfo();
  urlInput.focus();

  if ("isTauri" in window) {
    addHint.textContent =
      "İpucu: tarayıcı eklentisiyle bağlantıları tek tıkla buraya gönder. Ctrl+L adres çubuğu, Ctrl+, ayarlar.";
  }
}

void bootstrap();
