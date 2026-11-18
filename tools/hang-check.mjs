// Canli dogrulama: askida kalan sunucu artik zaman asimina ugramali.
//
// Sunucu baglanir, basliklari gonderir, govdeyi ASLA gondermez. Once (v0.6.3)
// indirme sonsuza kadar asili kaliyordu; v0.6.4'te ~30 sn'de hata vermeli.
import http from "node:http";
import { mkdir, readdir, rm } from "node:fs/promises";
import path from "node:path";

const capturePort = Number(process.env.HYDRA_PORT ?? 6712);
const targetDir = path.join(process.env.TEMP ?? ".", "hydra-hang-check");
const limitSeconds = Number(process.env.HANG_LIMIT ?? 150);

const server = http.createServer((req, res) => {
  const url = new URL(req.url ?? "/", "http://localhost");
  if (url.pathname !== "/hang") {
    res.writeHead(404).end("yok");
    return;
  }
  // Basliklar gonderilir, govde hic gelmez: klasik "askida kalan" sunucu.
  res.writeHead(200, { "content-type": "application/octet-stream", "content-length": "1048576" });
  res.flushHeaders?.();
  // Baglantiyi uzun sure acik tut.
  setTimeout(() => {
    try {
      res.end();
    } catch {
      // yoksay
    }
  }, 180_000);
});

/** Hydra'nin bu dizindeki kayitlarini okur. */
async function records() {
  const response = await fetch(`http://127.0.0.1:${capturePort}/downloads`);
  const list = await response.json();
  return list.filter((item) => typeof item.dir === "string" && item.dir.includes("hydra-hang-check"));
}

async function main() {
  await rm(targetDir, { recursive: true, force: true });
  await mkdir(targetDir, { recursive: true });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  console.log(`[hang] sunucu :${port}, hedef ${targetDir}`);

  const addUrl =
    `http://127.0.0.1:${capturePort}/add?url=` +
    encodeURIComponent(`http://127.0.0.1:${port}/hang`) +
    `&dir=${encodeURIComponent(targetDir)}`;
  const added = await (await fetch(addUrl)).json();
  if (!added.ok) throw new Error(`ekleme basarisiz: ${JSON.stringify(added)}`);
  console.log(`[hang] eklendi: ${added.filename}`);

  const started = Date.now();
  let final = null;
  let lastStatus = null;
  while (Date.now() - started < limitSeconds * 1000) {
    const mine = await records();
    const item = mine.find((entry) => entry.filename === added.filename);
    if (item && item.status !== lastStatus) {
      const t = ((Date.now() - started) / 1000).toFixed(1);
      console.log(`[hang] ${t} sn: ${item.status} · indirilen ${item.downloadedBytes}${item.error ? ` · hata: ${item.error.slice(0, 90)}` : ""}`);
      lastStatus = item.status;
    }
    if (item && (item.status === "failed" || item.status === "completed")) {
      final = item;
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }

  const elapsed = ((Date.now() - started) / 1000).toFixed(1);
  if (!final) {
    throw new Error(`${limitSeconds} sn icinde durum degismedi (hala asili)`);
  }
  if (final.status === "completed") {
    throw new Error(`yanlis: askida kalan sunucu "completed" olamaz (${elapsed} sn)`);
  }
  console.log(`[hang] ${elapsed} sn sonra durum: ${final.status}`);
  console.log(`[hang] hata mesaji: ${final.error ?? "(yok)"}`);

  // Yarim dosya birakilmamali (atomik bitis degismezi).
  const files = await readdir(targetDir);
  console.log(`[hang] hedef dizinde dosya: ${files.length === 0 ? "yok (dogru)" : files.join(", ")}`);

  // Temizlik: kayit + dizin.
  await fetch(`http://127.0.0.1:${capturePort}/downloads/${final.id}`, { method: "DELETE" }).catch(
    () => undefined,
  );
  await rm(targetDir, { recursive: true, force: true });
  console.log("[hang] BASARILI: zaman asimi calisiyor, kalinti yok");
}

main()
  .then(() => {
    server.close();
    process.exit(0);
  })
  .catch((error) => {
    console.error(`[hang] BASARISIZ: ${error instanceof Error ? error.message : String(error)}`);
    server.close();
    process.exit(1);
  });
