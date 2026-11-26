// Tek komut yayin araci: surumu artir, kontrolleri kos, imzali NSIS paketini uret,
// `latest.json` manifestini yaz, commit + tag + push yap ve GitHub release'ini olustur.
//
// Kullanim:
//   node tools/release.mjs 0.9.0 --dry-run            # hicbir seyi degistirmez, plani basar
//   node tools/release.mjs 0.9.0 --notes "..."        # gercek yayin
//   node tools/release.mjs 0.9.0 --key <anahtar.dosyasi>
//
// Anahtar varsayilan olarak `%USERPROFILE%\.tauri\hydra-dl-updater.key` dosyasindan
// okunur; parola **yalniz** terminalden gizli sorulur (bayrak/env ile verilmez).
//
// Tauri 2 sozlesmesi: `TAURI_SIGNING_PRIVATE_KEY` anahtar dosyasinin **icerigini**,
// `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` parolayi alir. v1'deki `..._PATH` kullanilmaz.
import { spawn } from "node:child_process";
import { readFile, writeFile, access } from "node:fs/promises";
import { homedir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const NPM = process.platform === "win32" ? "npm.cmd" : "npm";
/** Release dosyalarinin indirilecegi adres (depo: TitanLion72/hydra-dl). */
const REPO = "TitanLion72/hydra-dl";
const RELEASE_BASE = `https://github.com/${REPO}/releases/download`;

function arg(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

function flag(name) {
  return process.argv.includes(`--${name}`);
}

/** Ilk bayraksiz arguman (surum numarasi). */
function firstPositional() {
  const valueFlags = new Set(["--key", "--notes"]);
  for (let i = 2; i < process.argv.length; i += 1) {
    const token = process.argv[i];
    if (valueFlags.has(token)) {
      i += 1;
      continue;
    }
    if (!token.startsWith("--")) return token;
  }
  return undefined;
}

/** Surumun yazildigi uc dosya: okuma deseni + yazma islemi. */
const VERSION_FILES = [
  {
    file: "package.json",
    pattern: /"version"\s*:\s*"(\d+\.\d+\.\d+)"/,
    write: (text, next) => text.replace(/("version"\s*:\s*")\d+\.\d+\.\d+(")/, `$1${next}$2`),
  },
  {
    file: "src-tauri/tauri.conf.json",
    pattern: /"version"\s*:\s*"(\d+\.\d+\.\d+)"/,
    write: (text, next) => text.replace(/("version"\s*:\s*")\d+\.\d+\.\d+(")/, `$1${next}$2`),
  },
  {
    file: "src-tauri/Cargo.toml",
    pattern: /^version\s*=\s*"(\d+\.\d+\.\d+)"/m,
    write: (text, next) => text.replace(/^version\s*=\s*"\d+\.\d+\.\d+"/m, `version = "${next}"`),
  },
];

/** `x.y.z` -> karsilastirilabilir sayi uclusu. */
function parseVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(value ?? "");
  if (!match) return null;
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function isGreater(next, current) {
  for (let i = 0; i < 3; i += 1) {
    if (next[i] > current[i]) return true;
    if (next[i] < current[i]) return false;
  }
  return false;
}

/** Komutu canli ciktiyle kosar; sifir olmayan cikista hata firlatir. */
function runStream(command, args, env) {
  return new Promise((resolve, reject) => {
    // Windows'ta `.cmd`/`.bat` (npm.cmd) yalniz kabuk uzerinden calisir; `shell:false`
    // EINVAL verir. Kabuk yalniz yerel, sabit komut adlari icin acilir; kullanici
    // girdisi arguman olarak gecmez (bkz. AGENTS: "kabuk yok" kurali dis surecler icin).
    const needsShell = process.platform === "win32" && /\.(cmd|bat)$/i.test(command);
    const child = spawn(command, args, {
      cwd: ROOT,
      env: { ...process.env, ...(env ?? {}) },
      stdio: "inherit",
      shell: needsShell,
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`komut basarisiz (${code}): ${command} ${args.join(" ")}`));
    });
  });
}

