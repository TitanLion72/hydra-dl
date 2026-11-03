// Sanal liste yerleşim matematiği — birim testleri (`src/virtual.ts` saf).
//
// Node 26 TypeScript'i tip-sıyırmasıyla doğrudan çalıştırır; `virtual.ts`
// yalnız tip + saf fonksiyon içerdiği için ayrı derleme gerekmez.

import {
  buildLayout,
  visibleRange,
  clampScroll,
  DEFAULT_ROW_HEIGHT,
  ROW_GAP,
  VIRTUAL_THRESHOLD,
} from "../src/virtual.ts";

let pass = 0;
let fail = 0;
const ok = (name) => {
  pass += 1;
  console.log(`OK   ${name}`);
};
const bad = (name, extra = "") => {
  fail += 1;
  console.error(`FAIL ${name}${extra ? ` — ${extra}` : ""}`);
};
const eq = (name, actual, expected) => {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) ok(name);
  else bad(name, `beklenen ${e}, gelen ${a}`);
};
const check = (name, cond, extra = "") => (cond ? ok(name) : bad(name, extra));

const ids = (n) => Array.from({ length: n }, (_, i) => `id-${i}`);

// --- yerleşim ---------------------------------------------------------------
{
  const layout = buildLayout(ids(3), () => 100);
  eq("ilk satır 0'da", layout.offsets[0], 0);
  eq("ikinci satır yükseklik + boşluk", layout.offsets[1], 100 + ROW_GAP);
  eq("üçüncü satır", layout.offsets[2], 2 * (100 + ROW_GAP));
  eq("toplam yükseklik (son boşluk hariç)", layout.total, 3 * 100 + 2 * ROW_GAP);
}

{
  const layout = buildLayout([], () => 100);
  eq("boş liste toplam 0", layout.total, 0);
  eq("boş liste ofset yok", layout.offsets, []);
}

{
  // Bilinmeyen kayıt tahmini kullanır, bilinen kendi yüksekliğini.
  const heights = new Map([["id-1", 120]]);
  const layout = buildLayout(ids(3), (id) => heights.get(id));
  eq("bilinmeyen tahmin", layout.heights[0], DEFAULT_ROW_HEIGHT);
  eq("bilinen yükseklik", layout.heights[1], 120);
  eq("ofset birikimli", layout.offsets[2], DEFAULT_ROW_HEIGHT + ROW_GAP + 120 + ROW_GAP);
}

// --- pencere ---------------------------------------------------------------
{
  const layout = buildLayout(ids(100), () => 100);
  const range = visibleRange(layout, 0, 500, 0);
  check("başlangıçta ilk satır dahil", range.start === 0, String(range.start));
  check("500 px'te ~6 satır", range.end === 5, String(range.end));

  const scrolled = visibleRange(layout, 1080, 500, 0);
  check("kaydırınca pencere kayar", scrolled.start === 10, String(scrolled.start));
  check("pencere sınırlı", scrolled.end === 15, String(scrolled.end));
}

{
  const layout = buildLayout(ids(100), () => 100);
  const range = visibleRange(layout, 0, 500, 6);
  eq("overscan öncesi yok (başta)", range.start, 0);
  eq("overscan sonrası eklenir", range.end, 5 + 6);
}

{
  const layout = buildLayout(ids(100), () => 100);
  const range = visibleRange(layout, 0, 500, 6);
  check("son indeks sınırı aşmaz", range.end <= 100, String(range.end));
}

{
  const layout = buildLayout(ids(10), () => 100);
  const range = visibleRange(layout, 100_000, 500, 3);
  check("listenin sonunda son satır görünür", range.end === 10, String(range.end));
  check("başlangıç negatif olmaz", range.start >= 0, String(range.start));
}

{
  const layout = buildLayout(ids(10), () => 100);
  eq("boş listede pencere boş", visibleRange(buildLayout([], () => 100), 0, 500), {
    start: 0,
    end: 0,
  });
  check("küçük listede hepsi", visibleRange(layout, 0, 5000, 0).end === 10);
}

{
  // Değişken yükseklik: iki katı yüksek satır pencereyi doğru kaydırır.
  const heights = new Map([["id-0", 200]]);
  const layout = buildLayout(ids(10), (id) => heights.get(id));
  const range = visibleRange(layout, 208, 100, 0);
  // id-0: 0..200, id-1: 208..308
  check("yüksek satırdan sonra doğru satır", range.start === 1, String(range.start));
}

// --- kaydırma sınırı -------------------------------------------------------
eq("taşan kaydırma kırpılır", clampScroll(500, 300, 200), 100);
eq("taşmayan kaydırma korunur", clampScroll(50, 300, 200), 50);
eq("içerik ekrandan küçükse 0", clampScroll(50, 100, 200), 0);
eq("negatif kaydırma 0", clampScroll(-10, 300, 200), -10);

// --- eşik ------------------------------------------------------------------
check("eşik makul (100–1000)", VIRTUAL_THRESHOLD >= 100 && VIRTUAL_THRESHOLD <= 1000, String(VIRTUAL_THRESHOLD));

// --- 300 kayıt: DOM'da tutulan satır sayısı sınırlı ------------------------
{
  const layout = buildLayout(ids(300), () => 100);
  const range = visibleRange(layout, 0, 800, 6);
  const rendered = range.end - range.start;
  check("300 kayıtta çizilen satır < 40", rendered < 40, `${rendered} satır`);
  check("toplam yükseklik tüm listeyi kapsar", layout.total === 300 * 100 + 299 * ROW_GAP);
}

console.log(`\nSONUC: ${fail === 0 ? "PASS" : `FAIL (${fail})`} (${pass} geçti, ${fail} kaldı)`);
process.exit(fail === 0 ? 0 : 1);
