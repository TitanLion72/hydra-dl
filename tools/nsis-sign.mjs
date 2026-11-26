// NSIS kurulum paketini kodla imzalar (opsiyonel).
//
// Tauri `bundle.windows.signCommand` bu betigi cagirir ve imzalanacak dosyanin
// yolunu son arguman olarak verir. Sertifika tanimli **degilse** betik 0 ile
// cikar: imzasiz derleme bugun oldugu gibi calismaya devam eder.
//
// Gerekli ortam degiskenleri (biri):
//   HYDRA_SIGN_PFX        .pfx dosya yolu
//   HYDRA_SIGN_PFX_PASS   .pfx parolasi (PFX kullaniliyorsa)
// veya sertifika deposunda kurulu bir sertifika:
//   HYDRA_SIGN_THUMBPRINT sertifika parmak izi (40 hex)
// Opsiyonel:
//   HYDRA_SIGN_TIMESTAMP  zaman damgasi sunucusu
//                         (varsayilan: http://timestamp.digicert.com)
//
// Kullanim (elle deneme):
//   node tools/nsis-sign.mjs --check            # hazir mi?
//   node tools/nsis-sign.mjs "paket.exe"        # imzala
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";

const TIMESTAMP =
  process.env.HYDRA_SIGN_TIMESTAMP || "http://timestamp.digicert.com";

/** Imzalama icin gereken bilgi var mi? */
function config() {
  const pfx = process.env.HYDRA_SIGN_PFX?.trim();
  const thumbprint = process.env.HYDRA_SIGN_THUMBPRINT?.trim();
  if (pfx) {
    return {
      kind: "pfx",
      pfx,
      password: process.env.HYDRA_SIGN_PFX_PASS ?? "",
    };
  }
  if (thumbprint) {
    return { kind: "store", thumbprint };
  }
  return null;
}

/** `signtool.exe`'yi bulur: PATH, sonra bilinen Windows SDK yollari. */
function findSignTool() {
  const probe = spawnSync("where", ["signtool.exe"], { encoding: "utf8" });
  if (probe.status === 0) {
    return probe.stdout.split(/\r?\n/).find((line) => line.trim().length > 0)?.trim() ?? null;
  }
  // PATH'te yoksa: Windows Kits altindaki en yeni x64 surumu.
  const roots = [
    "C:\\Program Files (x86)\\Windows Kits\\10\\bin",
    "C:\\Program Files\\Windows Kits\\10\\bin",
  ];
  for (const root of roots) {
    if (!existsSync(root)) continue;
    const found = spawnSync("cmd", ["/c", `dir /b /s "${root}\\x64\\signtool.exe"`], {
      encoding: "utf8",
    });
    if (found.status !== 0) continue;
    const list = found.stdout
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
    if (list.length > 0) return list[list.length - 1];
  }
  return null;
}

function main() {
  const args = process.argv.slice(2);
  if (args.includes("--check")) {
    const cfg = config();
    const tool = findSignTool();
    if (!cfg) {
      console.log("[sign] sertifika tanimli degil → imzasiz derleme (normal)");
      return 0;
    }
    if (!tool) {
      console.log("[sign] HATA: sertifika tanimli ama signtool.exe bulunamadi");
      return 1;
    }
    if (cfg.kind === "pfx" && !existsSync(cfg.pfx)) {
      console.log(`[sign] HATA: .pfx dosyasi yok: ${cfg.pfx}`);
      return 1;
    }
    console.log(`[sign] hazir: ${cfg.kind} + signtool`);
    return 0;
  }

  const target = args[args.length - 1];
  const cfg = config();
  if (!cfg) {
    // Sertifika yok: sessizce gec (Tauri bunu hata saymasin).
    console.log("[sign] sertifika tanimli degil → imzalanmadi");
    return 0;
  }
  if (!target || !existsSync(target)) {
    console.log(`[sign] HATA: imzalanacak dosya yok: ${target ?? "(arguman yok)"}`);
    return 1;
  }
  const tool = findSignTool();
  if (!tool) {
    console.log("[sign] HATA: signtool.exe bulunamadi (Windows SDK kurulu mu?)");
    return 1;
  }

  const common = [
    "sign",
    "/fd",
    "SHA256",
    "/tr",
    TIMESTAMP,
    "/td",
    "SHA256",
  ];
  const command =
    cfg.kind === "pfx"
      ? [...common, "/f", cfg.pfx, "/p", cfg.password, target]
      : [...common, "/sha1", cfg.thumbprint, target];

  const result = spawnSync(tool, command, { stdio: "inherit" });
  if (result.status !== 0) {
    console.log(`[sign] HATA: signtool cikis kodu ${result.status}`);
    return 1;
  }
  console.log(`[sign] imzalandi: ${target}`);
  return 0;
}

process.exit(main());
