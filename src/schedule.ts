/* Zamanlanmış indirme penceresi — saf hesap (tek kaynak).
 *
 * Kullanıcı "gece 02:00–06:00 arası indir" der. Kuyruk kapısı bu pencereye bakar:
 * pencere dışında `Queued` işler başlatılmaz (çalışanlar duraklatılmaz — kullanıcı
 * işini yarıda kesmeyiz).
 *
 * `import` yok: `node` ile doğrudan test edilir.
 */

/** `"HH:MM"` → günün dakikası (0..1439). Geçersizse `null`. */
export function parseClock(text: string): number | null {
  const raw = String(text ?? "").trim();
  if (!raw) return null;
  const match = /^(\d{1,2}):(\d{2})$/.exec(raw);
  if (!match) return null;
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  if (!Number.isInteger(hours) || !Number.isInteger(minutes)) return null;
  if (hours > 23 || minutes > 59) return null;
  return hours * 60 + minutes;
}

/**
 * Pencere kullanılabilir mi? İki uç da boşsa pencere yok (= her zaman açık).
 * Tek uç dolu, aynı saat, veya ayrıştırılamayan değer → geçersiz sayılır ve
 * kuyruk kilitlenmesin diye **her zaman açık** davranılır.
 */
export function validWindow(start: string, end: string): boolean {
  const emptyStart = !String(start ?? "").trim();
  const emptyEnd = !String(end ?? "").trim();
  if (emptyStart && emptyEnd) return true;
  if (emptyStart || emptyEnd) return false;
  const from = parseClock(start);
  const to = parseClock(end);
  if (from === null || to === null) return false;
  return from !== to;
}

/**
 * Verilen dakika pencerenin içinde mi? Başlangıç dahil, bitiş hariç.
 * Gece yarısını aşan pencereler (23:00–05:00) desteklenir.
 */
export function inWindow(minuteOfDay: number, start: string, end: string): boolean {
  if (!validWindow(start, end)) return true;
  const emptyStart = !String(start ?? "").trim();
  if (emptyStart) return true;
  const from = parseClock(start)!;
  const to = parseClock(end)!;
  // Yarım dakikalara yuvarla (sistem saatinden gelen ondalık değerler).
  const now = Math.floor(minuteOfDay);
  if (from < to) return now >= from && now < to;
  // Gece yarısını aşan pencere: [from, 1440) ∪ [0, to)
  return now >= from || now < to;
}

/** Arayüzde gösterilecek kısa etiket; pencere yoksa boş metin. */
export function windowLabel(start: string, end: string): string {
  if (!validWindow(start, end)) return "";
  if (!String(start ?? "").trim()) return "";
  return `${String(start).trim()}–${String(end).trim()}`;
}
