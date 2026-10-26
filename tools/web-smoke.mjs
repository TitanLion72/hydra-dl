// Web sunucusu duman testi: health -> probe -> add -> SSE -> settings.
// Kullanim: node tools/web-smoke.mjs --port 6790 [--token abc]
import { createHash } from "node:crypto";
import http from "node:http";
import { mkdir, rm, stat } from "node:fs/promises";
import path from "node:path";

function arg(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

const port = Number(arg("port", "6790"));
const token = arg("token", "");
const base = `http://127.0.0.1:${port}`;
const targetDir = path.resolve(arg("dir", path.join(process.env.TEMP ?? ".", "hydra-web-smoke")));
const size = 4 * 1024 * 1024;

const payload = Buffer.alloc(size);
for (let i = 0; i < size; i += 1) payload[i] = (i * 7) % 251;
const expected = createHash("sha256").update(payload).digest("hex");

function headers(extra = {}) {
  return token ? { ...extra, "x-hydra-token": token } : extra;
}

async function call(pathname, init = {}) {
  const response = await fetch(`${base}${pathname}`, {
    ...init,
    headers: headers({ "content-type": "application/json", ...(init.headers ?? {}) }),
  });
  const text = await response.text();
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    json = text.slice(0, 200);
  }
  return { status: response.status, ok: response.ok, body: json };
}

function startFileServer() {
  const server = http.createServer((req, res) => {
    const range = req.headers.range;
    if (range) {
      const match = /bytes=(\d+)-(\d*)/.exec(range);
      if (match) {
        const start = Number(match[1]);
        const end = match[2] ? Math.min(Number(match[2]), size - 1) : size - 1;
        if (start > end) return res.writeHead(416).end();
        const chunk = payload.subarray(start, end + 1);
        res.writeHead(206, {
          "content-type": "application/octet-stream",
          "content-length": String(chunk.length),
          "content-range": `bytes ${start}-${end}/${size}`,
          "accept-ranges": "bytes",
          "content-disposition": 'attachment; filename="web-smoke.bin"',
        });
        return res.end(chunk);
      }
    }
    res.writeHead(200, {
      "content-type": "application/octet-stream",
      "content-length": String(size),
      "accept-ranges": "bytes",
      "content-disposition": 'attachment; filename="web-smoke.bin"',
    });
    res.end(payload);
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port })),
  );
}

function waitFor(description, timeoutMs, fn) {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const tick = async () => {
      try {
        const value = await fn();
        if (value) return resolve(value);
      } catch {
        // henuz hazir degil
      }
      if (Date.now() - started > timeoutMs) return reject(new Error(`Zaman aşımı: ${description}`));
      setTimeout(tick, 500);
    };
    void tick();
  });
}

function checkSse() {
  return new Promise((resolve, reject) => {
    const request = http.get(`${base}/events${token ? `?token=${token}` : ""}`, (res) => {
      if (res.statusCode !== 200) {
        reject(new Error(`SSE ${res.statusCode}`));
        return;
      }
      let buffer = "";
      res.on("data", (chunk) => {
        buffer += chunk.toString();
        if (buffer.includes("event: downloads") && buffer.includes("data: [")) {
          res.destroy();
          resolve(true);
        }
      });
      setTimeout(() => {
        res.destroy();
        reject(new Error("SSE olayı gelmedi"));
      }, 8000);
    });
    request.on("error", reject);
  });
}

async function main() {
  await rm(targetDir, { recursive: true, force: true });
  await mkdir(targetDir, { recursive: true });

  const health = await call("/health");
  if (!health.ok) throw new Error(`health ${health.status}: ${JSON.stringify(health.body)}`);
  console.log(`[web] health OK: ${health.body.app} v${health.body.version}`);

  const index = await fetch(`${base}/`);
  const html = await index.text();
  if (!index.ok || !html.includes("Hydra")) throw new Error("index.html servis edilemedi");
  console.log(`[web] index.html OK (${html.length} bayt)`);

  await checkSse();
  console.log("[web] SSE akışı OK");

  const { server, port: filePort } = await startFileServer();
  try {
    const probe = await call("/probe", {
      method: "POST",
      body: JSON.stringify({ url: `http://127.0.0.1:${filePort}/file`, media: false }),
    });
    if (!probe.ok) throw new Error(`probe ${probe.status}: ${JSON.stringify(probe.body)}`);
    if (probe.body.size !== size) throw new Error(`probe boyut ${probe.body.size} != ${size}`);
    console.log(
      `[web] probe OK: kind=${probe.body.kind} size=${probe.body.size} formats=${probe.body.formats?.length}`,
    );

    const added = await call("/add", {
      method: "POST",
      body: JSON.stringify({
        url: `http://127.0.0.1:${filePort}/file`,
        dir: targetDir,
      }),
    });
    if (!added.ok) throw new Error(`add ${added.status}: ${JSON.stringify(added.body)}`);
    console.log(`[web] add OK: ${added.body.filename}`);

    const target = path.join(targetDir, added.body.filename);
    await waitFor("indirme", 90_000, async () => {
      try {
        return (await stat(target)).size === size;
      } catch {
        return false;
      }
    });
    const hash = createHash("sha256").update(await import("node:fs/promises").then((m) => m.readFile(target))).digest("hex");
    if (hash !== expected) throw new Error(`hash uyuşmuyor: ${hash}`);
    console.log(`[web] indirme + hash OK: ${hash.slice(0, 16)}…`);

    const settings = await call("/settings");
    if (!settings.ok) throw new Error(`settings GET ${settings.status}`);
    console.log(`[web] settings OK: concurrent=${settings.body.maxConcurrent}`);

    const removed = await call(`/downloads/${added.body.id}?deleteFile=true`, { method: "DELETE" });
    if (!removed.ok) throw new Error(`delete ${removed.status}`);
    console.log("[web] temizlik OK");
  } finally {
    server.close();
  }
  console.log("[web] TÜMÜ GEÇTİ");
}

main()
  .then(() => process.exit(0))
  .catch((err) => {
    console.error(`[web] HATA: ${err.message}`);
    process.exit(1);
  });
