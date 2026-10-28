// Guvenlik kontrolu: repoda sir/kisisel yol taramasi + yol korumasi regresyonu.
//
// Kullanim: `node tools/security-check.mjs`
//
// Yaptigi isler:
//   1. Izlenen dosyalarda token/parola/anahtar ve kisisel mutlak yol arar.
//   2. `src-tauri/src/web.rs` icinde yol korumasinin (resolve_static_path +
//      needs_token + origin_guard) durdugunu dogrular; biri silinirse kirmizi.
//   3. Tehlikeli desenleri raporlar (innerHTML'e dis veri, Command::new ile
//      kabuk yorumlamasi, sabit karsilastirmali token).
//
// Cikis kodu 0 = temiz, 1 = bulgu var.

import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { promisify } from "node:util";

const run = promisify(execFile);

let findings = 0;
const ok = (message) => console.log(`OK   ${message}`);
const bad = (message) => {
  findings += 1;
  console.error(`FAIL ${message}`);
};

/** Git ile izlenen dosyalar (uretim/cikti klasorleri disarida). */
async function trackedFiles() {
  const { stdout } = await run("git", ["ls-files", "-z"], { maxBuffer: 32 * 1024 * 1024 });
  return stdout.split("\0").filter(Boolean);
}

/** Ikili/uretim dosyalari atla. */
function isTextCandidate(file) {
  if (/(^|\/)(node_modules|target|dist|build|\.git)\//.test(file)) return false;
  return /\.(rs|ts|js|mjs|cjs|json|toml|yml|yaml|md|html|css|ps1|sh)$/.test(file);
}

// 1) Sir ve kisisel yol taramasi ------------------------------------------------

const SECRET_PATTERNS = [
  [/ghp_[A-Za-z0-9]{20,}/, "GitHub kişisel erişim token'ı"],
  [/github_pat_[A-Za-z0-9_]{30,}/, "GitHub fine-grained token"],
  [/sk-[A-Za-z0-9]{32,}/, "API anahtarı (sk-…)"],
  [/AKIA[0-9A-Z]{16}/, "AWS erişim anahtarı"],
  [/-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----/, "özel anahtar"],
  [/(?<![\w-])(password|passwd|secret|api[_-]?key)\s*[:=]\s*["'][^"'\s]{8,}["']/i, "gömülü parola/anahtar"],
];

const PERSONAL_PATHS = [
  [/C:\\\\?Users\\\\?titan/i, "kişisel Windows yolu (C:\\Users\\titan)"],
  [/\/home\/[a-z0-9._-]+\/(Projeler|projects)/i, "kişisel POSIX yolu"],
];

const files = (await trackedFiles()).filter(isTextCandidate);
console.log(`..   ${files.length} metin dosyası taranıyor`);

const allowed = new Set([
  // Test/doküman örnekleri bilinçli olarak kişisel yol içerebilir; burada tutulur.
  "HANDOFF.md",
  "AGENTS.md",
  // Tarayıcının kendisi ve politikası: desen metinleri örnek yol içerir.
  "SECURITY.md",
  "tools/security-check.mjs",
  "README.md",
]);

for (const file of files) {
  let text;
  try {
    text = await readFile(file, "utf8");
  } catch {
    continue;
  }
  for (const [pattern, label] of SECRET_PATTERNS) {
    if (pattern.test(text)) bad(`${file}: ${label}`);
  }
  if (!allowed.has(file)) {
    for (const [pattern, label] of PERSONAL_PATHS) {
      if (pattern.test(text)) bad(`${file}: ${label}`);
    }
  }
}
ok("sır/kişisel yol taraması bitti");

// 2) Yol koruması regresyonu ---------------------------------------------------

const web = await readFile("src-tauri/src/web.rs", "utf8");
const guards = [
  ["fn resolve_static_path", "yol çözümleme (kök dışına çıkışı engeller)"],
  ["fn needs_token", "token kapsamı (varsayılan reddet)"],
  ["fn origin_guard", "Origin kontrolü (localhost CSRF)"],
  ["Component::Normal", "sürücü öneki/UNC reddi"],
  ["canonicalize", "symlink kaçışı kontrolü"],
  ["origin_allowed", "Origin beyaz listesi"],
];
for (const [needle, label] of guards) {
  if (web.includes(needle)) ok(`koruma yerinde: ${label}`);
  else bad(`KORUMA KAYIP: ${label} (${needle})`);
}

// Eski, kırılgan "prefix listesi" yaklaşımı geri gelmemeli.
if (/path\.starts_with\("\/torrents"\)\s*\n?\s*\|\|\s*path\.starts_with/.test(web)) {
  bad("token koruması yine 'izinli prefix listesi' ile yapılıyor (varsayılan reddet olmalı)");
} else {
  ok("token koruması varsayılan-reddet modelinde");
}

// 3) Tehlikeli desenler --------------------------------------------------------

const risky = [
  [/Command::new\(\s*"(cmd|powershell|sh|bash)"/, "kabuk çağrısı (Command::new)"],
];

for (const file of files) {
  if (!/\.(rs|ts|js|mjs)$/.test(file)) continue;
  let text;
  try {
    text = await readFile(file, "utf8");
  } catch {
    continue;
  }
  for (const [pattern, label] of risky) {
    if (pattern.test(text)) bad(`${file}: ${label}`);
  }

  // Tarayıcının kendisi desen/rapor metni içerir; kendi üstünde çalışmaz.
  if (file === "tools/security-check.mjs") continue;

  // innerHTML: şablondaki her ${...} ya kaçış fonksiyonundan geçmeli ya da
  // sayı/sabit olmalı. Kaba ama etkili bir kontrol.
  const lines = text.split(/\r?\n/);
  lines.forEach((line, index) => {
    if (!line.includes("innerHTML")) return;
    const safe = /\$\{([^}]*)\}/g;
    let match;
    while ((match = safe.exec(line)) !== null) {
      const expr = match[1].trim();
      if (/^(escapeHtml|escapeAttr|shortenUrl|formatBytes|formatEta|Math\.|Number\(|pct|status|kind|index|i|count|files\.length|item\.(status|kind|mime)|ICONS\[)/.test(expr)) {
        continue;
      }
      // İçinde kaçış çağrısı geçiyorsa güvenli say.
      if (/escapeHtml|escapeAttr|shortenUrl/.test(expr)) continue;
      // Salt sayısal/sabit ifadeler.
      if (/^[\d\s.+\-*/%()]+$/.test(expr)) continue;
      bad(`${file}:${index + 1}: innerHTML içinde kaçışsız ifade -> \${${expr.slice(0, 60)}}`);
    }
  });
}
ok("tehlikeli desen taraması bitti");

console.log(findings === 0 ? "\nSONUC: PASS" : `\nSONUC: FAIL (${findings} bulgu)`);
process.exit(findings === 0 ? 0 : 1);
