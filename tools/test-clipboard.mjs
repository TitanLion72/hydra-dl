// Pano izleme saf mantığı testleri.
import { pickClipboardLink, shouldFillLink } from "../src/clipboard-watch.ts";

let pass = 0;
let fail = 0;
const ok = (name) => {
  pass += 1;
  console.log(`OK   ${name}`);
};
const bad = (name, extra = "") => {
  fail += 1;
  console.error(`FAIL ${name}${extra ? ` — ${extra}` : ""}`);
};
const eq = (name, actual, expected) => {
  if (Object.is(actual, expected)) ok(name);
  else bad(name, `beklenen ${JSON.stringify(expected)}, gelen ${JSON.stringify(actual)}`);
};

// --- panodan baglanti secimi ------------------------------------------------
eq("tek http baglantisi secilir", pickClipboardLink("https://ornek.test/a.zip"), "https://ornek.test/a.zip");
eq("bosluklar kirpilir", pickClipboardLink("  https://ornek.test/a.zip  \n"), "https://ornek.test/a.zip");
eq("magnet kabul edilir", pickClipboardLink("magnet:?xt=urn:btih:abc"), "magnet:?xt=urn:btih:abc");
eq("bos pano yok sayilir", pickClipboardLink(""), null);
eq("sadece bosluk yok sayilir", pickClipboardLink("   \n\t "), null);
eq("duz metin yok sayilir", pickClipboardLink("merhaba dunya"), null);
eq("dosya yolu yok sayilir", pickClipboardLink("C:\\Users\\ornek\\a.zip"), null);
eq("coklu baglanti yok sayilir", pickClipboardLink("https://a.test/1.zip\nhttps://a.test/2.zip"), null);
eq("baglanti + aciklama yok sayilir", pickClipboardLink("indir: https://a.test/1.zip hemen"), null);
eq("parantezli baglanti temizlenir", pickClipboardLink("(https://ornek.test/a.zip)"), "https://ornek.test/a.zip");

// --- alan doldurma karari ---------------------------------------------------
eq("bos alana doldurulur", shouldFillLink("https://a.test/1.zip", null, ""), true);
eq("ayni baglanti iki kez doldurulmaz", shouldFillLink("https://a.test/1.zip", "https://a.test/1.zip", ""), false);
eq("kullanici yazmisssa dokunulmaz", shouldFillLink("https://a.test/1.zip", null, "https://elle.test/x"), false);
eq("baglanti yoksa doldurulmaz", shouldFillLink(null, null, ""), false);
eq("bos alan + bosluk doldurulur", shouldFillLink("https://a.test/1.zip", null, "   "), true);
eq("onceki otomatik doldurma ezilebilir", shouldFillLink("https://b.test/2.zip", "https://a.test/1.zip", "https://a.test/1.zip"), true);

console.log(`\nSONUC: ${fail === 0 ? "PASS" : `FAIL (${fail})`} (${pass} geçti, ${fail} kaldı)`);
process.exit(fail === 0 ? 0 : 1);
