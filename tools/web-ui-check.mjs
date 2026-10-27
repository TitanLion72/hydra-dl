// Web (telefon) arayüzü QA: gerçek webapp dosyalarını statik sunucuyla servis eder,
// API'yi Playwright route mock'larıyla karşılar. Uygulamanın çalışması gerekmez.
//
// Kapsam: ayarlar ekranı, yt-dlp durumu ve tek tıkla kurulum akışı (mutlu yol + hata).
//
// Kullanım: `node tools/web-ui-check.mjs`
//
// Playwright `tools/ui-harness` bağımlılığından çözülür; ayrı kurulum gerekmez.

import { createRequire } from "node:module";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const webappDir = path.join(repoRoot, "webapp");
const requireHarness = createRequire(path.join(here, "ui-harness", "package.json"));
const { chromium } = requireHarness("playwright");

const PORT = Number(process.env.HYDRA_WEBUI_PORT || 6791);
const BASE = `http://127.0.0.1:${PORT}`;
const SHOTS = process.env.TEMP || process.env.TMP || ".";

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".webmanifest": "application/manifest+json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
};

let failures = 0;
const ok = (message) => console.log(`OK   ${message}`);
const fail = (message) => {
  failures += 1;
  console.error(`FAIL ${message}`);
};
const check = (condition, message) => (condition ? ok(message) : fail(message));

const JSON_OK = { status: 200, contentType: "application/json" };

/** `webapp/` klasörünü kökten servis eden minimal sunucu (SPA yönlendirmesi yok). */
function startStaticServer() {
  const server = createServer(async (request, response) => {
    const url = new URL(request.url || "/", BASE);
    const file = url.pathname === "/" ? "/index.html" : url.pathname;
    const full = path.join(webappDir, path.normalize(file).replace(/^([/\\])+/, ""));
    if (!full.startsWith(webappDir)) {
      response.writeHead(403).end();
      return;
    }
    try {
      const body = await readFile(full);
      response.writeHead(200, {
        "content-type": TYPES[path.extname(full)] || "application/octet-stream",
      });
      response.end(body);
    } catch {
      response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
      response.end("bulunamadi");
    }
  });
  return new Promise((resolve) => {
    server.listen(PORT, "127.0.0.1", () => resolve(server));
  });
}

/** Ayarlar sekmesini açar ve formun yüklenmesini bekler. */
async function openSettings(page) {
  await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
  await page.click('.tab[data-view="settings"]');
  await page.waitForSelector("#view-settings:not([hidden])");
  await page.waitForSelector("#settingsForm:not([hidden])", { timeout: 15000 });
}

/** API uçlarını mock'lar; `installError` verilirse kurulum 502 döner. */
async function mockApi(page, { available, version = "2099.01.01", installError = null }) {
  let installed = available;
  await page.route("**/events", (route) =>
    route.fulfill({ status: 200, contentType: "text/event-stream", body: ": mock\n\n" }),
  );
  await page.route("**/health", (route) =>
    route.fulfill({
      ...JSON_OK,
      body: JSON.stringify({
        app: "hydra-dl",
        version: "0.2.2",
        uptimeSecs: 90,
        downloads: 0,
      }),
    }),
  );
  await page.route("**/downloads", (route) => route.fulfill({ ...JSON_OK, body: "[]" }));
  await page.route("**/settings", (route) =>
    route.fulfill({
      ...JSON_OK,
      body: JSON.stringify({
        downloadDir: "C:\\Users\\test\\Downloads",
        maxConcurrent: 3,
        connections: 8,
        speedLimitKbps: 0,
        captureEnabled: true,
        capturePort: 6712,
        ytdlpPath: "",
        autoResume: true,
        webToken: "",
      }),
    }),
  );
  await page.route("**/ytdlp/install", async (route) => {
    if (installError) {
      await route.fulfill({
        status: 502,
        contentType: "application/json",
        body: JSON.stringify({ ok: false, error: installError }),
      });
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 400)); // indirme taklidi
    installed = true;
    await route.fulfill({
      ...JSON_OK,
      body: JSON.stringify({ ok: true, path: "C:\\fake\\bin\\yt-dlp.exe" }),
    });
  });
  await page.route("**/ytdlp", (route) =>
    route.fulfill({
      ...JSON_OK,
      body: JSON.stringify(
        installed
          ? { available: true, path: "C:\\fake\\bin\\yt-dlp.exe", version }
          : { available: false, path: null, version: null },
      ),
    }),
  );
}

