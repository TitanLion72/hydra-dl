// Eklenti tarayıcı uyumluluğu testleri (ağ/tarayıcı gerekmez).
//
// Kullanım: node tools/test-extension.mjs
// Neyi korur:
//  1. compat.js doğru API'yi seçer (Chrome / Edge / Firefox / API yok).
//  2. Paketlenmiş manifestler Chrome ve Firefox için doğru biçimde.
//  3. compat.js her bağlamda ilk yüklenir (background, content script, HTML).
//  4. Doğrudan `chrome.*` çağrısı yok (compat.js dışında).
import assert from "node:assert/strict";
import { readdir, readFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ext = path.join(root, "extension");
let passed = 0;
let failed = 0;

function check(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`OK   ${name}`);
  } catch (err) {
    failed += 1;
    console.log(`FAIL ${name}\n     ${err.message}`);
  }
}

async function checkAsync(name, fn) {
  try {
    await fn();
    passed += 1;
    console.log(`OK   ${name}`);
  } catch (err) {
    failed += 1;
    console.log(`FAIL ${name}\n     ${err.message}`);
  }
}

async function readJson(file) {
  return JSON.parse(await readFile(file, "utf8"));
}

async function readAll(dir) {
  const out = new Map();
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      for (const [k, v] of await readAll(full)) out.set(k, v);
    } else {
      out.set(path.relative(dir, full).split(path.sep).join("/"), await readFile(full, "utf8"));
    }
  }
  return out;
}

const compatSrc = await readFile(path.join(ext, "compat.js"), "utf8");
const files = await readAll(ext);

function loadCompat(globals) {
  const ctx = vm.createContext(globals);
  vm.runInContext(compatSrc, ctx);
  return ctx.HX;
}

// ---------------------------------------------------------------- compat.js

check("Chrome: api = chrome.*, isFirefox = false", () => {
  const chrome = { runtime: { id: "x" } };
  const hx = loadCompat({ chrome });
  assert.equal(hx.api, chrome);
  assert.equal(hx.isFirefox, false);
});

check("Edge (browser + chrome): Firefox sanmaz", () => {
  // Chromium 121+ `browser` global'ini de tanımlar; eski ölçüt bu yüzden yanlıştı.
  const chrome = { runtime: { id: "x" } };
  const browser = { runtime: { id: "x" } };
  const hx = loadCompat({ chrome, browser });
  assert.equal(hx.api, chrome);
  assert.equal(hx.isFirefox, false);
});

check("Firefox: api = browser.*, isFirefox = true", () => {
  const chrome = { runtime: { id: "x" } };
  const browser = { runtime: { id: "x", getBrowserInfo: () => ({ name: "Firefox" }) } };
  const hx = loadCompat({ chrome, browser });
  assert.equal(hx.api, browser);
  assert.equal(hx.isFirefox, true);
});

check("API yoksa HX tanımlanmaz", () => {
  assert.equal(loadCompat({}), undefined);
});

// ---------------------------------------------------------------- manifest

const manifest = await readJson(path.join(ext, "manifest.json"));
check("kaynak manifest MV3 ve sürüm 0.2.x", () => {
  assert.equal(manifest.manifest_version, 3);
  assert.match(manifest.version, /^0\.2\./);
});

check("kaynak manifest: compat.js content script'lerin ilk elemanı", () => {
  assert.equal(manifest.content_scripts[0].js[0], "compat.js");
});

check("kaynak manifest: optional_host_permissions var (Firefox izni)", () => {
  assert.ok(Array.isArray(manifest.optional_host_permissions));
  assert.ok(manifest.optional_host_permissions.includes("http://127.0.0.1/*"));
});

// ---------------------------------------------------------------- bağlam sırası