/** Komutu sessizce kosup ciktisini dondurur (git/gh gibi kisa komutlar). */
function runCapture(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) resolve(stdout.trim());
      else reject(new Error((stderr.trim() || stdout.trim() || `cikis kodu ${code}`).slice(0, 400)));
    });
  });
}

async function exists(target) {
  try {
    await access(target);
    return true;
  } catch {
    return false;
  }
}

/**
 * Parolayi terminalden **gizli** sorar: tuslar yankilanmaz, deger hicbir yere yazilmaz.
 * Bayrak veya ortam degiskeni kabul edilmez (kabuk gecmisi/log sizintisi olmasin).
 */
function askPassword(question) {
  if (!process.stdin.isTTY) {
    return Promise.reject(new Error("parola terminalden sorulmali (stdin bir TTY degil)"));
  }
  return new Promise((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout, terminal: true });
    // Terminal kendi yankisini yapar; biz yalniz satir sonunu geciririz.
    rl._writeToOutput = (text) => {
      if (text.includes("\n") || text.includes("\r")) process.stdout.write("\n");
    };
    process.stdout.write(question);
    rl.question("", (answer) => {
      rl.close();
      process.stdout.write("\n");
      resolve(answer);
    });
  });
}

function manifest(version, notes, signature, url) {
  return {
    version,
    notes,
    pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms: {
      "windows-x86_64": { signature, url },
    },
  };
}

/** Surum notlari: `--notes` yoksa son tag'den bu yana git log ozeti. */
async function notesFor(version, previousTag) {
  const given = arg("notes");
  if (given) return given;
  let range = "HEAD";
  if (previousTag) {
    try {
      await runCapture("git", ["rev-parse", "--verify", previousTag]);
      range = `${previousTag}..HEAD`;
    } catch {
      // Tag yoksa tum gecmis ozetlenir.
    }
  }
  const log = await runCapture("git", ["log", "--oneline", "--no-decorate", "-20", range]).catch(
    () => "",
  );
  const lines = log ? log.split(/\r?\n/).filter(Boolean) : [];
  if (lines.length === 0) return `Hydra DL v${version}`;
  return [`Hydra DL v${version}`, "", ...lines.map((line) => `- ${line}`)].join("\n");
}

/**
 * `tauri.conf.json`'a updater yapilandirmasini gecici olarak enjekte eder.
 *
 * Neden: `createUpdaterArtifacts` acikken Tauri `plugins.updater.pubkey` ister;
 * pubkey ise ancak anahtar uretildikten sonra bilinir. Kullaniciyi JSON elle
 * duzenlemeye zorlamak yerine release araci anahtarin `.pub` dosyasindan okur,
 * derleme boyunca yapilandirmayi yazar ve sonunda **aynen geri koyar**.
 * Boylece anahtarsiz gelistirici derlemeleri eskisi gibi calisir.
 */
async function withUpdaterConfig(keyPubPath, run) {
  const configPath = path.join(ROOT, "src-tauri", "tauri.conf.json");
  const original = await readFile(configPath, "utf8");
  const pubkey = (await readFile(keyPubPath, "utf8")).trim();
  if (!pubkey) throw new Error(`public key bos: ${keyPubPath}`);
  const config = JSON.parse(original);
  config.bundle = config.bundle ?? {};
  config.bundle.createUpdaterArtifacts = true;
  config.plugins = config.plugins ?? {};
  config.plugins.updater = {
    pubkey,
    endpoints: [`https://github.com/${REPO}/releases/latest/download/latest.json`],
  };
  await writeFile(configPath, JSON.stringify(config, null, 2) + "\n", "utf8");
  try {
    return await run();
  } finally {
    // Basarisiz derlemede bile yapilandirma geri gelir: anahtarsiz `tauri build`
    // calismaya devam eder.
    await writeFile(configPath, original, "utf8");
  }
}

