/* Hydra DL — bağlantı listesi ayrıştırıcı (saf, **tek kaynak**).
 *
 * Toplu içe aktarma: kullanıcı bir metin bloğu yapıştırır; içinden geçerli
 * bağlantıları çıkarır, yinelenenleri eler, bağlantı olmayan satırları bildirir.
 *
 * Masaüstü (`src/links.ts` üzerinden) ve telefon (`webapp/app.js`) bu dosyayı
 * kullanır — iki ayrı kopya tutmuyoruz, davranış ayrışmıyor.
 *
 * Kurallar:
 * - Yalnız `http://`, `https://` ve `magnet:?` kabul edilir; diğer şemalar
 *   (`javascript:`, `file:` …) **sessizce atılmaz**, reddedilenlerde görünür.
 * - Satır sonu, virgül, boşluk ve `<...>` sarmalı ayırıcı sayılır: tek satırda
 *   birden çok bağlantı yapıştırılsa da hepsi bulunur.
 * - Sondaki noktalama (`.`, `,`, `;`, `!`, `?`, kapanmamış parantez) temizlenir;
 *   URL'nin parçası olan dengeli parantez korunur.
 */

/** Tek seferde kabul edilen en fazla bağlantı (yanlışlıkla dev dosya yapıştırmaya karşı). */
export const MAX_LINKS = 500;

const TOKEN_RE = /(?:https?:\/\/|magnet:\?)[^\s"'<>]+/gi;
const BAD_SCHEME_RE = /^[a-z][a-z0-9+.-]*:/i;

/** Baştaki tırnak/parantez ve sondaki noktalamayı temizler. */
export function trimLink(token) {
  let text = String(token ?? "").trim();
  text = text.replace(/^[("'`[{]+/, "");
  for (;;) {
    const last = text[text.length - 1];
    if (!last) break;
    if (last === ")" || last === "]" || last === "}") {
      const opener = last === ")" ? "(" : last === "]" ? "[" : "{";
      const opens = text.split(opener).length - 1;
      const closes = text.split(last).length - 1;
      if (closes > opens) {
        text = text.slice(0, -1);
        continue;
      }
      break;
    }
    if (",.;:!?'\"`>".includes(last)) {
      text = text.slice(0, -1);
      continue;
    }
    break;
  }
  return text;
}

/** Bu bağlantı destekleniyor mu? (`http`, `https`, `magnet`) */
export function isSupportedLink(text) {
  const value = trimLink(text).toLowerCase();
  return value.startsWith("http://") || value.startsWith("https://") || value.startsWith("magnet:?");
}

/**
 * Metni bağlantı listesine çevirir.
 *
 * @param {string} text Kullanıcının yapıştırdığı metin.
 * @returns {{links: string[], rejected: string[], duplicates: number, truncated: number}}
 */
export function parseLinkList(text) {
  const links = [];
  const rejected = [];
  const seen = new Set();
  let duplicates = 0;
  let truncated = 0;
  if (typeof text !== "string" || text.length === 0) {
    return { links, rejected, duplicates, truncated };
  }

  // `<https://ornek/x>` sarmalını boz: açılı parantezler URL'de geçmez.
  // Ayrıca bağlantıdan hemen önce gelen ayırıcıyı boşluğa çevir: `a,https://b`
  // tek token olarak yutulmasın (virgül URL yolunda kalabilir).
  const plain = text
    .replace(/[<>]/g, " ")
    .replace(/([^\s])(https?:\/\/|magnet:\?)/gi, "$1 $2");

  // Her satırı ayrı değerlendir: bağlantı içermeyen satır "reddedilen" sayılır.
  const lines = plain.split(/\r?\n/);
  const found = [];
  for (const line of lines) {
    const matches = line.match(TOKEN_RE) ?? [];
    if (matches.length === 0) {
      const trimmed = line.trim();
      if (trimmed && trimmed.length > 0) rejected.push(trimmed.slice(0, 160));
      continue;
    }
    // Satırın geri kalanı (bağlantı dışı metin) bilgilendirme amaçlı atılır.
    for (const match of matches) found.push(match);
  }

  for (const raw of found) {
    const link = trimLink(raw);
    if (!isSupportedLink(link)) {
      rejected.push(raw.slice(0, 160));
      continue;
    }
    if (seen.has(link)) {
      duplicates += 1;
      continue;
    }
    if (links.length >= MAX_LINKS) {
      truncated += 1;
      continue;
    }
    seen.add(link);
    links.push(link);
  }

  // Hiç bağlantı bulunamadıysa ve metin tek satırsa, kullanıcı şemasız adres
  // yazmış olabilir: reddedilenlerde görünsün diye dokunmuyoruz.
  return { links, rejected, duplicates, truncated };
}

/** Metinde desteklenmeyen bir şema var mı? (ör. `javascript:`, `file:`) */
export function hasUnsupportedScheme(text) {
  if (typeof text !== "string") return false;
  for (const raw of text.split(/[\s,]+/)) {
    const token = trimLink(raw);
    if (!token || !BAD_SCHEME_RE.test(token)) continue;
    if (!isSupportedLink(token)) return true;
  }
  return false;
}
