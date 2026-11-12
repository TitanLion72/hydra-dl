// i18n motoru birim testleri + envanter kapsama denetimi.
//
// Kural: sözlük **tam** olmalı. Eksik anahtar olursa İngilizce modda arayüz
// yarı Türkçe kalır; bu test onu merge öncesi yakalar.

import {
  t,
  tt,
  setLanguage,
  getLanguage,
  normalizeLanguage,
  hasTranslation,
  dictionary,
  localizeDom,
} from "../webapp/i18n.js";

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

// --- dil seçimi -------------------------------------------------------------
eq("varsayılan dil tr", getLanguage(), "tr");
eq("en seçilir", setLanguage("en"), "en");
eq("geçersiz dil tr'ye döner", setLanguage("klingon"), "tr");
eq("en-US → en", normalizeLanguage("en-US"), "en");
eq("en_GB → en", normalizeLanguage("en_GB"), "en");
eq("boş → tr", normalizeLanguage(""), "tr");
eq("null → tr", normalizeLanguage(null), "tr");

// --- temel çeviri -----------------------------------------------------------
setLanguage("tr");
eq("TR'de metin aynen kalır", t("Ekle"), "Ekle");
eq("TR'de bilinmeyen metin aynen", t("Bilinmeyen metin"), "Bilinmeyen metin");

setLanguage("en");
eq("EN'de çevrilir", t("Ekle"), "Add");
eq("boş anahtar güvenli", t(""), "");
eq("null anahtar güvenli", t(null), "");

// --- yer tutucular ----------------------------------------------------------
eq(
  "yer tutucu doldurulur",
  t("%1 bağlantı hazır", [3]),
  "3 links ready",
);
eq(
  "iki yer tutucu",
  t("Yeni sürüm: %1 (kurulu %2)", ["2026.11.01", "2026.10.26"]),
  "New version: 2026.11.01 (installed 2026.10.26)",
);
eq(
  "eksik değer yer tutucuyu bozmaz",
  t("%1 bağlantı hazır", []),
  "%1 links ready",
);
eq(
  "literal yüzde (%%) korunur",
  // `%%` sözlük **değerinde** literal yüzde demek (printf kuralı).
  t("İndiriliyor… %1 (%2 / %3)", ["37%", "1 MB", "2 MB"]),
  "Downloading… 37% (1 MB / 2 MB)",
);

// --- şablon (tt) ------------------------------------------------------------
{
  const count = 7;
  const size = "12 MB";
  eq("tt şablonu çevirir", tt`${count} dosya · ${size} (tümü)`, "7 files · 12 MB (all)");
  setLanguage("tr");
  eq("tt TR'de aynen", tt`${count} dosya · ${size} (tümü)`, "7 dosya · 12 MB (tümü)");
  setLanguage("en");
}

// --- sözlük sorgusu ---------------------------------------------------------
check("çeviri var", hasTranslation("Ekle", "en"));
check("çeviri yok", !hasTranslation("Bu metin yok", "en"));
check("TR sözlüğü boş", Object.keys(dictionary("tr")).length === 0);
check("EN sözlüğü dolu", Object.keys(dictionary("en")).length > 250);

// --- envanter kapsaması -----------------------------------------------------
// Sözlük tam olmalı: `webapp/i18n-en.js` her Türkçe metni içermeli.
const { EN } = await import("../webapp/i18n-en.js");
const inventory = JSON.parse(
  await (await import("node:fs/promises")).readFile(
    new URL("../tools/i18n-inventory.json", import.meta.url),
    "utf8",
  ),
);

/** `${...}` → `%1`, `%2` … */
function normalize(text) {
  let index = 0;
  return text.replace(/\$\{[^}]*\}/g, () => `%${++index}`);
}
function decodeEntities(text) {
  return text
    .replace(/&#10;/g, "\n")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
}
const translatable = inventory.filter((key) => !/[<>]/.test(key) && key.trim().length >= 2);
const missing = translatable.filter((raw) => {
  const candidates = [raw, decodeEntities(raw), normalize(decodeEntities(raw))];
  return !candidates.some((c) => Object.prototype.hasOwnProperty.call(EN, c));
});
eq("envanterdeki her metin sözlükte var", missing, []);

// --- boş sözlük / bilinmeyen dil güvenli ------------------------------------
setLanguage("tr");
eq("TR modda çeviri yok", t("Ekle"), "Ekle");

// --- DOM yerelleştirme (jsdom yok; sahte kök) -------------------------------
check("localizeDom DOM'suz güvenli", localizeDom(null) === undefined);

console.log(`\nSONUC: ${fail === 0 ? "PASS" : `FAIL (${fail})`} (${pass} geçti, ${fail} kaldı)`);
process.exit(fail === 0 ? 0 : 1);
