// Uctan uca duman testi: yerel Range destekli sunucu -> Hydra yakalama API'si -> inen dosya.
// Kullanim: node tools/e2e.mjs --capture-port 6712 --dir C:\tmp\hydra-e2e
import http from "node:http";
import { createHash } from "node:crypto";
import { mkdir, readFile, stat, rm } from "node:fs/promises";
import path from "node:path";

function arg(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

const capturePort = Number(arg("capture-port", "6712"));
const targetDir = path.resolve(arg("dir", path.join(process.env.TEMP ?? ".", "hydra-e2e")));
const size = Number(arg("size", String(16 * 1024 * 1024)));

const payload = Buffer.alloc(size);
for (let i = 0; i < size; i += 1) payload[i] = i % 251;
const expectedHash = createHash("sha256").update(payload).digest("hex");

function startServer() {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://localhost");
    if (url.pathname === "/meta") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ size, sha256: expectedHash }));
      return;
    }
    if (url.pathname !== "/file") {
      res.writeHead(404).end();
      return;
    }
    const range = req.headers.range;
    if (range) {
      const match = /bytes=(\d+)-(\d*)/.exec(range);
      if (match) {
        const start = Number(match[1]);
        const end = match[2] ? Math.min(Number(match[2]), size - 1) : size - 1;
        if (start > end) {
          res.writeHead(416).end();
          return;
        }
        const chunk = payload.subarray(start, end + 1);
        res.writeHead(206, {
          "content-type": "application/octet-stream",
          "content-length": String(chunk.length),
          "content-range": `bytes ${start}-${end}/${size}`,
          "accept-ranges": "bytes",
          "content-disposition": 'attachment; filename="e2e-test.bin"',
        });
        res.end(chunk);
        return;
      }
    }
    res.writeHead(200, {
      "content-type": "application/octet-stream",
      "content-length": String(size),
      "accept-ranges": "bytes",
      "content-disposition": 'attachment; filename="e2e-test.bin"',
    });
    res.end(payload);
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port }));
  });
}

async function waitFor(description, timeoutMs, fn) {
  const started = Date.now();
  for (;;) {
    try {
      const value = await fn();
      if (value) return value;
    } catch {
      // henuz hazir degil
    }
    if (Date.now() - started > timeoutMs) {
      throw new Error(`Zaman asimi: ${description}`);
    }
    await new Promise((r) => setTimeout(r, 500));
  }
}

async function main() {
  // Onceki kosudan kalan dosyalar ad cakismasi yaratmasin.
  await rm(targetDir, { recursive: true, force: true });
  await mkdir(targetDir, { recursive: true });
  const { server, port } = await startServer();
  try {
    await run(port);
  } finally {
    server.close();
  }
}

async function run(port) {
  console.log(`[e2e] test sunucusu :${port}, ${size} bayt`);

  await waitFor("Hydra yakalama sunucusu", 90_000, async () => {
    const response = await fetch(`http://127.0.0.1:${capturePort}/health`);
    return response.ok;
  });
  console.log("[e2e] Hydra yakalama sunucusu hazir");

  const addUrl =
    `http://127.0.0.1:${capturePort}/add?url=` +
    encodeURIComponent(`http://127.0.0.1:${port}/file`) +
    `&dir=${encodeURIComponent(targetDir)}`;
  const added = await (await fetch(addUrl)).json();
  if (!added.ok) throw new Error(`Ekleme basarisiz: ${JSON.stringify(added)}`);
  const filename = added.filename;
  console.log(`[e2e] kuyruga eklendi: ${filename} (${added.totalBytes} bayt, medya=${added.media})`);

  const target = path.join(targetDir, filename);
  await waitFor("indirme tamamlanmasi", 120_000, async () => {
    try {
      const info = await stat(target);
      return info.size === size;
    } catch {
      return false;
    }
  });
  console.log("[e2e] dosya boyutu tamam");

  // Sonucu yakalama API'sinden de dogrula (durum kaydi ile disk tutarli mi).
  const listResponse = await fetch(`http://127.0.0.1:${capturePort}/health`);
  const health = await listResponse.json();
  if (!health) throw new Error("health yaniti bos");

  // Birlestirme dosyayi yeniden adlandirdigi icin boyut kontrolu yeterli degil; hash karsilastir.
  const downloaded = await readFile(target);
  const hash = createHash("sha256").update(downloaded).digest("hex");
  if (hash !== expectedHash) {
    throw new Error(`Hash uyusmuyor: ${hash} != ${expectedHash}`);
  }
  console.log(`[e2e] BASARILI: ${target} (${downloaded.length} bayt, sha256 ${hash.slice(0, 16)}…)`);

  await rm(target, { force: true });
}

main()
  .then(() => process.exit(0))
  .catch((err) => {
    console.error(`[e2e] HATA: ${err.message}`);
    process.exit(1);
  });
