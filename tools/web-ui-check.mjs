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
const TORRENT_URL =
  "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=arsiv-2026";

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

/** API uçlarını mock'lar; `installError` verilirse kurulum 502 döner.
 *  `statusDelay` ile `/ytdlp` yanıtı geciktirilir (yavaş `--version` taklidi). */
async function mockApi(
  page,
  { available, version = "2099.01.01", installError = null, statusDelay = 0 },
) {
  let installed = available;
  const torrentSources = [];
  const setTorrentFilesCalls = [];
  const addCalls = [];
  page.__calls = { torrentSources, setTorrentFilesCalls, addCalls };
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
  await page.route("**/downloads", (route) =>
    route.fulfill({
      ...JSON_OK,
      body: JSON.stringify([
        {
          id: "dl-retry-1",
          url: "https://yavas.example.com/arsiv.iso",
          filename: "arsiv.iso",
          dir: "C:\\Users\\test\\Downloads",
          kind: "http",
          status: "downloading",
          totalBytes: 1073741824,
          downloadedBytes: 268435456,
          segments: [{ index: 0, start: 0, end: 1073741823, done: 268435456 }],
          resumable: true,
          speedBps: 0,
          progress: 0.25,
          error: null,
          createdAt: Date.now() - 60000,
          finishedAt: null,
          connections: 4,
          referer: null,
          mime: "application/octet-stream",
          filePath: null,
          format: null,
          swarm: null,
          source: null,
          retry: { attempt: 2, max: 5, waitMs: 2000 },
        },
      ]),
    }),
  );
  await page.route("**/probe", (route) =>
    route.fulfill({
      ...JSON_OK,
      body: JSON.stringify({
        ok: true,
        url: "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=arsiv-2026",
        finalUrl: "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=arsiv-2026",
        kind: "torrent",
        title: "arsiv-2026",
        filename: "arsiv-2026",
        resumable: true,
        needsYtdlp: false,
        formats: [],
        candidates: [],
        warnings: [],
      }),
    }),
  );
  await page.route("**/add", async (route) => {
    addCalls.push(JSON.parse(route.request().postData() || "{}"));
    await route.fulfill({
      ...JSON_OK,
      body: JSON.stringify({ ok: true, id: "dl-yeni", filename: "arsiv-2026" }),
    });
  });  await page.route("**/torrent/files", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 250)); // metadata taklidi
    const body = JSON.parse(route.request().postData() || "{}");
    if (body.files) {
      const updated = body.files;
      setTorrentFilesCalls.push({ id: body.id, files: updated });
      await route.fulfill({ ...JSON_OK, body: JSON.stringify({ ok: true }) });
      return;
    }
    torrentSources.push(body.source);
    await route.fulfill({
      ...JSON_OK,
      body: JSON.stringify({
        name: "arsiv-2026",
        infoHash: "0123456789abcdef0123456789abcdef01234567",
        totalBytes: 3145728000,
        files: [
          { index: 0, path: "bolum-01.mkv", length: 1572864000, included: true },
          { index: 1, path: "bolum-02.mkv", length: 1572864000, included: true },
          { index: 2, path: "afis.jpg", length: 524288, included: true },
        ],
      }),
    });
  });
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
  await page.route("**/ytdlp", async (route) => {
    if (statusDelay) await new Promise((resolve) => setTimeout(resolve, statusDelay));
    await route.fulfill({
      ...JSON_OK,
      body: JSON.stringify(
        installed
          ? { available: true, path: "C:\\fake\\bin\\yt-dlp.exe", version }
          : { available: false, path: null, version: null },
      ),
    });
  });
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

  // 4) Torrent: probe → dosya listesi → kısmi seçim → POST /add files ile.
  {
    const page = await browser.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    await mockApi(page, { available: true });
    await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
    await page.fill("#pasteUrl", TORRENT_URL);
    await page.press("#pasteUrl", "Enter");
    await page.waitForSelector("#sheet:not([hidden])");
    await page.waitForSelector("#fileGroup:not([hidden])", { timeout: 20000 });
    await page.waitForSelector("#fileList .file-opt", { timeout: 20000 });

    const rowCount = await page.locator("#fileList .file-opt").count();
    check(rowCount === 3, `telefonda 3 dosya satırı (${rowCount})`);
    const summary = (await page.textContent("#fileSummary")).trim();
    check(summary.includes("3 dosya") && summary.includes("tümü"), `özet: ${summary}`);

    // Ortadaki dosyayı kapat → 2/3.
    await page.locator("#fileList .file-opt").nth(1).locator("input").uncheck();
    const summary2 = (await page.textContent("#fileSummary")).trim();
    check(summary2.includes("2/3"), `kısmi seçim özeti: ${summary2}`);
    await page.screenshot({ path: path.join(SHOTS, "hydra-webui-torrent-dosyalar.png") });

    await page.click("#sheetAdd");
    let addCall = null;
    for (let i = 0; i < 40 && !addCall; i += 1) {
      await page.waitForTimeout(100);
      const calls = page.__calls?.addCalls ?? [];
      addCall = calls[0] ?? null;
    }
    check(!!addCall, "POST /add çağrısı gitti");
    check(
      JSON.stringify(addCall?.files) === JSON.stringify([0, 2]),
      `seçilen dosyalar gönderildi (${JSON.stringify(addCall?.files)})`,
    );
    check((page.__calls?.torrentSources ?? []).length === 1, "GET dosya listesi bir kez istendi");

    // "Hiçbiri" → uyarı, istek gitmez.
    await page.fill("#pasteUrl", TORRENT_URL);
    await page.press("#pasteUrl", "Enter");
    await page.waitForSelector("#fileList .file-opt", { timeout: 20000 });
    await page.click("#filesNone");
    const noneSummary = (await page.textContent("#fileSummary")).trim();
    check(noneSummary.includes("0/3"), `'Hiçbiri' seçimi kaldırdı: ${noneSummary}`);
    await page.click("#sheetAdd");
    await page.waitForTimeout(600);
    check((page.__calls?.addCalls ?? []).length === 1, "boş seçimde /add çağrısı gitmedi");
    const toast = (await page.textContent("#toast")).trim();
    check(/En az bir dosya/.test(toast), `uyarı gösterildi: ${toast}`);
    check(errors.length === 0, `konsol hatası yok (${errors.join(" | ")})`);
    await page.close();
  }

  // 4) Yavaş /ytdlp (sunucu `--version` çalıştırıyor): beklerken düğme kilitli,
  //    satır "kontrol ediliyor…" der; yanıt gelince düğme gizlenir.
  {
    const page = await browser.newPage();
    await mockApi(page, { available: true, statusDelay: 1500 });
    await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
    await page.click('.tab[data-view="settings"]');
    await page.waitForSelector("#settingsForm:not([hidden])", { timeout: 15000 });
    const waiting = (await page.textContent("#setYtdlp")).trim();
    check(waiting.startsWith("kontrol ediliyor"), `beklerken satır: ${waiting}`);
    check(await page.locator("#installYtdlp").isDisabled(), "beklerken kur düğmesi kilitli");
    await page.waitForFunction(
      () => /2099\.01\.01/.test(document.querySelector("#setYtdlp")?.textContent || ""),
      { timeout: 15000 },
    );
    check(await page.locator("#installYtdlp").isHidden(), "yanıt gelince düğme gizli");
    await page.close();
  }
  // 5) Telefon listesinde yeniden deneme göstergesi.
  {
    const page = await browser.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    await mockApi(page, { available: true });
    await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#list .card", { timeout: 15000 });
    const speed = (await page.textContent('#list .card [data-ref="speed"]')).trim();
    check(
      /Yeniden deneniyor 2\/5/.test(speed) && /2 sn/.test(speed),
      `telefonda yeniden deneme metni: ${speed}`,
    );
    const cls = await page.getAttribute('#list .card [data-ref="speed"]', "class");
    check((cls || "").includes("is-retry"), "telefonda retry vurgusu");
    check(errors.length === 0, `konsol hatası yok (${errors.join(" | ")})`);
    await page.screenshot({ path: path.join(SHOTS, "hydra-webui-retry.png") });
    await page.close();
  }
} finally {
  await browser.close();
  server.close();
}

console.log(failures === 0 ? "\nSONUC: PASS" : `\nSONUC: FAIL (${failures})`);
process.exit(failures === 0 ? 0 : 1);
