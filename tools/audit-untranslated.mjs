// main.ts içinde çevrilmemiş dinamik metinleri bulur (kaba tarama).
// Amaç: yerelleştirme sırasında gözden kaçan `textContent = "..."` atamalarını
// ve şablon literallerini listelemek. Kendisi bir test değil, denetim aracı.
import fs from "node:fs";

const file = "src/main.ts";
const lines = fs.readFileSync(file, "utf8").split(/\r?\n/);
const TR = /[çğıöşüÇĞİÖŞÜ]/;

let hits = 0;
lines.forEach((line, index) => {
  const trimmed = line.trim();
  if (trimmed.startsWith("//") || trimmed.startsWith("*")) return;
  // Zaten çevrilmiş satırları atla
  if (/\bt\(|\btt`/.test(line)) return;
  if (!TR.test(line)) return;
  // Yalnız atama/şablon satırları ilgi çekici
  if (!/(textContent|innerHTML|toast\(|title\s*=|hint\s*=|=\s*`|\.value\s*=)/.test(line)) return;
  hits += 1;
  console.log(`${index + 1}: ${trimmed.slice(0, 110)}`);
});
console.log(`\nÇevrilmemiş olabilecek ${hits} satır`);
