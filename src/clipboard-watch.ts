/* Pano izleme — saf karar mantığı (tek kaynak).
 *
 * Neden ayrı modül: pano okumak gizlilik açısından hassastır ve tarayıcı API'sine
 * bağlıdır. "Ne zaman doldurulur?" sorusu saf fonksiyonlarda durur, test edilir;
 * `main.ts` yalnız zamanlayıcıyı ve DOM'u bağlar.
 *
 * Bilinçli olarak **import yok**: bu modül `node` ile doğrudan (tip sıyırarak)
 * test edilir; `webapp/links.js` bağımlılığı burada tekrar edilmez ki test
 * altyapısı ek bir yük taşımasın.
 */

/** Bağlantının başındaki/sonundaki süsleri temizler (`(`, `"`, `.`, `,` …). */
function trimWrapping(text: string): string {
  let value = text.trim();
  for (;;) {
    const first = value[0];
    if (first && "\"'`([{<".includes(first)) {
      value = value.slice(1).trimStart();
      continue;
    }
    break;
  }
  for (;;) {
    const last = value[value.length - 1];
    if (last && "\"'`)]}>.,;:".includes(last)) {
      value = value.slice(0, -1).trimEnd();
      continue;
    }
    break;
  }
  return value;
}

/** Desteklenen şemalardan biri mi? */
function hasSupportedScheme(value: string): boolean {
  const lower = value.toLowerCase();
  return (
    lower.startsWith("http://") || lower.startsWith("https://") || lower.startsWith("magnet:?")
  );
}

/**
 * Panodaki metinden **tek** bir indirilebilir bağlantı çıkarır.
 *
 * Birden çok bağlantı, açıklama karışmış metin veya bağlantı olmayan içerik
 * `null` döner: pano izleme kullanıcının alanını sürpriz şekilde doldurmaz.
 */
export function pickClipboardLink(text: string): string | null {
  const raw = String(text ?? "").trim();
  if (!raw) return null;
  const candidate = trimWrapping(raw);
  if (!candidate) return null;
  // Tek bağlantıda boşluk olmaz: "indir: https://… hemen" gibi cümleler ve
  // birden çok satır burada elenir.
  if (/\s/.test(candidate)) return null;
  if (!hasSupportedScheme(candidate)) return null;
  return candidate;
}

/**
 * Alan doldurulmalı mı?
 *
 * Kurallar:
 * - yeni bir bağlantı olmalı (aynı bağlantı iki kez doldurulmaz),
 * - alan boş olmalı ya da **bizim** bir önceki otomatik doldurmamızı içermeli
 *   (kullanıcının yazdığı metin asla ezilmez).
 */
export function shouldFillLink(
  picked: string | null,
  lastSeen: string | null,
  fieldValue: string,
): boolean {
  if (!picked) return false;
  if (picked === lastSeen) return false;
  const current = String(fieldValue ?? "").trim();
  if (!current) return true;
  return current === lastSeen;
}
