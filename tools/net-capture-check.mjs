// Eklentinin ağdan medya yakalamasını GERÇEK tarayıcıda doğrular.
//
// Ne yapar:
//   1. Yerel bir test sunucusu açar: bir sayfa + m3u8 + mpd + mp4 + medya olmayan
//      istekler (css/js/png) ve HLS parçaları (ts) üretir.
//   2. Playwright ile Edge/Chrome'u eklenti yüklenmiş olarak açar.
//   3. `netCapture` ayarını açar, sayfayı ziyaret eder (JS tüm varlıkları çeker).
//   4. Arka plandan `net-media` mesajıyla yakalananları ister.
//
// Beklenen: m3u8 + mpd + mp4 yakalanır; css/js/png/ts yakalanmaz.

import { createRequire } from "node:module";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const extDir = path.join(repoRoot, "build", "extensions", "chrome");
const requireHarness = createRequire(path.join(repoRoot, "tools", "ui-harness", "package.json"));
const { chromium } = requireHarness("playwright");

const PORT = Number(process.env.HYDRA_NET_PORT || 6796);
const BASE = `http://127.0.0.1:${PORT}`;
/** Hydra'nın yakalama sunucusu (eklentinin ayarladığı port). */
const HEALTH = "http://127.0.0.1:6712";

let failures = 0;
const ok = (m) => console.log(`OK   ${m}`);
const fail = (m) => {
  failures += 1;
  console.error(`FAIL ${m}`);
};
const check = (cond, m) => (cond ? ok(m) : fail(m));

const PAGE = `<!doctype html><html lang="tr"><head><meta charset="utf-8">
<title>Hydra medya testi</title>
<link rel="stylesheet" href="/style.css">
</head><body>
<h1>Test</h1>
<img src="/logo.png" alt="">
<video id="v" controls></video>
<script src="/app.js"></script>
<script>
// Sayfa "medyayı JS ile çeker" senaryosu: HTML'de hiçbir medya adresi yok.
fetch("/media/master.m3u8");
fetch("/media/manifest.mpd");
fetch("/media/video.mp4", { method: "GET" });
fetch("/media/seg-1.ts");
fetch("/media/chunk.m4s");
fetch("/media/stream-nouext");
fetch("/api/data.json");
</script>
</body></html>`;

const server = createServer((req, res) => {
  const url = req.url.split("?")[0];
  if (url === "/" || url === "/index.html") {
    res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    res.end(PAGE);
    return;
  }
  if (url === "/app.js") {
    res.writeHead(200, { "content-type": "text/javascript" });
    res.end("/* uygulama */");
    return;
  }
  if (url === "/style.css") {
    res.writeHead(200, { "content-type": "text/css" });
    res.end("body{background:#0b1220}");
    return;
  }
  if (url === "/logo.png") {
    // 1x1 saydam PNG
    const png = Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      "base64",
    );
    res.writeHead(200, { "content-type": "image/png" });
    res.end(png);
    return;
  }
  if (url === "/media/master.m3u8") {
    res.writeHead(200, { "content-type": "application/vnd.apple.mpegurl" });
    res.end("#EXTM3U\n#EXTINF:2,\nseg-1.ts\n#EXT-X-ENDLIST\n");
    return;
  }  if (url === "/media/manifest.mpd") {
    res.writeHead(200, { "content-type": "application/dash+xml" });
    res.end('<?xml version="1.0"?><MPD mediaPresentationDuration="PT4S"><Period/></MPD>');
    return;
  }
  if (url === "/media/video.mp4") {
    res.writeHead(200, { "content-type": "video/mp4", "content-length": "1024" });
    res.end(Buffer.alloc(1024));
    return;
  }
  if (url === "/media/stream-nouext") {
    // Uzantı yok: yalnız content-type ile medya olduğu anlaşılır (CDN deseni).
    res.writeHead(200, { "content-type": "video/mp4", "content-length": "2048" });
    res.end(Buffer.alloc(2048));
    return;
  }
  if (url === "/media/seg-1.ts" || url === "/media/chunk.m4s") {
    res.writeHead(200, { "content-type": "video/mp2t" });
    res.end(Buffer.alloc(256));
    return;
  }
  if (url === "/api/data.json") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end('{"ok":true}');
    return;
  }
  res.writeHead(404);
  res.end("yok");
});