check("background.js compat.js'i önce yükler", () => {
  assert.match(files.get("background.js"), /importScripts\(\s*"compat\.js"/);
});

for (const html of ["popup.html", "options.html", "panel.html"]) {
  check(`${html}: ilk <script src> compat.js`, () => {
    const srcs = [...files.get(html).matchAll(/<script[^>]*src="([^"]+)"/g)].map((m) => m[1]);
    assert.ok(srcs.length > 0, "script yok");
    assert.equal(srcs[0], "compat.js", `ilk script: ${srcs[0]}`);
  });
}

// ---------------------------------------------------------------- doğrudan chrome.*

check("compat.js dışında doğrudan chrome.* çağrısı yok", () => {
  const offenders = [];
  for (const [name, source] of files) {
    if (!name.endsWith(".js") || name === "compat.js") continue;
    for (const line of source.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (trimmed.startsWith("//")) continue;
      // `chrome.` ile başlayan gerçek çağrı (yorum/açıklama metni değil).
      if (/(^|[^.\w])chrome\.[a-zA-Z]+\.[a-zA-Z]+\s*\(/.test(trimmed)) {
        offenders.push(`${name}: ${trimmed.slice(0, 70)}`);
      }
    }
  }
  assert.deepEqual(offenders, []);
});

// ---------------------------------------------------------------- paketler

const chromeManifestPath = path.join(root, "build/extensions/chrome/manifest.json");
const firefoxManifestPath = path.join(root, "build/extensions/firefox/manifest.json");

for (const [label, file] of [
  ["chrome", chromeManifestPath],
  ["firefox", firefoxManifestPath],
]) {
  await checkAsync(`${label} paketi üretilmiş (npm run build:ext)`, async () => {
    const info = await stat(file);
    assert.ok(info.isFile(), `${label} manifest yok: çalıştır → node tools/build-extension.mjs`);
  });
}

if (await stat(chromeManifestPath).then(() => true, () => false)) {
  const chromeManifest = await readJson(chromeManifestPath);
  check("chrome paketi service_worker kullanır", () => {
    assert.equal(chromeManifest.background.service_worker, "background.js");
    assert.equal(chromeManifest.background.scripts, undefined);
  });
  check("chrome paketinde gecko ayarı yok", () => {
    assert.equal(chromeManifest.browser_specific_settings, undefined);
  });
}

if (await stat(firefoxManifestPath).then(() => true, () => false)) {
  const ffManifest = await readJson(firefoxManifestPath);
  check("firefox paketi background.scripts kullanır", () => {
    assert.ok(Array.isArray(ffManifest.background.scripts));
    assert.equal(ffManifest.background.scripts[0], "compat.js");
    assert.equal(ffManifest.background.service_worker, undefined);
  });
  check("firefox paketi gecko kimliği ve min sürüm içerir", () => {
    const gecko = ffManifest.browser_specific_settings?.gecko;
    assert.ok(gecko, "browser_specific_settings.gecko yok");
    assert.ok(gecko.id && gecko.id.includes("@"), `gecko.id geçersiz: ${gecko.id}`);
    const major = Number.parseInt(String(gecko.strict_min_version).split(".")[0], 10);
    assert.ok(major >= 115, `strict_min_version: ${gecko.strict_min_version}`);
  });
  await checkAsync("firefox paketindeki tüm dosyalar mevcut", async () => {
    const dir = path.dirname(firefoxManifestPath);
    const missing = [];
    const must = new Set(["manifest.json"]);
    for (const cs of ffManifest.content_scripts ?? []) {
      for (const f of [...(cs.js ?? []), ...(cs.css ?? [])]) must.add(f);
    }
    for (const f of ffManifest.background.scripts) must.add(f);
    if (ffManifest.action?.default_popup) must.add(ffManifest.action.default_popup);
    if (ffManifest.options_page) must.add(ffManifest.options_page);
    for (const f of Object.values(ffManifest.icons ?? {})) must.add(f);
    for (const f of Object.values(ffManifest.action?.default_icon ?? {})) must.add(f);
    for (const f of must) {
      const ok = await stat(path.join(dir, f)).then(() => true, () => false);
      if (!ok) missing.push(f);
    }
    assert.deepEqual(missing, []);
  });
}

// ---------------------------------------------------------------- rapor

console.log(`\nSONUC: ${failed === 0 ? "PASS" : "FAIL"} (${passed} geçti, ${failed} kaldı)`);
process.exit(failed === 0 ? 0 : 1);
