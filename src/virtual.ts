/* Sanal liste yerleşim matematiği — **saf**, DOM yok.
 *
 * Neden ayrı: 200+ kayıtta tüm satırları DOM'a basmak arayüzü kilitliyor.
 * Yalnız görünen pencereyi çizeriz; konum hesabı burada, testle sabitlenir.
 *
 * Satır yükseklikleri **değişkendir** (hata satırı olan kayıt daha yüksek), bu
 * yüzden sabit yükseklik varsayımı yapmıyoruz: bilinmeyen kayıt için tahmin
 * kullanılır, ölçülen gerçek yükseklik önbelleğe yazılır. Ölçüm bir sonraki
 * karede yerleşimi düzeltir; kullanıcı bunu fark etmez (overscan payı var).
 */

/** Bir kaydın bilinmeyen yüksekliği için tahmin (px). */
export const DEFAULT_ROW_HEIGHT = 79;
/** Satırlar arası boşluk (`styles.css` → `.list { gap }`). */
export const ROW_GAP = 8;
/** Görünür alanın üstünde/altında hazır tutulan satır sayısı. */
export const OVERSCAN = 6;
/** Bu sayıya kadar tüm satırlar normal (flex) akışta çizilir. */
export const VIRTUAL_THRESHOLD = 150;

export interface Layout {
  /** Her kaydın üst kenarı (px), giriş sırasıyla aynı. */
  offsets: number[];
  /** Her kaydın yüksekliği (px). */
  heights: number[];
  /** Listenin toplam yüksekliği (son boşluk hariç). */
  total: number;
}

/**
 * Kayıt listesi için konumları hesaplar.
 *
 * `heightOf` bilinmeyen kayıt için `DEFAULT_ROW_HEIGHT` döner.
 */
export function buildLayout(
  ids: readonly string[],
  heightOf: (id: string) => number | undefined,
  gap: number = ROW_GAP,
): Layout {
  const offsets: number[] = new Array(ids.length);
  const heights: number[] = new Array(ids.length);
  let cursor = 0;
  for (let i = 0; i < ids.length; i += 1) {
    const height = heightOf(ids[i]) ?? DEFAULT_ROW_HEIGHT;
    offsets[i] = cursor;
    heights[i] = height;
    cursor += height + gap;
  }
  // Son satırdan sonra boşluk yok.
  const total = ids.length === 0 ? 0 : Math.max(0, cursor - gap);
  return { offsets, heights, total };
}

export interface Range {
  /** İlk çizilecek indeks (dahil). */
  start: number;
  /** Son çizilecek indeks (hariç). */
  end: number;
}

/**
 * Kaydırma konumuna göre çizilecek pencereyi bulur.
 *
 * `viewport` görünür alanın yüksekliği, `overscan` pay satır sayısıdır.
 * Boş listede `{ start: 0, end: 0 }`.
 */
export function visibleRange(
  layout: Layout,
  scrollTop: number,
  viewport: number,
  overscan: number = OVERSCAN,
): Range {
  const count = layout.offsets.length;
  if (count === 0) return { start: 0, end: 0 };

  const top = Math.max(0, scrollTop);
  const bottom = top + Math.max(0, viewport);

  // İlk görünen: üst kenarı ekranın altına düşen ilk satır.
  let first = count - 1;
  for (let i = 0; i < count; i += 1) {
    const rowBottom = layout.offsets[i] + layout.heights[i];
    if (rowBottom > top) {
      first = i;
      break;
    }
  }

  // Son görünen: üst kenarı ekranın üstünü geçen ilk satır.
  let last = first;
  for (let i = first; i < count; i += 1) {
    if (layout.offsets[i] < bottom) last = i;
    else break;
  }

  const start = Math.max(0, first - overscan);
  const end = Math.min(count, last + 1 + overscan);
  return { start, end };
}

/**
 * Kaydırma sonrası listenin toplam yüksekliği değiştiyse kaydırma konumunu
 * taşan miktar kadar düzeltir (son satırlar silinince boşluğa düşmesin).
 */
export function clampScroll(scrollTop: number, total: number, viewport: number): number {
  const max = Math.max(0, total - Math.max(0, viewport));
  if (scrollTop <= max) return scrollTop;
  return max;
}