const server = await startStaticServer();
const browser = await chromium.launch();
try {
  // 1) yt-dlp kurulu: sürüm görünür, kurulum düğmesi gizli.
  {
    const page = await browser.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    await mockApi(page, { available: true });
    await openSettings(page);
    await page.waitForTimeout(700);
    const fact = (await page.textContent("#setYtdlp")).trim();
    check(fact.includes("2099.01.01"), `kurulu sürüm gösterildi: ${fact}`);
    check(await page.locator("#installYtdlp").isHidden(), "kurulu iken düğme gizli");
    check(await page.locator("#ytdlpHelp").isHidden(), "kurulu iken yardım metni gizli");
    check(errors.length === 0, `konsol hatası yok (${errors.join(" | ")})`);
    await page.screenshot({ path: path.join(SHOTS, "hydra-webui-ytdlp-kurulu.png") });

    // Ayarlardan çıkıp geri dönünce durum tazelenmeli (önbellekteki ayarlarla yetinilmez).
    await page.click('.tab[data-view="downloads"]');
    await page.waitForSelector("#view-downloads:not([hidden])");
    await page.click('.tab[data-view="settings"]');
    await page.waitForSelector("#settingsForm:not([hidden])");
    await page.waitForTimeout(500);
    const factAgain = (await page.textContent("#setYtdlp")).trim();
    check(factAgain.includes("2099.01.01"), `geri dönünce durum korundu: ${factAgain}`);
    check(await page.locator("#installYtdlp").isHidden(), "geri dönünce düğme hâlâ gizli");
    await page.close();
  }

  // 2) yt-dlp yok: düğme görünür → tıkla → kuruldu → düğme gizlenir, sürüm tazelenir.
  {
    const page = await browser.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    await mockApi(page, { available: false });
    await openSettings(page);
    await page.waitForTimeout(700);
    const fact = (await page.textContent("#setYtdlp")).trim();
    check(fact === "bulunamadı", `durum metni: ${fact}`);
    check(await page.locator("#installYtdlp").isVisible(), "yokken kur düğmesi görünür");
    check(await page.locator("#ytdlpHelp").isVisible(), "yokken yardım metni görünür");
    await page.screenshot({ path: path.join(SHOTS, "hydra-webui-ytdlp-yok.png") });

    await page.click("#installYtdlp");
    const busy = (await page.textContent("#installYtdlp")).trim();
    check(/Kuruluyor/.test(busy), `tıklayınca düğme metni: ${busy}`);

    await page.waitForFunction(
      () => /Kuruldu/.test(document.querySelector("#ytdlpStatus")?.textContent || ""),
      { timeout: 20000 },
    );
    const status = (await page.textContent("#ytdlpStatus")).trim();
    check(status.includes("Kuruldu"), `kurulum mesajı: ${status}`);
    await page.waitForTimeout(800);
    const factAfter = (await page.textContent("#setYtdlp")).trim();
    check(factAfter.includes("2099.01.01"), `sürüm tazelendi: ${factAfter}`);
    check(await page.locator("#installYtdlp").isHidden(), "kurulumdan sonra düğme gizli");
    check(errors.length === 0, `konsol hatası yok (${errors.join(" | ")})`);
    await page.screenshot({ path: path.join(SHOTS, "hydra-webui-ytdlp-kuruldu.png") });
    await page.close();
  }

  // 3) Kurulum hatası: mesaj gösterilir, düğme yeniden denenebilir kalır.
  {
    const page = await browser.newPage();
    await mockApi(page, { available: false, installError: "GitHub'a ulaşılamadı" });
    await openSettings(page);
    await page.waitForTimeout(700);
    await page.click("#installYtdlp");
    await page.waitForFunction(
      () => /ulaşılamadı/.test(document.querySelector("#ytdlpStatus")?.textContent || ""),
      { timeout: 15000 },
    );
    const status = (await page.textContent("#ytdlpStatus")).trim();
    check(status.includes("ulaşılamadı"), `hata mesajı gösterildi: ${status}`);
    check(await page.locator("#installYtdlp").isEnabled(), "hatadan sonra düğme etkin");
    const label = (await page.textContent("#installYtdlp")).trim();
    check(/kur/i.test(label) && !/Kuruluyor/.test(label), `düğme metni geri döndü: ${label}`);
    await page.close();
  }
} finally {
  await browser.close();
  server.close();
}

console.log(failures === 0 ? "\nSONUC: PASS" : `\nSONUC: FAIL (${failures})`);
process.exit(failures === 0 ? 0 : 1);
