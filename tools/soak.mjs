// Dayaniklilik soak testi: N eszamanli indirme, hepsi bozulmadan bitmeli.
//
// Gercek uygulamaya (yakalama API'si) baglanir; her indirme kendi dosya adini ve
// icerigini tasir, boylece karisiklik (yanlis parca birlestirme) yakalanir.
//
// Kullanim:
//   node tools/soak.mjs --capture-port 6712 --count 10
//   node tools/soak.mjs --capture-port 6712 --count 20 --size 8
import http from "node:http";
import { createHash } from "node:crypto";
import { mkdir, readFile, rm, stat } from "node:fs/promises";
import path from "node:path";

function arg(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  return index !== -1 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

const capturePort = Number(arg("capture-port", "6712"));
const count = Number(arg("count", "10"));
// MB cinsinden; her dosya farkli boyutta olur (kucuk/orta/buyuk karisimi).
const maxMb = Number(arg("size", "4"));
const targetDir = path.resolve(arg("dir", path.join(process.env.TEMP ?? ".", "hydra-soak")));
const timeoutMs = Number(arg("timeout", "300000"));

function makePayload(seed, bytes) {
  const buffer = Buffer.alloc(bytes);
  // Her dosya icin farkli desen: yanlis parca karisirsa hash tutmaz.
  let value = seed * 7919 + 13;
  for (let i = 0; i < bytes; i += 1) {
    value = (value * 1103515245 + 12345) & 0x7fffffff;
    buffer[i] = (value >>> 16) & 0xff;
  }
  return buffer;
}

/** Sunucu: /file/<index> — her yol kendi icerigini servis eder. */
function startServer(payloads) {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://localhost");
    const match = /^\/file\/(\d+)$/.exec(url.pathname);
    if (!match) {
      res.writeHead(404).end("yok");
      return;
    }
    const payload = payloads[Number(match[1])];
    if (!payload) {
      res.writeHead(404).end("yok");
      return;
    }
    const total = payload.length;
    const range = req.headers.range;
    if (range) {
      const parts = /bytes=(\d+)-(\d*)/.exec(range);
      if (parts) {
        const start = Number(parts[1]);
        const end = parts[2] ? Math.min(Number(parts[2]), total - 1) : total - 1;
        if (start > end) {
          res.writeHead(416).end();
          return;
        }
        const chunk = payload.subarray(start, end + 1);
        res.writeHead(206, {
          "content-type": "application/octet-stream",
          "content-length": chunk.length,
          "content-range": `bytes ${start}-${end}/${total}`,
          "accept-ranges": "bytes",
        });
        res.end(chunk);
        return;
      }
    }
    res.writeHead(200, {
      "content-type": "application/octet-stream",
      "content-length": total,
      "accept-ranges": "bytes",
    });
    res.end(payload);
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port }));
  });
}

async function waitFor(description, timeout, fn) {
  const started = Date.now();
  let lastError = "";
  while (Date.now() - started < timeout) {
    try {
      const value = await fn();
      if (value) return value;
    } catch (error) {
      lastError = String(error);
    }
    await new Promise((resolve) => setTimeout(resolve, 400));
  }
  throw new Error(`${description}: zaman asimi${lastError ? ` (${lastError})` : ""}`);
}

