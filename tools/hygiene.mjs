// Depo ve çalışma alanı sağlık kontrolü.
//
// Amaç: "bir şey sessizce şişti mi?" sorusunu tek komutla yanıtlamak.
//   - git: izlenmeyen/eksik dosya, izlenen büyük dosya, .git boyutu
//   - çalışma alanı: target/node_modules/dist gibi üretim klasörleri
//   - geçici klasörler: %TEMP% altındaki hydra-* kalıntıları
//   - disk: boş alan eşiği (derleme `os error 112` ile düşmeden uyar)
//   - sır/kişisel yol taraması: `tools/security-check.mjs` ile aynı kurallar
//
// Kullanım:
//   node tools/hygiene.mjs            → rapor (uyarı varsa çıkış kodu 1)
//   node tools/hygiene.mjs --clean    → geçici klasörleri sil
//
// Bu betik hiçbir şeyi **kendiliğinden** silmez; `--clean` açıkça istenmeli.

import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, statfsSync, statSync, readFileSync } from "node:fs";
import { rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "..");
const clean = process.argv.includes("--clean");

/** Uyarı eşikleri (aşılırsa WARN). */
const LIMITS = {
  targetGb: 25,
  repoMb: 25,
  trackedFileMb: 5,
  freeGb: 15,
  tempGb: 5,
};

const warnings = [];
let checks = 0;

