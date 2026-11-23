// Zamanlanmis indirme penceresi testleri.
import { inWindow, parseClock, validWindow, windowLabel } from "../src/schedule.ts";

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

// --- saat ayristirma --------------------------------------------------------
eq("00:00 cozulur", parseClock("00:00"), 0);
eq("02:30 cozulur", parseClock("02:30"), 150);
eq("23:59 cozulur", parseClock("23:59"), 1439);
eq("bos saat null", parseClock(""), null);
eq("gecersiz saat null", parseClock("24:00"), null);
eq("gecersiz dakika null", parseClock("12:70"), null);
eq("harf null", parseClock("abc"), null);
eq("bosluklu saat kabul", parseClock(" 07:05 "), 425);

// --- pencere gecerliligi ----------------------------------------------------
eq("bos pencere gecerli (her zaman)", validWindow("", ""), true);
eq("tek uclu pencere gecersiz", validWindow("02:00", ""), false);
eq("ayni saat gecersiz", validWindow("02:00", "02:00"), false);
eq("gecerli pencere", validWindow("02:00", "06:00"), true);
eq("gece yarisini asan pencere", validWindow("23:00", "05:00"), true);

// --- pencere icinde mi ------------------------------------------------------
eq("bos pencere: her zaman acik", inWindow(600, "", ""), true);
eq("pencere icinde", inWindow(180, "02:00", "06:00"), true);
eq("baslangic dahil", inWindow(120, "02:00", "06:00"), true);
eq("bitis haric", inWindow(360, "02:00", "06:00"), false);
eq("pencere oncesi kapali", inWindow(60, "02:00", "06:00"), false);
eq("gece yarisini asan: 23:30 acik", inWindow(1410, "23:00", "05:00"), true);
eq("gece yarisini asan: 02:00 acik", inWindow(120, "23:00", "05:00"), true);
eq("gece yarisini asan: 12:00 kapali", inWindow(720, "23:00", "05:00"), false);
eq("gecersiz pencere: her zaman acik", inWindow(600, "02:00", ""), true);

// --- etiket -----------------------------------------------------------------
eq("bos pencere etiketi", windowLabel("", ""), "");
eq("etiket bicimi", windowLabel("02:00", "06:00"), "02:00–06:00");

console.log(`\nSONUC: ${fail === 0 ? "PASS" : `FAIL (${fail})`} (${pass} geçti, ${fail} kaldı)`);
process.exit(fail === 0 ? 0 : 1);
