// Güncelleme karar mantığı birim testleri (saf; ağ/GUI yok).
//
// Modül `src/updater-logic.ts`; node tip sıyırması ile doğrudan yüklenir,
// bu yüzden modülde çalışma zamanı import'u bulunmamalı.

import {
  readSkipped,
  writeSkipped,
  readPending,
  writePending,
  clearPending,
  shouldPrompt,
  percent,
  installLooksFailed,
} from "../src/updater-logic.ts";

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
  if (Object.is(actual, expected)) ok(name);
  else bad(name, `beklenen ${JSON.stringify(expected)}, gelen ${JSON.stringify(actual)}`);
};

function memory() {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, v),
    removeItem: (k) => map.delete(k),
  };
}

// --- "Sonra" denilen sürüm --------------------------------------------------
eq("bos hafiza: atlanan yok", readSkipped(memory()), null);
{
  const s = memory();
  writeSkipped(s, "0.8.1");
  eq("atlanan surum yazilir", readSkipped(s), "0.8.1");
}
eq("atlanan surum icin serit yok", shouldPrompt("0.8.1", "0.8.1"), false);
eq("yeni surum icin serit var", shouldPrompt("0.8.2", "0.8.1"), true);
eq("atlanan yoksa serit var", shouldPrompt("0.8.1", null), true);

// --- bekleyen kurulum işareti -----------------------------------------------
{
  const s = memory();
  writePending(s, "0.8.1");
  eq("bekleyen kurulum okunur", readPending(s), "0.8.1");
  clearPending(s);
  eq("bekleyen temizlenir", readPending(s), null);
}
eq("kurulum basarisiz: surum hala eski + isaret", installLooksFailed("0.8.1", "0.8.0"), true);
eq("kurulum basarili: surum eslesti", installLooksFailed("0.8.1", "0.8.1"), false);
eq("isaret yoksa basarisiz sayilmaz", installLooksFailed(null, "0.8.0"), false);

// --- yüzde ------------------------------------------------------------------
eq("yuzde hesaplanir", percent(42, 100), 42);
eq("yuzde tam sayi", percent(1, 3), 33);
eq("toplam yoksa yuzde yok", percent(42, null), null);
eq("toplam sifirsa yuzde yok", percent(0, 0), null);
eq("%100 ustune cikmaz", percent(120, 100), 100);
eq("gecersiz toplam yuzde uretmez", percent(50, Number.NaN), null);
eq("negatif indirme sifira kirpilir", percent(-5, 100), 0);

console.log(`\nSONUC: ${fail === 0 ? "PASS" : `FAIL (${fail})`} (${pass} geçti, ${fail} kaldı)`);
process.exit(fail === 0 ? 0 : 1);
