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

    const launched = await launchBrowser();
    browser = launched.browser;
    ok(`Tarayici acildi (channel: ${launched.channel})`);

    const context = await browser.newContext({
      viewport: { width: 1366, height: 900 },
      locale: "tr-TR",
      deviceScaleFactor: 1,
    });
    const page = await context.newPage();

    const consoleErrors = [];
    page.on("console", (message) => {
      if (message.type() === "error") consoleErrors.push(`console: ${message.text()}`);
    });
    page.on("pageerror", (error) => {
      consoleErrors.push(`pageerror: ${error.message}`);
    });

    const mockSource = await readFile(path.join(here, "mock-api.mjs"), "utf8");
    await page.route("**/src/api.ts*", (route) =>
      route.fulfill({
        status: 200,
        contentType: "text/javascript; charset=utf-8",
        body: mockSource,
      }),
    );
    ok("`/src/api.ts` sahte modul ile karsilandi");

    const shot = async (name) => {
      const file = path.join(artifactsDir, name);
      await page.screenshot({ path: file, fullPage: true });
      const info = await stat(file);
      check(info.size > 15 * 1024, `ekran goruntusu ${name} (${Math.round(info.size / 1024)} KB)`);
    };

    // --- 1. Ana ekran ------------------------------------------------------
    await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
    await page.waitForFunction(() => Boolean(window.__hydraCalls), null, { timeout: 20_000 });
    await page.waitForSelector("#empty:not([hidden])", { timeout: 20_000 });

    check((await page.title()) === "Hydra DL", "ana ekran basligi 'Hydra DL'");
    check(await page.locator("#empty").isVisible(), "bos durum gorunur");
    check((await page.locator("#empty h2").textContent()).trim() === "Henüz indirme yok", "bos durum metni dogru");
    check((await page.locator("#list .row").count()) === 0, "liste baslangicta bos");
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
    check((await page.locator("#set-web-port").inputValue()) === String(expected.webPort), "#set-web-port mock ayariyla uyumlu");
    check(
      (await page.locator("#set-concurrent").inputValue()) === String(expected.maxConcurrent),
      "#set-concurrent mock ayariyla uyumlu",
    );
    await shot("03-settings.png");

    await page.click("#save-settings");
    await page.waitForFunction(() => window.__hydraCalls.saveSettings.length > 0, null, { timeout: 10_000 });
    const saved = await page.evaluate(() => window.__hydraCalls.saveSettings[0]);
    check(saved.downloadDir === expected.downloadDir, "kaydetme dogru klasorle mock'a gitti");
    check(saved.webEnabled === expected.webEnabled, "kaydetme dogru web ayariyla mock'a gitti");
    check(saved.notifyOnFinish === expected.notifyOnFinish, "kaydetme bildirim ayarini korudu");
    check(saved.torrentEnabled === expected.torrentEnabled, "kaydetme torrent ayarini korudu");
    await page.waitForSelector("#settings-drawer", { state: "hidden", timeout: 10_000 });
    ok("ayarlar kaydedildikten sonra cekmece kapandi");

    // Test bildirimi dugmesi api.notifyTest cagirmali.
    await page.click("#settings-btn");
    await page.waitForSelector("#settings-drawer:not([hidden])", { timeout: 20_000 });
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

let code = 1;
try {
  code = await main();
} catch (err) {
  console.error(`FAIL ${err instanceof Error ? err.message : String(err)}`);
  code = 1;
}
process.exit(code);