async function main() {
  const dryRun = flag("dry-run");
  const version = firstPositional();
  const next = parseVersion(version);
  if (!next) {
    console.error("Kullanim: node tools/release.mjs <x.y.z> [--dry-run] [--notes \"...\"] [--key <yol>]");
    process.exit(1);
  }

  // --- 1. Surum dogrulama ----------------------------------------------------
  const sources = [];
  for (const entry of VERSION_FILES) {
    const text = await readFile(path.join(ROOT, entry.file), "utf8");
    const match = entry.pattern.exec(text);
    if (!match) throw new Error(`${entry.file}: surum satiri bulunamadi`);
    sources.push({ ...entry, text, current: match[1] });
  }
  const current = sources[0].current;
  if (current !== sources[1].current || current !== sources[2].current) {
    throw new Error(
      `surumler tutarsiz: ${sources.map((s) => `${s.file}=${s.current}`).join(", ")}`,
    );
  }
  const currentParsed = parseVersion(current);
  if (!isGreater(next, currentParsed)) {
    throw new Error(`yeni surum mevcut surumden buyuk olmali (${current} -> ${version})`);
  }

  const exeName = `Hydra DL_${version}_x64-setup.exe`;
  const bundleDir = path.join(ROOT, "src-tauri", "target", "release", "bundle", "nsis");
  const exePath = path.join(bundleDir, exeName);
  const sigPath = `${exePath}.sig`;
  const manifestPath = path.join(bundleDir, "latest.json");
  const notesPath = path.join(process.env.TEMP ?? homedir(), `hydra-release-notes-${version}.md`);
  const downloadUrl = `${RELEASE_BASE}/v${version}/${encodeURIComponent(exeName)}`;
  const keyPath = path.resolve(arg("key", path.join(homedir(), ".tauri", "hydra-dl-updater.key")));

  console.log(`[release] surum: ${current} -> ${version}${dryRun ? " (KURU KOSU)" : ""}`);

  const notes = await notesFor(version, `v${current}`);
  const updateEndpoint = `https://github.com/${REPO}/releases/latest/download/latest.json`;
  console.log(`[release] guncelleme adresi: ${updateEndpoint}`);
  console.log(`[release] imza anahtari: ${keyPath}${(await exists(keyPath)) ? "" : " (YOK)"}`);

  // --- 3. Kontroller (iki modda da kosar; biri kirmizi ise dur) -------------
  console.log("[release] kontroller: cargo test + npm build + security-check");
  await runStream("cargo", ["test", "--manifest-path", path.join("src-tauri", "Cargo.toml")]);
  await runStream(NPM, ["run", "build"]);
  await runStream(process.execPath, [path.join("tools", "security-check.mjs")]);

  if (dryRun) {
    // Kuru kosuda plan disinda hicbir sey yapilmaz: dosya yazilmaz, aga cikilmaz.
    console.log("\n[kuru kosu] 1) Surumu artirilacak dosyalar:");
    for (const entry of sources) {
      console.log(`  - ${entry.file}: ${entry.current} -> ${version}`);
    }

    // Onceki bir derleme varsa gercek imzayi goster, yoksa yer tutucu.
    const signature = (await exists(sigPath))
      ? (await readFile(sigPath, "utf8")).trim()
      : "<.sig dosyasinin icerigi (derlemeden sonra)>";

    console.log("\n[kuru kosu] 2) Beklenen ciktilar:");
    console.log(`  - exe   : ${exePath}`);
    console.log(`  - imza  : ${sigPath}`);
    console.log(`  - manifest: ${manifestPath}`);

    console.log("\n[kuru kosu] 3) latest.json icerigi:");
    console.log(JSON.stringify(manifest(version, notes, signature, downloadUrl), null, 2));

    console.log("\n[kuru kosu] 4) Calistirilacak yayin komutu:");
    console.log(
      `  gh release create v${version} --title "Hydra DL v${version}" --notes-file "${notesPath}" ` +
        `"${exePath}" "${sigPath}" "${manifestPath}"`,
    );
    console.log(
      `\n[kuru kosu] Once: git commit -am "release: v${version}" && git tag -a v${version} && ` +
        `git push --follow-tags`,
    );
    console.log("\n[kuru kosu] Hicbir dosya degistirilmedi; commit/tag/push/release yapilmadi.");
    return;
  }

  // --- 2. Surum numarasini uc dosyaya yaz -----------------------------------
  for (const entry of sources) {
    const updated = entry.write(entry.text, version);
    if (updated === entry.text) throw new Error(`${entry.file}: surum yazilamadi`);
    await writeFile(path.join(ROOT, entry.file), updated, "utf8");
    console.log(`[release] ${entry.file}: ${entry.current} -> ${version}`);
  }

  // --- 4. Anahtar + parola (yalniz gizli terminal girisinden) ----------------
  if (!(await exists(keyPath))) {
    throw new Error(
      `imza anahtari yok: ${keyPath}\n` +
        "  Once uret: npm.cmd run tauri signer generate -- -w %USERPROFILE%\\.tauri\\hydra-dl-updater.key",
    );
  }
  const keyContents = await readFile(keyPath, "utf8");
  if (!keyContents.trim()) throw new Error(`anahtar dosyasi bos: ${keyPath}`);
  const password = await askPassword("[release] imza anahtari parolasi (gizli): ");
  if (!password) {
    console.warn("[release] UYARI: anahtar parolasiz; parola korumali anahtar onerilir.");
  }

  // --- 5. Imzali paket ------------------------------------------------------
  console.log("[release] derleme: npm run tauri build (imzali)");
  const keyPubPath = `${keyPath}.pub`;
  if (!(await exists(keyPubPath))) {
    throw new Error(`public key bulunamadi: ${keyPubPath} (anahtar uretilirken olusur)`);
  }
  await withUpdaterConfig(keyPubPath, () =>
    runStream(NPM, ["run", "tauri", "build"], {
      TAURI_SIGNING_PRIVATE_KEY: keyContents,
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: password,
    }),
  );
  if (!(await exists(exePath))) throw new Error(`kurulum paketi bulunamadi: ${exePath}`);
  if (!(await exists(sigPath))) {
    throw new Error(
      `imza bulunamadi: ${sigPath} (tauri.conf.json > bundle.createUpdaterArtifacts acik mi?)`,
    );
  }

  // --- 6. latest.json -------------------------------------------------------
  const signature = (await readFile(sigPath, "utf8")).trim();
  const data = manifest(version, notes, signature, downloadUrl);
  await writeFile(manifestPath, `${JSON.stringify(data, null, 2)}\n`, "utf8");
  await writeFile(notesPath, `${notes}\n`, "utf8");
  console.log(`[release] manifest yazildi: ${manifestPath}`);
  console.log(`[release] indirme adresi: ${downloadUrl}`);

  // --- 7. Commit + tag + push ----------------------------------------------
  await runCapture("git", ["add", ...VERSION_FILES.map((entry) => entry.file), "src-tauri/Cargo.lock"]);
  await runCapture("git", ["commit", "-m", `release: v${version}`]);
  await runCapture("git", ["tag", "-a", `v${version}`, "-m", `Hydra DL v${version}`]);
  await runCapture("git", ["push", "--follow-tags"]);
  console.log(`[release] commit + tag v${version} + push tamam`);

  // --- 8. GitHub release ----------------------------------------------------
  const ghCommand = [
    "release",
    "create",
    `v${version}`,
    "--title",
    `Hydra DL v${version}`,
    "--notes-file",
    notesPath,
    exePath,
    sigPath,
    manifestPath,
  ];
  await runStream("gh", ghCommand);
  console.log(`[release] yayin tamam: https://github.com/${REPO}/releases/tag/v${version}`);
}

main().catch((error) => {
  console.error(`[release] HATA: ${error.message}`);
  process.exit(1);
});