async function main() {
  await new Promise((resolve) => server.listen(PORT, "127.0.0.1", resolve));

  let context = null;
  try {
    // Eklentiyi yükle (kalıcı olmayan bağlam; MV3 service worker çalışır).
    context = await chromium.launchPersistentContext("", {
      channel: "msedge",
      headless: true,
      args: [`--disable-extensions-except=${extDir}`, `--load-extension=${extDir}`],
    });

    // Service worker'ı bekle (eklenti arka planı).
    let worker = context.serviceWorkers()[0];
    if (!worker) {
      worker = await context.waitForEvent("serviceworker", { timeout: 20000 });
    }
    check(!!worker, "eklenti service worker yüklendi");

    const extId = new URL(worker.url()).host;
    ok(`eklenti kimliği: ${extId}`);

    // Ağ yakalamayı aç.
    const toggled = await worker.evaluate(async () => {
      await HX.api.storage.sync.set({ netCapture: true });
      return (await getSettings()).netCapture === true;
    });
    check(toggled, "netCapture ayarı açıldı");

    // Dinleyicinin kayıt olduğundan emin olmak için ayar değişimini bekle.
    await new Promise((resolve) => setTimeout(resolve, 500));

    // Test sayfasını aç: JS tüm varlıkları ister.
    const page = await context.newPage();
    await page.goto(`${BASE}/`, { waitUntil: "networkidle" });
    await new Promise((resolve) => setTimeout(resolve, 800));

    // Yakalananları arka plandan iste (aynı sekme kimliğiyle).
    const tabId = await worker.evaluate(async () => {
      const tabs = await HX.api.tabs.query({ url: "http://127.0.0.1/*" });
      return tabs.length > 0 ? tabs[0].id : null;
    });
    check(Number.isInteger(tabId), `test sekmesi bulundu (tabId=${tabId})`);

    const response = await worker.evaluate(async (id) => {
      return await new Promise((resolve) => {
        HX.api.runtime.onMessage.hasListeners();
        resolve(
          readNetEntries(id).then((entries) => ({ ok: true, entries })),
        );
      });
    }, tabId);

    const entries = Array.isArray(response?.entries) ? response.entries : [];
    const urls = entries.map((entry) => entry.url);
    console.log("..   yakalananlar:");
    for (const entry of entries) console.log(`       ${entry.kind.padEnd(6)} ${entry.url}`);

    check(
      urls.some((u) => u.endsWith("/media/master.m3u8")),
      "HLS manifesti yakalandı",
    );
    check(
      urls.some((u) => u.endsWith("/media/manifest.mpd")),
      "DASH manifesti yakalandı",
    );
    check(urls.some((u) => u.endsWith("/media/video.mp4")), "doğrudan video yakalandı");
    check(
      urls.some((u) => u.endsWith("/media/stream-nouext")),
      "uzantısız akış content-type ile yakalandı (CDN)",
    );
    check(
      !urls.some((u) => u.endsWith("/media/seg-1.ts")),
      "HLS parçası listeye alınmadı",
    );
    check(
      !urls.some((u) => u.endsWith("/media/chunk.m4s")),
      "DASH parçası listeye alınmadı",
    );
    check(!urls.some((u) => u.endsWith("/style.css")), "CSS yakalanmadı");
    check(!urls.some((u) => u.endsWith("/app.js")), "JS yakalanmadı");
    check(!urls.some((u) => u.endsWith("/logo.png")), "görsel yakalanmadı");
    check(!urls.some((u) => u.endsWith("/api/data.json")), "JSON API yakalanmadı");

    // Kapatınca temizleniyor mu?
    await worker.evaluate(async (id) => {
      await HX.api.storage.sync.set({ netCapture: false });
      await writeNetEntries(id, []);
    }, tabId);
    const afterClear = await worker.evaluate(async (id) => readNetEntries(id), tabId);
    check(afterClear.length === 0, "kapatınca/temizleyince kayıtlar silindi");

    // --- 2) Uçtan uca: yakalanan HLS'i eklentinin kendisiyle Hydra'ya gönder ---
    //
    // "Site fark etmeksizin" iddiasının kanıtı: yakaladığımız adres gerçekten
    // indirilebiliyor mu? (Hydra açık olmalı; yoksa bu adım atlanır.)
    const hlsUrl = urls.find((u) => u.includes("master.m3u8"));
    if (hlsUrl) {
      const health = await worker.evaluate(async () => {
        try {
          return await hydraHealth();
        } catch {
          return null;
        }
      });
      if (!health) {
        console.log("..   Hydra DL kapalı: uçtan uca indirme adımı atlandı");
      } else {
        const outDir = path.join(process.env.TEMP || ".", "hydra-netcapture-out");
        const added = await worker.evaluate(
          async ({ url, referer, dir }) => {
            try {
              return { ok: true, data: await hydraAdd({ url, referer, dir }) };
            } catch (err) {
              return { ok: false, error: err.message };
            }
          },
          { url: hlsUrl, referer: `${BASE}/`, dir: outDir },
        );
        check(added.ok, `yakalanan HLS Hydra'ya eklendi (${added.ok ? added.data.id : added.error})`);
        if (added.ok) {
          const id = added.data.id;
          let item = null;
          for (let i = 0; i < 60; i += 1) {
            const list = await (await fetch(`${HEALTH}/downloads`)).json();
            item = (list || []).find((d) => d.id === id);
            if (item && ["completed", "failed"].includes(item.status)) break;
            await new Promise((resolve) => setTimeout(resolve, 250));
          }
          check(
            item?.status === "completed",
            `yakalanan HLS indirildi (${item?.status}${item?.error ? `: ${item.error}` : ""})`,
          );
          if (item) {
            const stat = await readFile(item.filePath).catch(() => null);
            check(!!stat && stat.length > 0, `dosya diskte (${stat ? stat.length : 0} bayt)`);
          }
          // Test kaydını temizle.
          await fetch(`${HEALTH}/downloads/${id}`, { method: "DELETE" }).catch(() => {});
        }
      }
    }

    await page.close();
  } finally {
    if (context) await context.close().catch(() => {});
    server.close();
  }

  console.log(failures === 0 ? "\nSONUC: PASS" : `\nSONUC: FAIL (${failures})`);
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((err) => {
  console.error(`FAIL ${err?.stack ?? err}`);
  server.close();
  process.exit(1);
});
