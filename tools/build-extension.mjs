// Eklentiyi tarayıcıya göre paketler.
//
// Tek kaynak: `extension/`. Bu betik iki çıktı üretir:
//   build/extensions/chrome/    (Chrome, Edge, Brave — MV3, service_worker)
//   build/extensions/firefox/   (Firefox — MV3, arka plan betikleri + gecko id)
//
// Kullanım:
//   node tools/build-extension.mjs            # ikisi
//   node tools/build-extension.mjs firefox    # yalnız Firefox
//
// Firefox'u elle "geçici" yüklemek için: about:debugging#/runtime/this-firefox
//   → "Geçici Eklenti Yükle" → dist/extension-firefox/manifest.json
import { cp, mkdir, readdir, rm, stat, writeFile, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.join(root, "extension");
const dist = path.join(root, "build", "extensions");

const FIREFOX_ID = "hydra-dl@titanlion72";
const CONTENT_SCRIPTS = ["compat.js", "hunter.js"];
const BACKGROUND_SCRIPTS = ["compat.js", "shared.js", "background.js"];

async function readManifest() {
  const raw = await readFile(path.join(source, "manifest.json"), "utf8");
  return JSON.parse(raw);
}

function chromeManifest(base) {
  return {
    ...base,
    background: { service_worker: "background.js" },
    content_scripts: [
      {
        matches: ["http://*/*", "https://*/*"],
        js: CONTENT_SCRIPTS,
        css: ["hunter.css"],
        run_at: "document_idle",
        all_frames: true,
      },
    ],
    minimum_chrome_version: "110",
  };
}

function firefoxManifest(base) {
  return {
    ...base,
    // Firefox MV3'te service worker yok: arka plan "event page" olarak betiklerle yüklenir.
    background: { scripts: BACKGROUND_SCRIPTS },
    content_scripts: [
      {
        matches: ["http://*/*", "https://*/*"],
        js: CONTENT_SCRIPTS,
        css: ["hunter.css"],
        run_at: "document_idle",
        all_frames: true,
      },
    ],
    // İmzasız yüklenebilmesi için sabit kimlik gerekir.
    browser_specific_settings: {
      gecko: {
        id: FIREFOX_ID,
        strict_min_version: "115.0",
      },
    },
  };
}

async function copyTree(target) {
  await rm(target, { recursive: true, force: true });
  await mkdir(target, { recursive: true });
  for (const entry of await readdir(source)) {
    if (entry === "manifest.json") continue;
    await cp(path.join(source, entry), path.join(target, entry), { recursive: true });
  }
}

async function build(kind) {
  const base = await readManifest();
  const target = path.join(dist, kind);
  await copyTree(target);
  const manifest = kind === "firefox" ? firefoxManifest(base) : chromeManifest(base);
  await writeFile(
    path.join(target, "manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
    "utf8",
  );
  return { target, manifest };
}

async function verify({ target, manifest }) {
  const problems = [];
  const must = new Set(["manifest.json"]);
  for (const scripts of manifest.content_scripts ?? []) {
    for (const file of [...(scripts.js ?? []), ...(scripts.css ?? [])]) must.add(file);
  }
  for (const file of manifest.background?.scripts ?? []) must.add(file);
  if (manifest.background?.service_worker) must.add(manifest.background.service_worker);
  if (manifest.action?.default_popup) must.add(manifest.action.default_popup);
  if (manifest.options_page) must.add(manifest.options_page);
  for (const sizes of Object.values(manifest.icons ?? {})) must.add(sizes);
  for (const sizes of Object.values(manifest.action?.default_icon ?? {})) must.add(sizes);

  for (const file of must) {
    try {
      const info = await stat(path.join(target, file));
      if (!info.isFile() || info.size === 0) problems.push(`${file}: boş veya dosya değil`);
    } catch {
      problems.push(`${file}: eksik`);
    }
  }
  return problems;
}

async function main() {
  const only = process.argv[2];
  const kinds = only ? [only] : ["chrome", "firefox"];
  for (const kind of kinds) {
    if (!["chrome", "firefox"].includes(kind)) {
      throw new Error(`Bilinmeyen hedef: ${kind} (chrome|firefox)`);
    }
    const result = await build(kind);
    const problems = await verify(result);
    if (problems.length) {
      throw new Error(`${kind}: ${problems.join(", ")}`);
    }
    console.log(`OK  ${path.relative(root, result.target)}`);
  }
}

main().catch((err) => {
  console.error(`HATA: ${err.message}`);
  process.exit(1);
});
