// Toplu içe aktarma ayrıştırıcısı — birim testleri.
//
// Kanonik kaynak `webapp/links.js`; masaüstü (`src/links.ts`) ve telefon
// (`webapp/app.js`) aynı dosyayı kullanır. Burada davranışı sabitliyoruz:
// satır/ayırıcı tanıma, yinelenen eleme, noktalama temizliği, şema reddi.

import { parseLinkList, trimLink, isSupportedLink, MAX_LINKS } from "../webapp/links.js";

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
function eq(name, actual, expected) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) ok(name);
  else bad(name, `beklenen ${e}, gelen ${a}`);
}
function check(name, cond, extra = "") {
  if (cond) ok(name);
  else bad(name, extra);
}

// --- temel: satır satır -----------------------------------------------------
eq(
  "her satır bir bağlantı",
  parseLinkList("https://a.example/1.zip\nhttps://b.example/2.zip").links,
  ["https://a.example/1.zip", "https://b.example/2.zip"],
);

eq(
  "boş satırlar yok sayılır",
  parseLinkList("\n\nhttps://a.example/1.zip\n\n").links,
  ["https://a.example/1.zip"],
);

eq(
  "tek satırda virgülle ayrılmış üç bağlantı",
  parseLinkList("https://a/1, https://b/2,https://c/3").links,
  ["https://a/1", "https://b/2", "https://c/3"],
);

eq(
  "aynı satırda boşlukla ayrılmış bağlantılar",
  parseLinkList("https://a/1 https://b/2").links,
  ["https://a/1", "https://b/2"],
);

// --- sarmal ve noktalama ----------------------------------------------------
eq(
  "açılı parantez sarmalı temizlenir",
  parseLinkList("<https://a.example/dosya.zip>").links,
  ["https://a.example/dosya.zip"],
);
eq("<sarmal> reddedilen sayılmaz", parseLinkList("<https://a/1>").rejected, []);

eq("sondaki nokta temizlenir", parseLinkList("https://a.example/x.zip.").links, [
  "https://a.example/x.zip",
]);
eq("tırnak içindeki bağlantı", parseLinkList('"https://a.example/x.zip"').links, [
  "https://a.example/x.zip",
]);

// --- dengeli parantez korunur (Wikipedia tarzı adres) -----------------------
eq(
  "dengeli parantez URL'nin parçası",
  parseLinkList("https://tr.wikipedia.org/wiki/Foo_(bar)").links,
  ["https://tr.wikipedia.org/wiki/Foo_(bar)"],
);
eq(
  "kapanmamış parantez atılır",
  parseLinkList("(https://a.example/x.zip)").links,
  ["https://a.example/x.zip"],
);

// --- magnet -----------------------------------------------------------------
eq(
  "magnet bağlantısı kabul edilir",
  parseLinkList("magnet:?xt=urn:btih:abc123&dn=ornek").links,
  ["magnet:?xt=urn:btih:abc123&dn=ornek"],
);

// --- yinelenen --------------------------------------------------------------
{
  const res = parseLinkList("https://a/1\nhttps://a/1\nhttps://b/2");
  eq("yinelenen elenir", res.links, ["https://a/1", "https://b/2"]);
  eq("yinelenen sayısı bildirilir", res.duplicates, 1);
}

// --- reddedilenler ----------------------------------------------------------
{
  const res = parseLinkList("https://a/1\nbu bir bağlantı değil\nftp://eski/1");
  eq("geçerli olan listede", res.links, ["https://a/1"]);
  check(
    "bağlantı olmayan satır reddedilende",
    res.rejected.some((r) => r.includes("bu bir bağlantı değil")),
    JSON.stringify(res.rejected),
  );
  check(
    "desteklenmeyen şema reddedilende",
    res.rejected.some((r) => r.includes("ftp://eski/1")),
    JSON.stringify(res.rejected),
  );
  check(
    "javascript: şeması listeye girmez",
    !parseLinkList("javascript:alert(1)").links.length,
  );
}

// --- sınır ------------------------------------------------------------------
{
  const many = Array.from({ length: MAX_LINKS + 5 }, (_, i) => `https://a/${i}`).join("\n");
  const res = parseLinkList(many);
  eq("sınırda durur", res.links.length, MAX_LINKS);
  eq("fazlası truncated olarak bildirilir", res.truncated, 5);
}

// --- boş girdi --------------------------------------------------------------
eq("boş metin", parseLinkList("").links, []);
eq("null güvenli", parseLinkList(null).links, []);
eq("boşluk güvenli", parseLinkList("   \n  ").links, []);

// --- yardımcılar ------------------------------------------------------------
eq("trimLink baştaki parantez", trimLink("(https://a/1"), "https://a/1");
check("isSupportedLink http", isSupportedLink("https://a/1"));
check("isSupportedLink magnet", isSupportedLink("magnet:?xt=urn:btih:x"));
check("isSupportedLink file değil", !isSupportedLink("file:///C:/x"));

// --- satır içi ek metin: bağlantı bulunur ----------------------------------
eq(
  "metnin içindeki bağlantı çıkarılır",
  parseLinkList("indir: https://a.example/x.zip (12 MB)").links,
  ["https://a.example/x.zip"],
);

console.log(`\nSONUC: ${fail === 0 ? "PASS" : `FAIL (${fail})`} (${pass} geçti, ${fail} kaldı)`);
process.exit(fail === 0 ? 0 : 1);