async function main() {
  await rm(targetDir, { recursive: true, force: true });
  await mkdir(targetDir, { recursive: true });

  const payloads = [];
  const expected = [];
  for (let index = 0; index < count; index += 1) {
    // 256 KB ile maxMb MB arasinda degisen boyutlar: hem kucuk hem buyuk parca.
    const factor = 0.25 + (index % 5) * (Math.max(maxMb - 0.25, 0.25) / 4);
    const bytes = Math.max(64 * 1024, Math.round(factor * 1024 * 1024));
    const payload = makePayload(index + 1, bytes);
    payloads.push(payload);
    expected.push({
      index,
      bytes,
      hash: createHash("sha256").update(payload).digest("hex"),
    });
  }

  const { server, port } = await startServer(payloads);
  const started = Date.now();
  try {
    await waitFor("Hydra yakalama sunucusu", 90_000, async () => {
      const response = await fetch(`http://127.0.0.1:${capturePort}/health`);
      return response.ok;
    });
    console.log(`[soak] ${count} eszamanli indirme, en fazla ${maxMb} MB, dizin: ${targetDir}`);

    // Mevcut kayitlarin kimliklerini sakla: temizlikte yalniz yeniler silinsin.
    const beforeIds = new Set();
    try {
      const existing = await (await fetch(`http://127.0.0.1:${capturePort}/downloads`)).json();
      if (Array.isArray(existing)) {
        for (const entry of existing) beforeIds.add(entry.id);
      }
    } catch {
      // Liste alinamazsa temizlik yalniz yol eslesmesiyle yapilir.
    }

    const adds = await Promise.all(      expected.map(async (item) => {
        const url =
          `http://127.0.0.1:${capturePort}/add?url=` +
          encodeURIComponent(`http://127.0.0.1:${port}/file/${item.index}`) +
          `&dir=${encodeURIComponent(targetDir)}`;
        const response = await fetch(url);
        if (!response.ok) throw new Error(`add ${response.status}`);
        const body = await response.json();
        if (!body.ok) throw new Error(`Ekleme basarisiz: ${JSON.stringify(body)}`);
        return { ...item, filename: body.filename };
      }),
    );
    console.log("[soak] hepsi kuyruga eklendi");

    await waitFor(
      "tum indirmelerin tamamlanmasi",
      timeoutMs,
      async () => {
        try {
          await Promise.all(adds.map((item) => stat(path.join(targetDir, item.filename))));
          return true;
        } catch {
          return false;
        }
      },
    );

    // Her dosya birebir dogru mu? (yanlis parca birlestirme / karisma kontrolu)
    const problems = [];
    for (const item of adds) {
      const file = path.join(targetDir, item.filename);
      const info = await stat(file);
      const hash = createHash("sha256").update(await readFile(file)).digest("hex");
      if (info.size !== item.bytes) {
        problems.push(`${item.filename}: boyut ${info.size} != ${item.bytes}`);
      } else if (hash !== item.hash) {
        problems.push(`${item.filename}: sha256 uyusmuyor`);
      }
    }
    if (problems.length > 0) {
      throw new Error(`bozuk indirme(ler):\n  ${problems.join("\n  ")}`);
    }

    const seconds = ((Date.now() - started) / 1000).toFixed(1);
    const totalBytes = adds.reduce((sum, item) => sum + item.bytes, 0);
    console.log(
      `[soak] BASARILI: ${adds.length} dosya, ${(totalBytes / 1024 / 1024).toFixed(1)} MB, ${seconds} sn`,
    );

    // Temizlik: kayitlar kullanicinin gecmisine dusmesin, dosyalar diskte kalmasin.
    try {
      const list = await (await fetch(`http://127.0.0.1:${capturePort}/downloads`)).json();
      if (Array.isArray(list)) {
        const added = list.filter(
          (entry) =>
            !beforeIds.has(entry.id) &&
            typeof entry.filePath === "string" &&
            entry.filePath.includes(targetDir),
        );
        await Promise.all(
          added.map((entry) =>
            fetch(`http://127.0.0.1:${capturePort}/downloads/${entry.id}`, {
              method: "DELETE",
            }).catch(() => undefined),
          ),
        );
      }
    } catch {
      // Temizlik basarisiz olsa da soak sonucu gecerli.
    }
    for (const item of adds) {
      await rm(path.join(targetDir, item.filename), { force: true });
    }
  } finally {
    server.close();
  }
}

main().catch((error) => {
  console.error(`[soak] BASARISIZ: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
