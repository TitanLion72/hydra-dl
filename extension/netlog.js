// Hydra DL — ağdan medya yakalama (IDM tarzı "tarayıcı ne indiriyorsa onu gör").
//
// Neden gerekli: birçok site medyayı sayfada göstermez; JS ile `m3u8`/`mpd`/`mp4`
// isteği atar. Sayfa HTML'ini taramak bunları göremez. `webRequest` ile tarayıcının
// **gerçekten** istediği adresleri görürüz — site ne olursa olsun.
//
// Gizlilik: varsayılan **kapalı**. Açıkken bile yalnız medya uzantılı/türlü
// istekler saklanır, başka hiçbir istek kaydedilmez. Kayıt sekmeye özeldir,
// sekme kapanınca silinir ve asla dışarıya gönderilmez (yalnız kullanıcının
// kendi Hydra sunucusuna, kullanıcı "İndir"e basınca).
//
// Klasik script: service worker'da `importScripts`, panelde `<script>` ile yüklenir.

/** Tek sekmede tutulacak en fazla kayıt (bellek/panel şişmesin). */
const NETLOG_LIMIT = 60;
/** Manifest/segment isteklerinde üst sınır: bu boyuttan büyük olanlar şüpheli. */
const NETLOG_MAX_URL = 4096;

/** Doğrudan indirilebilir medya uzantıları (segmentler Hariç). */
const MEDIA_FILE_EXT =
  /\.(mp4|webm|mkv|m4v|mov|avi|flv|wmv|3gp|mpe?g|ogv|mp3|m4a|aac|flac|wav|ogg|opus|weba|m4b|aiff)(\?|#|$)/i;

/** Akış manifestleri: en değerli yakalama (tek URL → tüm video). */
const MEDIA_MANIFEST_EXT = /\.(m3u8|mpd|ism|f4m)(\?|#|$)/i;

/**
 * HLS/DASH **parça** istekleri. Bunlar tek tek indirilemez/anlamsız;
 * manifest'i yakalamak yeterli. Listeye koyarsak panel çöplüğe döner.
 * (`.aac` burada yok: tek başına ses dosyası da olabilir.)
 */
const MEDIA_SEGMENT_EXT = /\.(ts|m4s|cmfv|cmfa|vtt)(\?|#|$)/i;

/** `ac=3` bilgisi: 1=other, 2=script, 3=image, 4=stylesheet, 5=object, ... */
const NETLOG_SKIP_TYPES = new Set(["image", "font", "stylesheet", "script"]);

/**
 * Bir isteği sınıflandırır.
 *
 * @param {string} url İstek adresi.
 * @param {string} [contentType] Yanıt türü (varsa; uzantı yoksa kullanılır).
 * @param {string} [resourceType] `webRequest` türü ("media", "xmlhttprequest", ...).
 * @returns {{kind: "hls"|"dash"|"video"|"audio"|"file", ext: string} | null}
 *   Medya değilse `null`.
 */
function classifyMediaRequest(url, contentType, resourceType) {
  if (typeof url !== "string" || url.length === 0 || url.length > NETLOG_MAX_URL) return null;
  if (!/^https?:/i.test(url)) return null;
  if (resourceType && NETLOG_SKIP_TYPES.has(String(resourceType))) return null;

  // 1) Manifest: en değerli.
  if (MEDIA_MANIFEST_EXT.test(url)) {
    return { kind: /\.mpd(\?|#|$)/i.test(url) ? "dash" : "hls", ext: /\.mpd/i.test(url) ? "mpd" : "m3u8" };
  }

  // 2) Akış parçaları: manifest varken gereksiz, listeye alma.
  if (MEDIA_SEGMENT_EXT.test(url)) return null;

  // 3) Doğrudan medya dosyası.
  if (MEDIA_FILE_EXT.test(url)) {
    const ext = url.match(MEDIA_FILE_EXT)[1].toLowerCase();
    const video = /^(mp4|webm|mkv|m4v|mov|avi|flv|wmv|3gp|mpe?g|ogv)$/.test(ext);
    return { kind: video ? "video" : "audio", ext };
  }

  // 4) Uzantı yok ama yanıt türü medya (CDN'ler sık yapar).
  const type = typeof contentType === "string" ? contentType.toLowerCase() : "";
  if (type.startsWith("video/")) return { kind: "video", ext: extFromMime(type, "mp4") };
  if (type.startsWith("audio/")) return { kind: "audio", ext: extFromMime(type, "m4a") };
  if (type.includes("mpegurl")) return { kind: "hls", ext: "m3u8" };
  if (type.includes("dash+xml")) return { kind: "dash", ext: "mpd" };

  return null;
}

function extFromMime(mime, fallback) {
  if (mime.includes("webm")) return "webm";
  if (mime.includes("mp4")) return "mp4";
  if (mime.includes("matroska")) return "mkv";
  if (mime.includes("ogg")) return "ogg";
  if (mime.includes("mpeg") && mime.startsWith("audio/")) return "mp3";
  if (mime.includes("flac")) return "flac";
  if (mime.includes("wav")) return "wav";
  return fallback;
}

/**
 * Yeni kaydı listeye ekler (en yeni başta, tekrarsız, sınırlı).
 *
 * Saf fonksiyon: girdi dizisi değiştirilmez; yeni dizi döner. Test edilebilir.
 */
function pushNetEntry(entries, entry, limit = NETLOG_LIMIT) {
  const list = Array.isArray(entries) ? entries.slice() : [];
  if (!entry || typeof entry.url !== "string") return list;
  const withoutDuplicate = list.filter((item) => item.url !== entry.url);
  withoutDuplicate.unshift(entry);
  return withoutDuplicate.slice(0, Math.max(1, limit));
}

/** Panelde gösterilecek kısa etiket. */
function netEntryLabel(entry) {
  if (!entry) return "";
  if (entry.kind === "hls") return "HLS akışı";
  if (entry.kind === "dash") return "DASH akışı";
  if (entry.kind === "video") return "Video";
  if (entry.kind === "audio") return "Ses";
  return "Dosya";
}

/** Dosya boyutu (yanıt başlığındaki `content-length`). */
function netEntrySize(entry) {
  const size = Number(entry && entry.size);
  if (!Number.isFinite(size) || size <= 0) return "";
  const units = ["B", "KB", "MB", "GB"];
  let value = size;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 100 || unit === 0 ? 0 : 1;
  return `${value.toFixed(digits).replace(".", ",")} ${units[unit]}`;
}

/** Adresten okunabilir dosya adı. */
function netEntryName(entry) {
  try {
    const parsed = new URL(entry.url);
    const last = decodeURIComponent(parsed.pathname.split("/").filter(Boolean).pop() || "");
    if (last) return last;
    return parsed.hostname;
  } catch (_err) {
    return entry.url;
  }
}

// `self` service worker'da, `window` panelde; ikisi de `globalThis`ın takma adı.
// (Node/vm testlerinde de çalışsın diye globalThis kullanıyoruz.)
globalThis.HydraNetlog = {
  NETLOG_LIMIT,
  classifyMediaRequest,
  pushNetEntry,
  netEntryLabel,
  netEntrySize,
  netEntryName,
};
