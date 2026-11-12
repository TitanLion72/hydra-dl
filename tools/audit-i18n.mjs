// i18n sözlük denetimi: tekrar + envanter kapsaması (şablon-farkında).
//
// Envanter metinleri koddaki haliyle `${ifade}` içerir; sözlükte `%1`, `%2` …
// biçiminde saklanır. Karşılaştırma öncesi normalize ediyoruz.
import { EN } from "../webapp/i18n-en.js";
import fs from "node:fs";

const source = fs.readFileSync(new URL("../webapp/i18n-en.js", import.meta.url), "utf8");
const seen = new Set();
const duplicate = [];
for (const line of source.split(/\r?\n/)) {
  const m = line.match(/^\s*"((?:[^"\\]|\\.)*)"\s*:/);
  if (!m) continue;
  const key = JSON.parse(`"${m[1]}"`);
  if (seen.has(key)) duplicate.push(key);
  seen.add(key);
}

/** `${...}` yer tutucularını sırayla `%1`, `%2` … yapar. */
function normalizePlaceholders(text) {
  let index = 0;
  return text.replace(/\$\{[^}]*\}/g, () => `%${++index}`);
}

/** HTML entity'lerini çözer (getAttribute da çözer). */
function decodeEntities(text) {
  return text
    .replace(/&#10;/g, "\n")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
}

// Envanter: depodaki dondurulmus liste (testlerle ayni kaynak). Yoksa %TEMP%
// kopyasina dus (elle envanter uretirken).
const inventoryPath = fs.existsSync(new URL("./i18n-inventory.json", import.meta.url))
  ? new URL("./i18n-inventory.json", import.meta.url)
  : `${process.env.TEMP}/hydra-i18n-inventory.json`;
const inventory = JSON.parse(fs.readFileSync(inventoryPath, "utf8"));

// Çevrilmesi anlamsız girdiler: HTML parçası (etiket içeren) ve tek karakter.
const isTranslatable = (key) => !/[<>]/.test(key) && key.trim().length >= 2;

const missing = [];
let checked = 0;
for (const raw of inventory) {
  if (!isTranslatable(raw)) continue;
  checked += 1;
  const candidates = [raw, decodeEntities(raw), normalizePlaceholders(decodeEntities(raw))];
  if (!candidates.some((c) => Object.prototype.hasOwnProperty.call(EN, c))) missing.push(raw);
}

console.log(`Sözlükte ${Object.keys(EN).length} anahtar · envanter ${inventory.length} metin`);
console.log(`Denetlenen: ${checked} · tekrar: ${duplicate.length} · eksik: ${missing.length}`);
for (const key of duplicate) console.log(`  DUP   ${JSON.stringify(key)}`);
for (const key of missing) console.log(`  EKSIK ${JSON.stringify(key)}`);

// `tt`...`` şablonlarının sözlükte karşılığı var mı? Yoksa İngilizce modda o
// metin Türkçe kalır — sessiz bozukluk. Bu yüzden ayrıca denetliyoruz.
function templateKeysOf(file) {
  // Yorumları ayıkla: yorum içindeki `tt` geçişi yanlış eşleşme üretiyor.
  const source = fs
    .readFileSync(file, "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "");
  const keys = new Set();
  for (const match of source.matchAll(/(^|[^A-Za-z0-9_$])tt`([^`]*)`/g)) {
    let key = "";
    const parts = match[2].split(/\$\{[^}]*\}/);
    parts.forEach((part, index) => {
      key += part;
      if (index < parts.length - 1) key += `%${index + 1}`;
    });
    keys.add(key);
  }
  return [...keys];
}

const templateKeys = [...templateKeysOf("src/main.ts"), ...templateKeysOf("webapp/app.js")];
const missingTemplates = templateKeys.filter(
  (key) => !Object.prototype.hasOwnProperty.call(EN, key),
);

console.log(`Şablon anahtarı: ${templateKeys.length} · eksik: ${missingTemplates.length}`);
for (const key of missingTemplates) console.log(`  EKSIK-TT ${JSON.stringify(key)}`);

process.exit(
  duplicate.length === 0 && missing.length === 0 && missingTemplates.length === 0 ? 0 : 1,
);