const mb = (bytes) => `${(bytes / 1024 / 1024).toFixed(1)} MB`;
const gb = (bytes) => `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;

function ok(message) {
  checks += 1;
  console.log(`OK   ${message}`);
}

function warn(message, hint) {
  checks += 1;
  warnings.push({ message, hint });
  console.warn(`WARN ${message}${hint ? `\n     → ${hint}` : ""}`);
}

function git(args) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
}

function dirSize(dir) {
  if (!existsSync(dir)) return 0;
  let total = 0;
  const stack = [dir];
  while (stack.length > 0) {
    const current = stack.pop();
    let entries;
    try {
      entries = readdirSync(current, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else if (entry.isFile()) {
        try {
          total += statSync(full).size;
        } catch {
          // kilitli dosya: yoksay
        }
      }
    }
  }
  return total;
}

// ---------------------------------------------------------------- git

function checkGit() {
  let status;
  try {
    status = git(["status", "--porcelain"]);
  } catch (err) {
    warn(`git durumu okunamadı: ${err.message}`);
    return;
  }
  if (status.length === 0) ok("çalışma ağacı temiz");
  else {
    const lines = status.split(/\r?\n/).filter(Boolean);
    warn(
      `${lines.length} değişiklik/izlenmeyen dosya var`,
      lines.slice(0, 5).join(" | "),
    );
  }

  // İzlenen dosyalar arasında büyük olan var mı? (yanlışlıkla commit'lenen medya)
  const tracked = git(["ls-files"]).split(/\r?\n/).filter(Boolean);
  const big = [];
  for (const file of tracked) {
    const full = path.join(root, file);
    try {
      const size = statSync(full).size;
      if (size > LIMITS.trackedFileMb * 1024 * 1024) big.push(`${file} (${mb(size)})`);
    } catch {
      big.push(`${file} (eksik)`);
    }
  }
  if (big.length === 0) ok(`${tracked.length} izlenen dosya, hepsi < ${LIMITS.trackedFileMb} MB`);
  else warn(`izlenen büyük dosya: ${big.join(", ")}`, "binary/medya repoya girmemeli");

  // .git şişmesi (yanlışlıkla eklenen büyük blob geçmişte kalır)
  const gitSize = dirSize(path.join(root, ".git"));
  if (gitSize <= LIMITS.repoMb * 1024 * 1024) ok(`.git boyutu ${mb(gitSize)}`);
  else warn(`.git ${mb(gitSize)} > ${LIMITS.repoMb} MB`, "geçmişte büyük blob olabilir");

  // .gitignore kapsamı: üretim klasörleri izlenmemeli
  const mustIgnore = ["node_modules/", "dist/", "src-tauri/target/"];
  const ignoreFile = path.join(root, ".gitignore");
  const patterns = existsSync(ignoreFile)
    ? readFileSync(ignoreFile, "utf8").split(/\r?\n/).map((line) => line.trim())
    : [];
  const missing = mustIgnore.filter((pattern) => !patterns.includes(pattern));
  if (missing.length === 0) ok(".gitignore üretim klasörlerini kapsıyor");
  else warn(`.gitignore eksik: ${missing.join(", ")}`);
}

// ---------------------------------------------------------------- çalışma alanı

function checkWorkspace() {
  const dirs = [
    ["src-tauri/target", LIMITS.targetGb * 1024 ** 3],
    ["node_modules", 1024 ** 3],
    ["dist", 200 * 1024 ** 2],
    ["tools/ui-harness/node_modules", 500 * 1024 ** 2],
    ["tools/ui-harness/artifacts", 100 * 1024 ** 2],
    ["build", 100 * 1024 ** 2],
  ];
  for (const [rel, limit] of dirs) {
    const full = path.join(root, rel);
    if (!existsSync(full)) {
      ok(`${rel} yok`);
      continue;
    }
    const size = dirSize(full);
    if (size > limit) {
      warn(
        `${rel} ${gb(size)} > eşik ${gb(limit)}`,
        rel === "src-tauri/target"
          ? "`cargo clean` ile temizle; disk dolarsa derleme os error 112 verir"
          : "üretilen çıktı; gerekiyorsa sil",
      );
    } else {
      ok(`${rel} ${gb(size)}`);
    }
  }
}

// ---------------------------------------------------------------- geçici klasörler

function tempCandidates() {
  const temp = os.tmpdir();
  let entries = [];
  try {
    entries = readdirSync(temp, { withFileTypes: true });
  } catch {
    return [];
  }
  return entries
    .filter((entry) => entry.name.startsWith("hydra-"))
    .map((entry) => path.join(temp, entry.name));
}

async function checkTemp() {
  const candidates = tempCandidates();
  if (candidates.length === 0) {
    ok("geçici hydra-* kalıntısı yok");
    return;
  }
  let total = 0;
  const sizes = [];
  for (const dir of candidates) {
    const stat = statSync(dir);
    // Dosya da olabilir (ör. log)
    const size = stat.isDirectory() ? dirSize(dir) : stat.size;
    total += size;
    sizes.push({ name: path.basename(dir), size });
  }
  if (total > LIMITS.tempGb * 1024 ** 3) {
    warn(
      `geçici klasörler ${gb(total)} > eşik ${LIMITS.tempGb} GB`,
      clean ? "temizleniyor…" : "`node tools/hygiene.mjs --clean` ile sil",
    );
  } else {
    ok(`geçici hydra-* ${gb(total)} (${candidates.length} öğe)`);
  }
  // En büyük 5 kalıntı: gürültü yapmadan ipucu ver.
  sizes.sort((a, b) => b.size - a.size);
  const top = sizes.slice(0, 5).map((item) => `${item.name} ${mb(item.size)}`);
  if (top.length > 0) console.log(`..   en büyük: ${top.join(" · ")}`);
  if (clean) {
    for (const dir of candidates) {
      // Canlı test sunucusu gibi çalışan süreçlerin dosyaları kilitli olabilir.
      await rm(dir, { recursive: true, force: true }).catch(() => {});
    }
    console.log("..   geçici hydra-* klasörleri silindi");
  }
}

// ---------------------------------------------------------------- disk

function checkDisk() {
  // Windows'ta sürücü kökü; POSIX'te "/".
  const mount = process.platform === "win32" ? path.parse(root).root : "/";
  try {
    const stats = statfsSync(mount);
    const free = stats.bavail * stats.bsize;
    if (free < LIMITS.freeGb * 1024 ** 3) {
      warn(
        `boş alan ${gb(free)} < ${LIMITS.freeGb} GB`,
        "`cargo clean` ve `node tools/hygiene.mjs --clean` çalıştır",
      );
    } else {
      ok(`boş alan ${gb(free)}`);
    }
  } catch (err) {
    warn(`disk bilgisi okunamadı: ${err.message}`);
  }
}

// ---------------------------------------------------------------- güvenlik

function checkSecurity() {
  try {
    const out = execFileSync(process.execPath, [path.join(here, "security-check.mjs")], {
      cwd: root,
      encoding: "utf8",
    });
    if (/SONUC: PASS/.test(out)) ok("güvenlik taraması (sır/yol/XSS) PASS");
    else warn("güvenlik taraması PASS değil", "`node tools/security-check.mjs` çıktısına bak");
  } catch {
    warn("güvenlik taraması başarısız", "`node tools/security-check.mjs` çıktısına bak");
  }
}

// ---------------------------------------------------------------- main

console.log("Hydra DL — depo/çalışma alanı sağlık kontrolü\n");
checkGit();
checkWorkspace();
await checkTemp();
checkDisk();
checkSecurity();

if (warnings.length === 0) {
  console.log(`\nSONUC: PASS (${checks} kontrol)`);
  process.exit(0);
}
console.warn(`\nSONUC: WARN (${warnings.length} uyarı / ${checks} kontrol)`);
process.exit(1);
