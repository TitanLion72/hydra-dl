// Masaüstü arayüzünü Tauri olmadan gerçek tarayıcıda çalıştıran QA koşum takımı.
//
// Yaptığı iş:
//   1. Vite dev sunucusunu (port 1420) alt süreç olarak başlatır.
//   2. Playwright ile `/src/api.ts` isteğini `mock-api.mjs` kaynağıyla karşılar.
//   3. İnceleme paneli, ayarlar çekmecesi ve indirme listesi akışlarını gezer.
//   4. Her adımda `artifacts/*.png` ekran görüntüsü bırakır.
//
// Kullanım: `cd tools\ui-harness; npm.cmd run test`

import { spawn, spawnSync } from "node:child_process";
import { mkdir, readFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..", "..");
const artifactsDir = path.join(here, "artifacts");
// Vite "localhost" adini ::1'e cozebiliyor; 127.0.0.1 ile denetlersek bos yere duser.
const BASE = "http://localhost:1420";
const PROBE_URL = "https://example.com/video.mp4";
const TORRENT_URL =
  "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=arsiv-2026";
const READY_TIMEOUT = 60_000;

let failures = 0;

function ok(message) {
  console.log(`OK   ${message}`);
}

function fail(message) {
  failures += 1;
  console.error(`FAIL ${message}`);
}

function check(condition, message) {
  if (condition) ok(message);
  else fail(message);
  return Boolean(condition);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function serverUp() {
  try {
    const res = await fetch(BASE, { signal: AbortSignal.timeout(2000) });
    return res.status < 500;
  } catch {
    return false;
  }
}

async function startDevServer() {
  const output = [];
  const child = spawn(process.env.ComSpec ?? "cmd.exe", ["/d", "/s", "/c", "npm.cmd run dev -- --port 1420"], {
    cwd: repoRoot,
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const collect = (chunk) => {
    output.push(chunk.toString());
    if (output.length > 200) output.shift();
  };
  child.stdout.on("data", collect);
  child.stderr.on("data", collect);

  const startedAt = Date.now();
  while (Date.now() - startedAt < READY_TIMEOUT) {
    if (child.exitCode !== null) break;
    if (await serverUp()) return { child, output };
    await sleep(500);
  }
  const tail = output.join("").split(/\r?\n/).slice(-20).join("\n");
  throw new Error(`Vite dev sunucusu yanit vermedi (${READY_TIMEOUT / 1000} sn).\n${tail}`);
}

async function launchBrowser() {
  const errors = [];
  for (const channel of ["chrome", "msedge"]) {
    try {
      return { browser: await chromium.launch({ channel, headless: true }), channel };
    } catch (err) {
      errors.push(`${channel}: ${err instanceof Error ? err.message.split("\n")[0] : String(err)}`);
    }
  }
  throw new Error(`Sistem tarayicisi acilamadi.\n${errors.join("\n")}`);
}

async function main() {
  await mkdir(artifactsDir, { recursive: true });

  if (await serverUp()) {
    throw new Error(`${BASE} zaten yanit veriyor; port 1420 dolu. Kosum baska bir sunucuya baglanmasin diye durduruldu.`);
  }

  let child = null;
  let output = [];
  let browser = null;

  try {
    console.log("..   Vite dev sunucusu baslatiliyor (port 1420)");
    const started = await startDevServer();
    child = started.child;
    output = started.output;
    ok(`Vite dev sunucusu hazir (${BASE})`);

    // Vite, yeni bir bağımlılığı ilk gördüğünde ön-derler ve istemciyi
    // "optimized dependencies changed" diyerek **tam sayfa yeniler**. Bu,
    // senaryoların ortasına denk gelirse şerit kaybolur (flaky). Sunucu
    // hazır olur olmaz bir kez ısıtıp ön-derlemenin oturmasını bekliyoruz.
    // Isıtma başarısızsa sessizce geç: senaryolar gerçek davranışı zaten sınar,
    // başarısız bir ısıtmaya "OK" yazmayız.
    let warmed = false;
    try {
      const first = await fetch(`${BASE}/src/main.ts`);
      await sleep(1500);
      const second = await fetch(`${BASE}/src/main.ts`);
      warmed = first.ok && second.ok;
      await sleep(400);
    } catch {
      warmed = false;
    }
    if (warmed) ok("Vite bagimlilik onbellegi isindi");
    else console.warn("[ui-harness] Vite onbellegi isitilamadi (onemsiz)");

    const launched = await launchBrowser();
    browser = launched.browser;
    ok(`Tarayici acildi (channel: ${launched.channel})`);

    const context = await browser.newContext({
      viewport: { width: 1366, height: 900 },
      locale: "tr-TR",
      deviceScaleFactor: 1,
    });
    const page = await context.newPage();

    // Mock durumu sayfa yenilemesinden sağ çıksın: `globalThis` sıfırlanır,
    // bu yüzden updater senaryosunun durumu `localStorage` üzerinden taşınır.
    await page.addInitScript(() => {
      let updater = { available: false };
      try {
        const raw = window.localStorage.getItem("__hydraUpdaterMock");
        if (raw) updater = JSON.parse(raw);
      } catch {
        // localStorage henüz yok (about:blank) — varsayılanla devam
      }
      globalThis.__hydraUpdater = updater;
    });

    const consoleErrors = [];
    page.on("console", (message) => {
      if (message.type() === "error") consoleErrors.push(`console: ${message.text()}`);
    });
    page.on("pageerror", (error) => {
      consoleErrors.push(`pageerror: ${error.message}`);
    });

    // Sahte api kaynağına senaryo gereği küçük bir yama eklenebilir
    // (ör. `SETTINGS.updateCheck = false;`).
    let apiPatch = "";
    const mockSource = await readFile(path.join(here, "mock-api.mjs"), "utf8");
    await page.route("**/src/api.ts*", (route) =>
      route.fulfill({
        status: 200,
        contentType: "text/javascript; charset=utf-8",
        body: mockSource + apiPatch,
      }),
    );
    ok("`/src/api.ts` sahte modul ile karsilandi");

    // Diyalog eklentisi tarayicida `invoke` cagirir ve patlar; testte sahte
    // modulle degistiriyoruz. Donen yolu `window.__hydraDialog` belirler.
    const dialogStub = `
export async function open(options) {
  return globalThis.__hydraDialog?.open ?? null;
}
export async function save(options) {
  return globalThis.__hydraDialog?.save ?? null;
}
export async function message() {}
export async function ask() { return true; }
export async function confirm() { return true; }
`;
    await page.route(/plugin-dialog/, (route) =>
      route.fulfill({
        status: 200,
        contentType: "text/javascript; charset=utf-8",
        body: dialogStub,
      }),
    );
    ok("diyalog eklentisi sahte modulle karsilandi");

    // Güncelleme eklentisi de tarayıcıda yok; davranışı `__hydraUpdater` sürer.
    // `check()` her çağrıldığında `checks` sayacı artar (ayar kapalı senaryosu
    // "hiç çağrılmadı"yı bununla doğrular). `failWith` verilirse indirme
    // reddeder; `closed` sayacı "Sonra"/başarısız kurulumda kaynağın bırakılıp
    // bırakılmadığını gösterir.
    //
    // Adres iki biçimde gelebilir: Vite paketi ön-derlediğinde
    // `/node_modules/.vite/deps/@tauri-apps_plugin-updater.js` (alt çizgi!),
    // ön-derleme yoksa `/node_modules/@tauri-apps/plugin-updater/...`. Bu
    // yüzden desen hem `-` hem `_` kabul eder.
    const updaterStub = `
export async function check() {
  const u = globalThis.__hydraUpdater ?? {};
  u.checks = (u.checks ?? 0) + 1;
  if (!u.available) return null;
  return {
    version: u.version ?? "0.0.0",
    body: u.notes ?? "",
    downloadAndInstall: async (cb) => {
      // Kurulum anındaki temiz kapanış sayacı: sıra doğrulanabilsin
      // (duraklatma indirmeden **önce** yapılmalı).
      u.pauseCallsAtInstall = globalThis.__hydraCalls?.pauseAllForShutdown ?? 0;
      u.installed = (u.installed ?? 0) + 1;
      cb?.({ event: "Started", data: { contentLength: 100 } });
      // Arayüzün arada boyayabilmesi için küçük bir bekleme: gerçek indirme
      // saniyeler sürer, anlık çözülen bir stub yüzde metnini görünmez kılar.
      await new Promise((r) => setTimeout(r, 120));
      cb?.({ event: "Progress", data: { chunkLength: 40 } });
      await new Promise((r) => setTimeout(r, 120));
      if (u.failWith) throw new Error(u.failWith);
      cb?.({ event: "Progress", data: { chunkLength: 60 } });
      cb?.({ event: "Finished" });
    },
    close: async () => { u.closed = (u.closed ?? 0) + 1; },
  };
}
`;
    await page.route(/plugin[-_]updater/, (route) =>
      route.fulfill({
        status: 200,
        contentType: "text/javascript; charset=utf-8",
        body: updaterStub,
      }),
    );
    ok("guncelleme eklentisi sahte modulle karsilandi");

    // `getVersion()` (eklentiyle aynı sürüm kaynağı: `tauri.conf.json`).
    // Gerçek modül tarayıcıda `invoke` çağırıp patlar; mock'un uygulama
    // sürümünü (`APP_INFO.version`) döndürüyoruz ki yarım kurulum senaryosu
    // gerçek davranışı yansıtsın.
    //
    // Desen hem çıplak import yolunu (`@tauri-apps/api/app`) hem de Vite'ın
    // ön-derlenmiş adını (`@tauri-apps_api_app.js`) yakalar.
    await page.route(/tauri-apps[/_]api[/_]app/, (route) =>
      route.fulfill({
        status: 200,
        contentType: "text/javascript; charset=utf-8",
        body: `export async function getVersion() {
  return globalThis.__hydraMock?.appInfo?.version ?? "0.0.0";
}`,
      }),
    );
    ok("`getVersion` sahte modulle karsilandi");

    const shot = async (name) => {
      const file = path.join(artifactsDir, name);
      await page.screenshot({ path: file, fullPage: true });
      const info = await stat(file);
      check(info.size > 15 * 1024, `ekran goruntusu ${name} (${Math.round(info.size / 1024)} KB)`);
    };

    /**
     * Yerleşim geometrisi.
     *
     * `.app` 7 satırlı bir grid; şerit ve seçim çubuğu gizliyken `display:none`
     * oldukları için **grid öğesi değiller**. Satırlar açıkça sabitlenmezse
     * öğeler kayar: `.content` `auto` satıra, `.statusbar` `1fr` satıra düşer
     * ve durum çubuğu ekranın altına yapışmaz. Bu kontrol o kaymayı yakalar.
     */
    const checkLayoutPinned = async (label) => {
      const geom = await page.evaluate(() => {
        const bar = document.querySelector(".statusbar");
        const content = document.querySelector(".content");
        const topbar = document.querySelector(".topbar");
        const viewport = window.innerHeight;
        if (!bar || !content || !topbar) return null;
        return {
          viewport,
          barBottom: Math.round(bar.getBoundingClientRect().bottom * 10) / 10,
          barHeight: Math.round(bar.getBoundingClientRect().height),
          contentHeight: Math.round(content.getBoundingClientRect().height),
          topbarTop: Math.round(topbar.getBoundingClientRect().top * 10) / 10,
        };
      });
      if (!geom) {
        fail(`${label}: yerlesim olculemedi`);
        return;
      }
      check(
        Math.abs(geom.barBottom - geom.viewport) <= 2,
        `${label}: durum cubugu ekranin altina sabit (alt=${geom.barBottom}, ekran=${geom.viewport})`,
      );
      check(
        geom.contentHeight > 200,
        `${label}: liste alani makul yukseklikte (${geom.contentHeight}px)`,
      );
      check(
        geom.barHeight < 120,
        `${label}: durum cubugu sismedi (${geom.barHeight}px)`,
      );
    };

    // --- 1. Ana ekran ------------------------------------------------------
    await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
    await page.waitForFunction(() => Boolean(window.__hydraCalls), null, { timeout: 20_000 });
    await page.waitForSelector("#empty:not([hidden])", { timeout: 20_000 });

    check((await page.title()) === "Hydra DL", "ana ekran basligi 'Hydra DL'");
    check(await page.locator("#empty").isVisible(), "bos durum gorunur");
    check((await page.locator("#empty h2").textContent()).trim() === "Henüz indirme yok", "bos durum metni dogru");
    check((await page.locator("#list .row").count()) === 0, "liste baslangicta bos");
    // Şerit gizliyken (normal durum) yerleşim kaymamalı.
    await checkLayoutPinned("serit gizli");
    await shot("01-empty.png");

    // --- 2. Inceleme paneli ------------------------------------------------
    await page.fill("#url-input", PROBE_URL);
    await page.click("#probe-btn");
    await page.waitForSelector("#probe-sheet:not([hidden])", { timeout: 20_000 });
    await page.waitForSelector("#probe-formats .format-row", { timeout: 20_000 });

    const formatRows = page.locator("#probe-formats .format-row");
    const formatCount = await formatRows.count();
    check(formatCount >= 2, `format satiri sayisi >= 2 (${formatCount})`);
    check(await formatRows.first().locator("input[type=radio]").isChecked(), "ilk format satiri secili");
    const summary = (await page.locator("#probe-summary").textContent()).trim();
    check(summary.length > 0, `ozet metni dolu ('${summary}')`);
    check((await page.locator("#probe-extra .candidates").count()) > 0, "diger kaynaklar bolumu var");
    check((await page.locator("#probe-extra .hint.warn").count()) > 0, "uyari metni gosterildi");
    const probeCall = await page.evaluate(() => window.__hydraCalls.probe[0]);
    check(probeCall?.url === PROBE_URL, "probe cagrisi dogru URL ile gitti");
    await shot("02-probe.png");

    // --- 3. Esc ile panel kapanir ------------------------------------------
    await page.keyboard.press("Escape");
    await page.waitForSelector("#probe-sheet", { state: "hidden", timeout: 10_000 });
    check((await page.locator("#probe-sheet").getAttribute("hidden")) !== null, "Esc ile inceleme paneli kapandi (hidden)");

    // --- 4. Format secimi + api.add ----------------------------------------
    await page.click("#probe-btn");
    await page.waitForSelector("#probe-sheet:not([hidden])", { timeout: 20_000 });
    await page.waitForSelector("#probe-formats .format-row", { timeout: 20_000 });

    const secondRadio = page.locator("#probe-formats .format-row input[value='137']");
    await secondRadio.check();
    check(await secondRadio.isChecked(), "ikinci format satiri secildi (137)");
    await page.click("#probe-download");
    await page.waitForFunction(() => window.__hydraCalls.add.length > 0, null, { timeout: 10_000 });
    const addCall = await page.evaluate(() => window.__hydraCalls.add[0]);
    check(addCall.url === PROBE_URL, `api.add dogru url ile cagrildi (${addCall.url})`);
    check(addCall.format === "137", `api.add dogru format ile cagrildi (${addCall.format})`);
    check(addCall.media === true, "api.add medya bayragi ile cagrildi");
    await page.waitForSelector("#probe-sheet", { state: "hidden", timeout: 10_000 });
    ok("indirme sonrasi inceleme paneli kapandi");

    // --- 4b. Torrent: dosya listesi ve secim -------------------------------
    await page.fill("#url-input", TORRENT_URL);
    await page.click("#probe-btn");
    await page.waitForSelector("#probe-sheet:not([hidden])", { timeout: 20_000 });
    await page.waitForSelector("#probe-files .file-row", { timeout: 20_000 });

    const fileRows = page.locator("#probe-files .file-row");
    check((await fileRows.count()) === 3, `torrent dosya satiri sayisi 3 (${await fileRows.count()})`);
    const fileHead = (await page.locator("#probe-files .files-head").textContent()).replace(/\s+/g, " ").trim();
    check(fileHead.includes("3/3"), `dosya ozeti '3/3' diyor ('${fileHead}')`);
    check((await page.locator("#probe-files input:checked").count()) === 3, "baslangicta tum dosyalar secili");

    // Ortadaki dosyayi kapat -> ozet 2/3 olmali.
    await fileRows.nth(1).locator("input").uncheck();
    const fileHead2 = (await page.locator("#probe-files .files-head").textContent()).replace(/\s+/g, " ").trim();
    check(fileHead2.includes("2/3"), `kismi secim ozeti 2/3 ('${fileHead2}')`);
    check((await page.locator("#probe-files .file-row.on").count()) === 2, "secilmeyen satir isaretsiz");
    await shot("02b-torrent-files.png");

    await page.click("#probe-download");
    await page.waitForFunction(() => window.__hydraCalls.add.length > 1, null, { timeout: 10_000 });
    const torrentAdd = await page.evaluate(() => window.__hydraCalls.add[1]);
    check(
      JSON.stringify(torrentAdd.files) === JSON.stringify([0, 2]),
      `api.add secilen dosyalari gonderdi (${JSON.stringify(torrentAdd.files)})`,
    );

    // "Tumu" dugmesi secimi geri alir.
    await page.waitForSelector("#probe-sheet", { state: "hidden", timeout: 10_000 });
    await page.fill("#url-input", TORRENT_URL);
    await page.click("#probe-btn");
    await page.waitForSelector("#probe-files .file-row", { timeout: 20_000 });
    await page.click("#probe-files [data-files='none']");
    const noneHead = (await page.locator("#probe-files .files-head").textContent()).replace(/\s+/g, " ").trim();
    check(noneHead.includes("0/3"), `'Hicbiri' tumunu kaldirdi ('${noneHead}')`);
    await page.click("#probe-files [data-files='all']");
    check((await page.locator("#probe-files input:checked").count()) === 3, "'Tumu' secimi geri getirdi");
    await page.keyboard.press("Escape");
    await page.waitForSelector("#probe-sheet", { state: "hidden", timeout: 10_000 });

    // --- 4b. Toplu ice aktarma --------------------------------------------
    const beforeBulk = await page.evaluate(() => window.__hydraCalls.add.length);
    await page.click("#bulk-btn");
    await page.waitForSelector("#bulk-sheet:not([hidden])", { timeout: 10_000 });
    ok("toplu pencere acildi");

    await page.fill(
      "#bulk-text",
      [
        "https://ornek.test/a.zip",
        "<https://ornek.test/b.zip>,",
        "https://ornek.test/a.zip", // yinelenen
        "bu satir baglanti degil",
        "magnet:?xt=urn:btih:abc&dn=ornek",
      ].join("\n"),
    );
    await page.waitForFunction(
      () => (document.querySelector("#bulk-summary")?.textContent ?? "").includes("3 bağlantı hazır"),
      null,
      { timeout: 10_000 },
    );
    const bulkSummary = (await page.locator("#bulk-summary").textContent()) ?? "";
    check(bulkSummary.includes("1 yinelenen atlandı"), `yinelenen sayildi ('${bulkSummary.trim()}')`);
    check(await page.locator("#bulk-rejected").isVisible(), "baglanti olmayan satir uyarida gorunuyor");
    await shot("02b-bulk.png");

    await page.click("#bulk-add");
    await page.waitForFunction(
      (before) => window.__hydraCalls.add.length === before + 3,
      beforeBulk,
      { timeout: 30_000 },
    );
    const bulkCalls = await page.evaluate((before) => window.__hydraCalls.add.slice(before), beforeBulk);
    check(
      JSON.stringify(bulkCalls.map((call) => call.url)) ===
        JSON.stringify(["https://ornek.test/a.zip", "https://ornek.test/b.zip", "magnet:?xt=urn:btih:abc&dn=ornek"]),
      `3 baglanti sirayla mock'a gitti (${bulkCalls.map((c) => c.url).join(", ")})`,
    );
    await page.waitForSelector("#bulk-sheet", { state: "hidden", timeout: 10_000 });
    ok("toplu ekleme sonrasi pencere kapandi");
    // --- 5. Ayarlar --------------------------------------------------------
    await page.click("#settings-btn");
    await page.waitForSelector("#settings-drawer:not([hidden])", { timeout: 20_000 });
    await page.waitForFunction(
      () => {
        const input = document.querySelector("#set-dir");
        return input instanceof HTMLInputElement && input.value.trim().length > 0;
      },
      null,
      { timeout: 20_000 },
    );

    const expected = await page.evaluate(() => window.__hydraMock.settings);
    const dirValue = (await page.locator("#set-dir").inputValue()).trim();
    check(dirValue === expected.downloadDir, `#set-dir mock ayariyla dolu (${dirValue})`);
    check((await page.locator("#set-web").isChecked()) === expected.webEnabled, "#set-web mock ayariyla uyumlu");
    check(
      (await page.locator("#set-notify").isChecked()) === expected.notifyOnFinish,
      "#set-notify mock ayariyla uyumlu",
    );
    check(
      (await page.locator("#set-update-check").isChecked()) === true,
      "#set-update-check varsayilan acik",
    );
    check(
      (await page.locator("#set-clipboard-watch").isChecked()) === false,
      "#set-clipboard-watch varsayilan kapali",
    );
    // Masaustu girdisi opacity:0/pointer-events:none; sarmalayan etiket tiklanir.
    await page.locator("label.switch:has(#set-update-check)").click();
    await page.locator("label.switch:has(#set-clipboard-watch)").click();
    check(
      (await page.locator("#set-update-check").isChecked()) === false,
      "#set-update-check etiketle kapatildi",
    );
    check(
      (await page.locator("#set-proxy").inputValue()) === expected.proxyUrl,
      "#set-proxy mock ayariyla uyumlu",
    );
    check(
      (await page.locator("#set-web-tls").isChecked()) === expected.webTls,
      "#set-web-tls mock ayariyla uyumlu (HTTPS kapali)",
    );
    check((await page.locator("#set-web-port").inputValue()) === String(expected.webPort), "#set-web-port mock ayariyla uyumlu");
    check(
      (await page.locator("#set-concurrent").inputValue()) === String(expected.maxConcurrent),
      "#set-concurrent mock ayariyla uyumlu",
    );
    await shot("03-settings.png");

    // --- Dil: seçim anında uygulanır, sayfa yenilenmez ---------------------
    check(
      (await page.locator("#set-language").inputValue()) === expected.language,
      `#set-language mock ayariyla dolu (${expected.language})`,
    );
    const addTextTr = (await page.locator("#add-btn").textContent()) ?? "";
    check(addTextTr.includes("Ekle"), `Turkce modda ekle dugmesi Turkce ('${addTextTr.trim()}')`);
    await page.selectOption("#set-language", "en");
    await page.waitForFunction(
      () => document.documentElement.lang === "en",
      null,
      { timeout: 5000 },
    );
    const addTextEn = (await page.locator("#add-btn").textContent()) ?? "";
    check(addTextEn.includes("Add"), `Ingilizce modda ekle dugmesi cevrildi ('${addTextEn.trim()}')`);
    const placeholderEn = (await page.locator("#url-input").getAttribute("placeholder")) ?? "";
    check(
      placeholderEn.includes("Paste a link"),
      `placeholder cevrildi ('${placeholderEn}')`,
    );
    // Listeyi yeniden yuklemeden geri don: indirmeler korunur.
    const rowsBefore = await page.evaluate(() => window.__hydraCalls.list);
    await page.selectOption("#set-language", "tr");
    await page.waitForFunction(
      () => document.documentElement.lang === "tr",
      null,
      { timeout: 5000 },
    );
    const addTextBack = (await page.locator("#add-btn").textContent()) ?? "";
    check(addTextBack.includes("Ekle"), "Turkce'ye donunce metin geri geldi");
    check(
      (await page.evaluate(() => window.__hydraCalls.list)) === rowsBefore,
      "dil degisimi listeyi yeniden yuklemedi (SSE/indirmeler korunur)",
    );
    ok("dil secimi aninda uygulaniyor");

    await page.click("#save-settings");
    await page.waitForFunction(() => window.__hydraCalls.saveSettings.length > 0, null, { timeout: 10_000 });
    const saved = await page.evaluate(() => window.__hydraCalls.saveSettings[0]);
    check(saved.downloadDir === expected.downloadDir, "kaydetme dogru klasorle mock'a gitti");
    check(saved.webEnabled === expected.webEnabled, "kaydetme dogru web ayariyla mock'a gitti");
    check(saved.notifyOnFinish === expected.notifyOnFinish, "kaydetme bildirim ayarini korudu");
    check(saved.updateCheck === false, "kaydetme guncelleme kontrolunu kapali gonderdi");
    check(saved.clipboardWatch === true, "kaydetme pano izlemeyi acti");
    check(saved.torrentEnabled === expected.torrentEnabled, "kaydetme torrent ayarini korudu");
    await page.waitForSelector("#settings-drawer", { state: "hidden", timeout: 10_000 });
    ok("ayarlar kaydedildikten sonra cekmece kapandi");

    // Test bildirimi dugmesi api.notifyTest cagirmali.
    await page.click("#settings-btn");
    await page.waitForSelector("#settings-drawer:not([hidden])", { timeout: 20_000 });
    // Testin biraktigi durum varsayilan olmali. Cekmece yeniden acilinca form, kaydedilen
    // ayarlarla baglanir (updateCheck=false); mock GET varsayilani (true) ise tazeler.
    await page.waitForTimeout(500); // olasi GET tazelemesinin oturmasini bekle
    if (!(await page.locator("#set-update-check").isChecked())) {
      await page.locator("label.switch:has(#set-update-check)").click();
    }
    check(
      await page.locator("#set-update-check").isChecked(),
      "#set-update-check varsayilan duruma dondu",
    );
    await page.waitForSelector("#notify-test:visible", { timeout: 20_000 });
    await page.click("#notify-test");
    await page.waitForFunction(() => (window.__hydraCalls.notifyTest ?? 0) > 0, null, {
      timeout: 10_000,
    });
    ok("test bildirimi dugmesi api.notifyTest cagirdi");
    await page.keyboard.press("Escape");
    await page.waitForSelector("#settings-drawer", { state: "hidden", timeout: 10_000 });

    // --- 6. Indirme listesi ------------------------------------------------
    check(await page.evaluate(() => window.__hydraMock.pushList()), "mock 5 indirmeyi olay uzerinden gonderdi");
    await page.waitForFunction(() => document.querySelectorAll("#list .row").length === 5, null, { timeout: 10_000 });

    const rows = await page.evaluate(() =>
      [...document.querySelectorAll("#list .row")].map((row) => ({
        id: row.dataset.id ?? "",
        status: row.dataset.status ?? "",
        badge: row.querySelector(".badge")?.textContent?.trim() ?? "",
        name: row.querySelector(".name")?.textContent?.trim() ?? "",
        kind: row.querySelector(".type-icon")?.getAttribute("data-kind") ?? "",
      })),
    );
    for (const row of rows) {
      console.log(`..     satir ${row.id}: ${row.status} / ${row.badge} / ${row.name} (${row.kind})`);
    }
    check(rows.length === 5, `listede 5 indirme satiri var (${rows.length})`);
    check(
      ["downloading", "completed", "failed"].every((status) => rows.some((row) => row.status === status)),
      "downloading / completed / failed durumlarinin ucu de listelendi",
    );
    const expectedBadges = { downloading: "İndiriliyor", completed: "Tamamlandı", failed: "Hata" };
    for (const [status, label] of Object.entries(expectedBadges)) {
      const row = rows.find((item) => item.status === status);
      check(row?.badge === label, `${status} rozeti '${label}'`);
    }
    check(rows.some((row) => row.kind === "torrent"), "torrent kaydinin ikonu dogru");
    check((await page.locator("#count-all").textContent()).trim() === "5", "sayac 'Tumu' 5 gosteriyor");
    await page.waitForTimeout(300);
    await shot("04-list.png");

    // --- 6a. Yeniden deneme gostergesi ------------------------------------
    const retryRow = page.locator('#list .row[data-id="dl-retry-5"]');
    const retryText = (await retryRow.locator(".speed").textContent()).trim();
    check(
      /Yeniden deneniyor 3\/5/.test(retryText) && /4 sn/.test(retryText),
      `yeniden deneme metni: ${retryText}`,
    );
    check(
      (await retryRow.locator(".speed").getAttribute("class")).includes("is-retry"),
      "yeniden deneme satiri vurgulu",
    );
    // --- 6b. Satirdan torrent dosya secimini degistirme --------------------
    const torrentRow = page.locator('#list .row[data-id="dl-torrent-4"]');
    const torrentMeta = (await torrentRow.locator(".seg-info").textContent()).trim();
    check(torrentMeta.includes("2/3"), `satir '2/3 dosya' gosteriyor ('${torrentMeta}')`);
    // Tohum orani: uploaded/downloaded = 1.00
    check(torrentMeta.includes("↑ 1.00"), `tohum orani gosterildi ('${torrentMeta}')`);

    await torrentRow.locator('button[data-action="files"]').click();
    await page.waitForSelector("#files-sheet:not([hidden])", { timeout: 10_000 });
    check((await page.locator("#files-list .file-row").count()) === 3, "dosya secim sayfasi 3 satir cizdi");
    check((await page.locator("#files-list input:checked").count()) === 2, "mevcut secim (2 dosya) isaretli");

    await page.locator("#files-list .file-row").nth(2).locator("input").check();
    await page.click("#files-apply");
    await page.waitForFunction(() => (window.__hydraCalls.setTorrentFiles?.length ?? 0) > 0, null, {
      timeout: 10_000,
    });
    const setCall = await page.evaluate(() => window.__hydraCalls.setTorrentFiles[0]);
    check(setCall.id === "dl-torrent-4", `setTorrentFiles dogru kayda gitti (${setCall.id})`);
    check(JSON.stringify(setCall.files) === JSON.stringify([0, 1, 2]), `setTorrentFiles tum dosyalari gonderdi (${JSON.stringify(setCall.files)})`);
    await page.waitForSelector("#files-sheet", { state: "hidden", timeout: 10_000 });

    // --- 6c. Gecmiste arama -------------------------------------------------
    await page.fill("#search-input", "ders");
    await page.waitForFunction(
      () => document.querySelectorAll("#list .row").length === 1,
      null,
      { timeout: 10_000 },
    );
    const searchRow = (await page.locator("#list .row .name").first().textContent()).trim();
    check(searchRow === "ders-12.mp4", `arama tek kaydi buldu (${searchRow})`);
    check(
      (await page.locator("#count-all").textContent()).trim() === "1",
      "arama acikken 'Tumu' sayaci daraldi",
    );

    // Buyuk/kucuk harf ve Turkce karakter eslesmeli (dosya adinda 'arsiv' var).
    await page.fill("#search-input", "ARSİV");
    await page.waitForFunction(
      () => document.querySelectorAll("#list .row").length === 2,
      null,
      { timeout: 10_000 },
    );
    ok("arama buyuk harf/Turkce karakter duyarsiz (ARSİV -> 2 kayit)");

    // Eslesmeyen arama bos durum metnini degistirmeli.
    await page.fill("#search-input", "zzzz-yok");
    await page.waitForSelector("#empty:not([hidden])", { timeout: 10_000 });
    const emptyTitle = (await page.locator("#empty h2").textContent()).trim();
    check(emptyTitle === "Aramanla eşleşen kayıt yok", `eslesmeyen aramada bos durum metni ('${emptyTitle}')`);

    // Escape aramayi temizler, liste geri gelir.
    await page.locator("#search-input").focus();
    await page.keyboard.press("Escape");
    await page.waitForFunction(
      () => document.querySelectorAll("#list .row").length === 5,
      null,
      { timeout: 10_000 },
    );
    check((await page.locator("#search-input").inputValue()) === "", "Esc aramayi temizledi");
    check(
      (await page.locator("#count-all").textContent()).trim() === "5",
      "arama temizlenince sayac 5'e dondu",
    );

    // --- 6d. Siralama -------------------------------------------------------
    await page.selectOption("#sort-select", "size");
    await page.waitForFunction(
      () =>
        document.querySelector("#list .row .name")?.textContent?.trim() === "koleksiyon",
      null,
      { timeout: 10_000 },
    );
    const bySize = await page.evaluate(() =>
      [...document.querySelectorAll("#list .row .name")].map((el) => el.textContent.trim()),
    );
    check(
      JSON.stringify(bySize) ===
        JSON.stringify([
          "koleksiyon",
          "arsiv-2026.tar.zst",
          "arsiv.iso",
          "belgesel-4k.mp4",
          "ders-12.mp4",
        ]),
      `boyut siralamasi dogru (${bySize.join(" > ")})`,
    );

    await page.selectOption("#sort-select", "name");
    const firstName = await page.evaluate(
      () => document.querySelector("#list .row .name")?.textContent?.trim() ?? "",
    );
    check(firstName.startsWith("arsiv"), `ad siralamasi ilk kaydi '${firstName}'`);
    await shot("04b-search-sort.png");

    // Siralama tercihi kalici: yeniden yukleyince korunur.
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForFunction(() => Boolean(window.__hydraCalls), null, { timeout: 20_000 });
    check(
      (await page.locator("#sort-select").inputValue()) === "name",
      "siralama tercihi yeniden yuklemede korundu",
    );
    await page.selectOption("#sort-select", "new");
    await page.waitForFunction(() => Boolean(window.__hydraCalls), null, { timeout: 20_000 });
    ok("varsayilan siralama 'Yeni -> eski' geri secildi");

    // --- 6e. Ayarlar aninda acilir (yavas appInfo arayuzu kilitlemez) -------
    await page.evaluate(() => {
      window.__hydraMock.delayMs = { appInfo: 1500 };
    });
    const openedAt = Date.now();
    await page.click("#settings-btn");
    await page.waitForSelector("#settings-drawer:not([hidden])", { timeout: 5000 });
    const openMs = Date.now() - openedAt;
    check(openMs < 500, `ayarlar ${openMs} ms'de acildi (1,5 sn'lik yavas api'ye ragmen)`);
    check(await page.locator("#save-settings").isDisabled(), "veri gelmeden kaydet dugmesi kilitli");
    await page.waitForFunction(
      () => {
        const input = document.querySelector("#set-dir");
        return input instanceof HTMLInputElement && input.value.trim().length > 0;
      },
      null,
      { timeout: 10_000 },
    );
    check(await page.locator("#save-settings").isEnabled(), "veri gelince kaydet dugmesi acildi");

    // --- 6e2. yt-dlp güncelleme kontrolü ----------------------------------
    await page.click("#check-ytdlp");
    await page.waitForFunction(
      () => (window.__hydraCalls.checkYtdlpUpdate ?? 0) > 0,
      null,
      { timeout: 10_000 },
    );
    const updateText = (await page.locator("#ytdlp-update-status").textContent()) ?? "";
    check(updateText.includes("2026.11.01"), `guncelleme metni yeni surumu gosteriyor ('${updateText.trim()}')`);
    check(await page.locator("#update-ytdlp").isVisible(), "yeni surum varsa Guncelle dugmesi gorunur");
    check(
      ((await page.locator("#update-ytdlp").textContent()) ?? "").includes("2026.11.01"),
      "Guncelle dugmesi surumu yaziyor",
    );

    const installBefore = await page.evaluate(() => window.__hydraCalls.installYtdlp ?? 0);
    await page.click("#update-ytdlp");
    await page.waitForFunction(
      (before) => (window.__hydraCalls.installYtdlp ?? 0) > before,
      installBefore,
      { timeout: 15_000 },
    );
    ok("Guncelle dugmesi kurulum akisini tetikledi");

    // --- 6e3. Ayar disa/ice aktarma ---------------------------------------
    await page.evaluate(() => {
      globalThis.__hydraDialog = { save: "C:\\Users\\ornek\\yedek.json", open: "C:\\Users\\ornek\\yedek.json" };
    });
    await page.click("#export-settings");
    await page.waitForFunction(
      () => (window.__hydraCalls.exportSettingsTo ?? []).length > 0,
      null,
      { timeout: 10_000 },
    );
    const exported = await page.evaluate(() => window.__hydraCalls.exportSettingsTo[0]);
    check(
      exported?.path === "C:\\Users\\ornek\\yedek.json",
      `disa aktarma secilen yolu kullandi (${exported?.path})`,
    );
    check(exported?.includeToken === false, "token varsayilan olarak disa aktarilmadi");
    const exportMsg = (await page.locator("#backup-status").textContent()) ?? "";
    check(exportMsg.includes("Aktarıldı"), `disa aktarma geri bildirimi ('${exportMsg.trim()}')`);

    await page.click("#import-settings");
    await page.waitForFunction(
      () => (document.querySelector("#backup-status")?.textContent ?? "").includes("ayar güncellendi"),
      null,
      { timeout: 10_000 },
    );
    const importMsg = (await page.locator("#backup-status").textContent()) ?? "";
    check(
      importMsg.includes("2 ayar güncellendi"),
      `ice aktarma ozeti degisen sayiyi gosteriyor ('${importMsg.trim()}')`,
    );
    check(
      importMsg.includes("eşzamanlı indirme"),
      "degisen alan adi Turkce etiketle yazildi",
    );
    ok("ayar disa/ice aktarma akisi tamam");
    await shot("03b-yedekleme.png");

    await page.evaluate(() => {
      window.__hydraMock.delayMs = null;
    });
    await page.keyboard.press("Escape");
    await page.waitForSelector("#settings-drawer", { state: "hidden", timeout: 5000 });

    // --- 6f. Sanal liste: 300 kayitta DOM sinirli kalir --------------------
    await page.evaluate(() => window.__hydraMock.pushMany(300));
    await page.waitForFunction(
      () => (document.querySelector("#count-all")?.textContent ?? "") === "300",
      null,
      { timeout: 15_000 },
    );
    const domRows = await page.evaluate(() => document.querySelectorAll("#list .row").length);
    check(domRows > 0 && domRows < 60, `300 kayitta DOM'da ${domRows} satir (sinirli)`);
    check(
      await page.evaluate(() => document.querySelector("#list")?.classList.contains("virtual")),
      "liste sanal moda gecti",
    );
    const firstVisible = await page.evaluate(
      () => document.querySelector("#list .row")?.dataset.id ?? "",
    );
    check(firstVisible === "dl-many-0", `en ustteki kayit ilk sirada (${firstVisible})`);
    await shot("04b-virtual.png");

    // Sona kaydir: son kayit gorunur olmali (offset hesabi dogru).
    await page.evaluate(() => {
      const content = document.querySelector(".content");
      if (content) content.scrollTop = content.scrollHeight;
    });
    await page.waitForFunction(
      () => {
        const rows = [...document.querySelectorAll("#list .row")];
        return rows.some((row) => row.dataset.id === "dl-many-299");
      },
      null,
      { timeout: 15_000 },
    );
    ok("sona kaydirinca son kayit cizildi");
    const bottomRows = await page.evaluate(() => document.querySelectorAll("#list .row").length);
    check(bottomRows < 60, `kaydirmada da DOM sinirli (${bottomRows} satir)`);

    // --- 6g. Toplu secim --------------------------------------------------
    await page.evaluate(() => {
      const content = document.querySelector(".content");
      if (content) content.scrollTop = 0;
    });
    await page.waitForTimeout(200);
    await page.evaluate(() => {
      const boxes = [...document.querySelectorAll("#list .pick input")];
      for (const box of boxes.slice(0, 3)) box.click();
    });
    await page.waitForSelector("#selectbar:not([hidden])", { timeout: 5000 });
    const selectText = (await page.locator("#select-count").textContent()) ?? "";
    check(selectText.includes("3"), `secim cubugu 3 kayit gosteriyor ('${selectText.trim()}')`);
    check(
      (await page.locator("#selectbar button[data-bulk='pause']").isEnabled()) === true,
      "duraklat etkin (secimde aktif kayit var)",
    );
    await shot("04c-secim.png");

    const pauseBefore = await page.evaluate(() => window.__hydraCalls.pause.length);
    await page.click("#selectbar button[data-bulk='pause']");
    await page.waitForFunction(
      (before) => window.__hydraCalls.pause.length > before,
      pauseBefore,
      { timeout: 10_000 },
    );
    const paused = await page.evaluate((before) => window.__hydraCalls.pause.slice(before), pauseBefore);
    check(paused.includes("dl-many-0"), `secilen aktif kayit duraklatildi (${paused.join(", ")})`);

    await page.click("#selectbar button[data-bulk='clear']");
    await page.waitForSelector("#selectbar", { state: "hidden", timeout: 5000 });
    ok("secim temizlendi, cubuk kapandi");
    await page.evaluate(() => window.__hydraMock.pushList());
    await page.waitForFunction(
      () => document.querySelectorAll("#list .row").length === 5,
      null,
      { timeout: 10_000 },
    );
    ok("normal listeye donunce sanal mod kapandi");
    check(
      !(await page.evaluate(() => document.querySelector("#list")?.classList.contains("virtual"))),
      "liste artik sanal degil",
    );

    // --- 6h. Güncelleme şeridi ---------------------------------------------
    // Eklenti sahte; davranış `__hydraUpdater` (sayfa yenilemede localStorage
    // üzerinden taşınır). Kontrol açılıştan 3 sn sonra çalışır.
    const setUpdateMock = (state) =>
      page.evaluate((next) => {
        window.localStorage.setItem("__hydraUpdaterMock", JSON.stringify(next));
      }, state);

    /**
     * Açılış kontrolünün gerçekten koşmasını bekler (3 sn gecikme + pay).
     * Sayaç sıfırlanmışsa (az önce yenilendiyse) hemen okumak yanıltıcı olur.
     */
    const seenUpdaterCheck = () =>
      page
        .waitForFunction(() => (globalThis.__hydraUpdater?.checks ?? 0) > 0, null, {
          timeout: 8_000,
        })
        .then(() => true)
        .catch(() => false);

    // 1) Sürüm yok → şerit gizli, ama kontrol gerçekten çağrıldı.
    check(await page.locator("#update-strip").isHidden(), "surum yokken serit gizli");
    check(await seenUpdaterCheck(), "acilista guncelleme kontrolu cagrildi");
    check(await page.locator("#update-strip").isHidden(), "kontrol sonrasi serit hala gizli");

    // 2) Sürüm var → şerit görünür, metin sürümü içerir.
    await setUpdateMock({ available: true, version: "0.8.1" });
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForSelector("#update-strip:not([hidden])", { timeout: 10_000 });
    const stripText = ((await page.locator("#update-text").textContent()) ?? "").trim();
    check(stripText.includes("0.8.1"), `serit surumu gosteriyor ('${stripText}')`);
    check(await page.locator("#update-install").isVisible(), "Guncelle dugmesi gorunur");
    check(await page.locator("#update-later").isVisible(), "Sonra dugmesi gorunur");
    // Şerit görünürken de satırlar kaymamalı (7. satır gerçekten ekleniyor).
    await checkLayoutPinned("serit gorunur");
    await shot("05-update-strip.png");

    // 3) "Sonra" → gizlenir ve aynı sürüm için hatırlanır.
    await page.click("#update-later");
    check(await page.locator("#update-strip").isHidden(), "Sonra seridi kapatti");
    // Kaynak bırakılmalı: aksi hâlde eklenti tarafında rid sızar.
    const closedAfterSkip = await page.evaluate(() => globalThis.__hydraUpdater?.closed ?? 0);
    check(closedAfterSkip > 0, `Sonra guncelleme kaynagini birakti (closed=${closedAfterSkip})`);
    await page.reload({ waitUntil: "domcontentloaded" });
    check(await seenUpdaterCheck(), "atlanan surumde kontrol yine calisti");
    check(await page.locator("#update-strip").isHidden(), "ayni surum icin tekrar sorulmadi");

    // 4) Yeni sürüm → "Güncelle" indirir; önce yüzde, sonra kurulum metni.
    await setUpdateMock({ available: true, version: "0.9.0" });
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForSelector("#update-strip:not([hidden])", { timeout: 10_000 });
    await page.click("#update-install");
    // Yüzde metni gerçekten geçmeli: `Started` (0 byte) → `Progress` (40/100).
    // Yüzde bağlantısı ters çevrilse (downloaded/total yerine total/downloaded)
    // bu bekleme zaman aşımına düşer.
    const sawPercent = await page
      .waitForFunction(
        () => {
          const text = document.querySelector("#update-text")?.textContent ?? "";
          return text.includes("%") && text.includes("40");
        },
        null,
        { timeout: 10_000 },
      )
      .then(() => true)
      .catch(() => false);
    check(sawPercent, "indirme yuzdesi seritte gorundu (%40)");

    await page.waitForFunction(() => (globalThis.__hydraUpdater?.installed ?? 0) > 0, null, {
      timeout: 10_000,
    });
    await page.waitForFunction(
      () => (document.querySelector("#update-text")?.textContent ?? "").includes("Kuruluyor"),
      null,
      { timeout: 10_000 },
    );
    const installingText = ((await page.locator("#update-text").textContent()) ?? "").trim();
    check(installingText.includes("Kuruluyor"), `kurulum metni gosterildi ('${installingText}')`);
    check(await page.locator("#update-install").isHidden(), "kurulumda Guncelle dugmesi gizli");
    const pendingMarker = await page.evaluate(() => window.localStorage.getItem("hydra.update.pending"));
    check(pendingMarker === "0.9.0", `kurulum oncesi isaret yazildi (${pendingMarker})`);
    // Temiz kapanış: kurulumdan önce çalışanlar duraklatılıp durum kaydedilir
    // (Rust `pause_all_for_shutdown`). Çağrı yoksa yükseltme indirmeleri keser.
    const pauseCalls = await page.evaluate(() => window.__hydraCalls.pauseAllForShutdown ?? 0);
    check(pauseCalls > 0, `kurulum oncesi temiz kapanis istendi (${pauseCalls} cagri)`);
    // Sıra da doğru olmalı: `downloadAndInstall` **çağrıldığı anda** sayaç > 0.
    const pauseAtInstall = await page.evaluate(
      () => globalThis.__hydraUpdater?.pauseCallsAtInstall ?? -1,
    );
    check(
      pauseAtInstall > 0,
      `duraklatma kurulumdan once yapildi (kurulum aninda ${pauseAtInstall} cagri)`,
    );

    // 4b) Kurulum reddederse → hata metni + "Yeniden dene", pending silinir.
    await page.evaluate(() => window.localStorage.removeItem("hydra.update.pending"));
    await setUpdateMock({ available: true, version: "0.9.1", failWith: "agl hatasi" });
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForSelector("#update-strip:not([hidden])", { timeout: 10_000 });
    await page.click("#update-install");
    await page.waitForFunction(
      () => (document.querySelector("#update-strip")?.getAttribute("class") ?? "").includes("error"),
      null,
      { timeout: 10_000 },
    );
    const errorText = ((await page.locator("#update-text").textContent()) ?? "").trim();
    check(
      errorText.includes("indirilemedi"),
      `basarisiz kurulum hatayi gosterdi ('${errorText}')`,
    );
    check(
      ((await page.locator("#update-install").textContent()) ?? "").trim() === "Yeniden dene",
      "basarisiz kurulumda dugme 'Yeniden dene' oldu",
    );
    // Başarısız yolda da sıra: indirme **denenirken** duraklatma çoktan olmuştu.
    const pauseAtFailedInstall = await page.evaluate(
      () => globalThis.__hydraUpdater?.pauseCallsAtInstall ?? -1,
    );
    check(
      pauseAtFailedInstall > 0,
      `basarisiz denemede bile once duraklatildi (kurulum aninda ${pauseAtFailedInstall} cagri)`,
    );
    // Kurtarma yolu: kurulum düşünce kapanış kilidi açılmalı, yoksa kuyruk
    // sonsuza dek durur ve kullanıcı "Güncelle"ye basmadan önceki hâline dönemez.
    const cancelCalls = await page.evaluate(() => window.__hydraCalls.cancelShutdown ?? 0);
    check(cancelCalls > 0, `basarisiz kurulumda kapanis geri alindi (${cancelCalls} cagri)`);
    const pendingAfterFailure = await page.evaluate(() =>
      window.localStorage.getItem("hydra.update.pending"),
    );
    check(pendingAfterFailure === null, `basarisiz kurulum isareti temizledi (${pendingAfterFailure})`);

    // 4c) "Yeniden dene" taze kaynak alır (ölü rid ile sonsuz döngü olmaz).
    const checksBeforeRetry = await page.evaluate(() => globalThis.__hydraUpdater?.checks ?? 0);
    // Bu kez başarılı olsun: sahte eklentinin canlı nesnesinden hatayı kaldır
    // (localStorage yalnız yenilemede okunuyor).
    await page.evaluate(() => {
      delete globalThis.__hydraUpdater.failWith;
    });
    await page.click("#update-install");
    await page.waitForFunction(
      (before) => (globalThis.__hydraUpdater?.checks ?? 0) > before,
      checksBeforeRetry,
      { timeout: 10_000 },
    );
    ok("Yeniden dene yeni bir check() cagirdi (taze kaynak)");
    await page.waitForFunction(
      () => (globalThis.__hydraUpdater?.installed ?? 0) >= 2,
      null,
      { timeout: 10_000 },
    );
    ok("Yeniden dene indirmeyi yeniden baslatti");

    // 5) Yarım kalan kurulum açılışta bildirilir (sürüm 0.2.0'da kaldı).
    //    Sahte eklenti sürüm sunmuyor ki 3 sn sonraki kontrol metni ezmesin.
    await setUpdateMock({ available: false });
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForSelector("#update-strip:not([hidden])", { timeout: 10_000 });
    const failedText = ((await page.locator("#update-text").textContent()) ?? "").trim();
    check(
      failedText.includes("Güncelleme tamamlanmadı"),
      `yarim kurulum bildirildi ('${failedText}')`,
    );
    check(
      (await page.locator("#update-strip").getAttribute("class")).includes("error"),
      "yarim kurulum seridi hata tonunda",
    );

    // 5b) Bu durumda "Sonra" da kalıcı olmalı: aksi hâlde 3 sn sonraki
    //     kontrol şeridi aynı oturumda geri getirir.
    await setUpdateMock({ available: true, version: "0.9.1" });
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForSelector("#update-strip:not([hidden])", { timeout: 10_000 });
    await page.click("#update-later");
    check(await page.locator("#update-strip").isHidden(), "yarim kurulumda Sonra kapatti");
    check(await seenUpdaterCheck(), "yarim kurulumdan sonra kontrol kostu");
    check(
      await page.locator("#update-strip").isHidden(),
      "yarim kurulumda Sonra kalici oldu (serit geri gelmedi)",
    );
    await page.evaluate(() => window.localStorage.removeItem("hydra.update.pending"));

    // 6) Ayar kapalı → kontrol hiç çağrılmaz (yeni sayfa: sayaç sıfırdan başlar).
    apiPatch = "\nSETTINGS.updateCheck = false;\n";
    await setUpdateMock({ available: true, version: "0.9.9" });
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForTimeout(3500);
    const disabledChecks = await page.evaluate(() => globalThis.__hydraUpdater?.checks ?? 0);
    check(disabledChecks === 0, `ayar kapaliyken kontrol cagrilmadi (${disabledChecks})`);
    check(await page.locator("#update-strip").isHidden(), "ayar kapaliyken serit gizli");
    apiPatch = "";

    // --- 7. Konsol ---------------------------------------------------------
    if (consoleErrors.length === 0) {
      ok("konsol hatasi yok");
    } else {
      fail(`konsolda ${consoleErrors.length} hata var`);
      for (const message of consoleErrors) console.error(`     ${message}`);
    }
  } finally {
    if (browser) await browser.close().catch(() => {});
    if (child?.pid) {
      spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
    }
    if (failures > 0) {
      const tail = output.join("").trim();
      if (tail) {
        console.error("---- Vite cikti kuyrugu ----");
        console.error(tail.split(/\r?\n/).slice(-25).join("\n"));
      }
    }
  }

  if (failures > 0) {
    console.error(`\nSONUC: FAIL (${failures} sorun)`);
    return 1;
  }
  console.log("\nSONUC: PASS");
  return 0;
}

// Cikis kodu: main() 0/1 doner; hata olursa 1.
let code;
try {
  code = await main();
} catch (err) {
  console.error(`FAIL ${err instanceof Error ? err.message : String(err)}`);
  code = 1;
}
process.exit(code ?? 1);
