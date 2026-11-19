# Otomatik Güncelleme — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Uygulama açılışta imzalı bir manifestten yeni sürümü öğrensin, kullanıcı onayıyla
indirip kursun; sürüm yayınlamak tek komut olsun.

**Architecture:** `tauri-plugin-updater` 2.14.1 imza doğrulama + NSIS kurulumunu yapar.
Biz üç şeyi yazarız: (1) saf, test edilebilir karar mantığı (`src/updater-logic.ts`),
(2) eklentiyi saran ince çalışma zamanı (`src/updater.ts`), (3) tek komutluk yayın aracı
(`tools/release.mjs`). Kurulumdan önce uygulama temiz kapanır (duraklat → kaydet → torrent
kapat) ve açılışta kaldığı yerden devam eder.

**Tech Stack:** Tauri 2, Rust, `tauri-plugin-updater` 2.14.1, `@tauri-apps/plugin-updater`
2.14.1, vanilla TS, GitHub Releases, Node ESM araçları.

**Spec:** `docs/superpowers/specs/2026-11-19-auto-update-design.md`

## Global Constraints

- Platform Windows x64, NSIS (`bundle.targets: ["nsis"]`), paket adı **boşluk içerir**:
  `Hydra DL_<sürüm>_x64-setup.exe` → URL'de `%20`.
- Bağımlılık sürümleri **tam sabit**: `tauri-plugin-updater = "2.14.1"`,
  `"@tauri-apps/plugin-updater": "2.14.1"`. Gevşek `"2"` yasak (uzaktan kod çalıştırma yolu).
- Anahtar: `~/.tauri/hydra-dl-updater.key` (parolalı, repoya girmez) +
  `~/.tauri/hydra-dl-updater.key.pub` → `tauri.conf.json → plugins.updater.pubkey`.
- Env değişkenleri (Tauri 2 adları): `TAURI_SIGNING_PRIVATE_KEY` (dosya **içeriği**),
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. `..._PATH` **v1 adıdır, kullanılmaz**.
- Yeni ayar alanı `updateCheck`, **varsayılan `true`**, `#[serde(default = ...)]`.
- Kullanıcı onayı zorunlu: onaysız indirme/kurulum yok.
- Arayüz metinleri **Türkçe**; her yeni metin için `webapp/i18n-en.js`'e karşılık gerekir
  (`node tools/audit-i18n.mjs` kırmızı olur yoksa).
- Mevcut değişmezler: parçalar `parts/<id>/<index>.part`, `.hydra-tmp` atomik bitiş,
  `downloads.json` geriye uyumlu, token varsayılan korumalı.
- Bitiş tanımı: `cargo test` + clippy `-D warnings` + fmt + `npm run build` +
  `test-i18n` + `security-check` + `test-updater` yeşil.

## Review Focus

Spec'in sessiz kaldığı ama kullanıcıyı ısıracak beş durum — her biri ilgili göreve test
olarak eklenmiştir:

1. **Release hiç yokken açılış** (bugünkü gerçek durum: 0 release) — uygulama hata
   göstermemeli, yavaşlamamalı. → Task 6.
2. **`updateCheck = false`** — hiçbir ağ çağrısı gitmemeli (masaüstü + telefon). → Task 6.
3. **Kurulum sessizce başarısız** (antivirüs/izin) — sonraki açılışta kırmızı şerit
   "güncelleme tamamlanmadı". → Task 6, Task 5.
4. **Bozuk/uyumsuz imza** — indirme reddedilmeli, ikili çalıştırılmamalı, şerit hata
   göstermeli. → Task 5, Task 8.
5. **Güncelleme sırasında süren indirme** — duraklatılmalı, parçalar korunmalı, açılışta
   devam etmeli. → Task 7.

---

### Task 1: İmza anahtarı (bootstrap, kullanıcı katılımlı)

**Files:**
- Create: `~/.tauri/hydra-dl-updater.key`, `~/.tauri/hydra-dl-updater.key.pub`
  (repo dışı — commit edilmez)

**Interfaces:**
- Produces: pubkey metni (Task 2'de `tauri.conf.json`'a girer)

- [ ] **Step 1: Anahtarı üret**

```powershell
npx tauri signer generate -w "$env:USERPROFILE\.tauri\hydra-dl-updater.key"
```

Parola sorulur: kullanıcı belirler, **kaydeder** (release her seferinde soracak).

- [ ] **Step 2: Public key'i not al**

```
Get-Content "$env:USERPROFILE\.tauri\hydra-dl-updater.key.pub"
```

Çıktı `dW50cnVzdGVkIGNvbW1lbnQ6...` biçiminde tek satır base64. Tamamı Task 2'ye girer.

- [ ] **Step 3: Yedeği doğrula (atlanamaz)**

```powershell
Get-Item "$env:USERPROFILE\.tauri\hydra-dl-updater.key" | Select-Object Length, LastWriteTime
```

Kullanıcıya sorulur: bu dosya şifreli bir yere kopyalandı mı? Cevap **evet** olmadan
Task 2'ye geçilmez — kaybedilen anahtar eski istemcilerdeki güncelleme zincirini koparır.

---

### Task 2: Eklenti iskeleti (davranış yok, derleme yeşil)

**Files:**
- Modify: `src-tauri/Cargo.toml` (dependencies)
- Modify: `package.json` (dependencies)
- Modify: `src-tauri/src/lib.rs:27-30` (plugin kaydı)
- Modify: `src-tauri/tauri.conf.json` (`bundle`, yeni `plugins`)
- Modify: `src-tauri/capabilities/default.json` (izin)

**Interfaces:**
- Produces: `tauri_plugin_updater` çalışma zamanı kayıtlı; `createUpdaterArtifacts` açık;
  `updater:default` izni. JS tarafında `@tauri-apps/plugin-updater` import edilebilir.

- [ ] **Step 1: Rust bağımlılığını ekle**

`src-tauri/Cargo.toml`, `tauri-plugin-dialog` satırının altına:

```toml
tauri-plugin-updater = "2.14.1"
```

- [ ] **Step 2: npm bağımlılığını ekle**

```powershell
npm.cmd install "@tauri-apps/plugin-updater@2.14.1" --save-exact
```

- [ ] **Step 3: Plugin'i kaydet**

`src-tauri/src/lib.rs`, `.plugin(tauri_plugin_dialog::init())` satırının altına:

```rust
.plugin(tauri_plugin_updater::Builder::new().build())
```

- [ ] **Step 4: `tauri.conf.json` — paketleme + endpoint**

`bundle` içine `"createUpdaterArtifacts": true` ekle. Kök düzeyde yeni `plugins`:

```json
"plugins": {
  "updater": {
    "pubkey": "<Task 1 Step 2 çıktısı, tek satır>",
    "endpoints": [
      "https://github.com/TitanLion72/hydra-dl/releases/latest/download/latest.json"
    ]
  }
}
```

- [ ] **Step 5: İzin ver**

`src-tauri/capabilities/default.json`, `"dialog:default"` satırının altına:
`"updater:default"`.

- [ ] **Step 6: Derlemenin geçtiğini doğrula**

```powershell
cargo check --manifest-path src-tauri\Cargo.toml
npm.cmd run build
```

Beklenen: ikisi de hatasız. Davranış değişmedi.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock package.json package-lock.json \
        src-tauri/src/lib.rs src-tauri/tauri.conf.json src-tauri/capabilities/default.json
git commit -m "feat(updater): tauri-plugin-updater 2.14.1 iskeleti"
```

---

### Task 3: `Settings.updateCheck` (varsayılan açık)

**Files:**
- Modify: `src-tauri/src/store.rs` (struct ~:50, `Default` ~:52-76, tests)
- Modify: `src/api.ts` (`Settings` tipi, `proxyUrl`/`language` yanı)
- Modify: `docs/API.md` (Settings tablosu)

**Interfaces:**
- Produces: `Settings.updateCheck: bool` (JSON `updateCheck`), varsayılan `true`;
  eski `downloads.json` dosyalarında alan yoksa `true` okunur. Export/import serde
  üzerinden kendiliğinden çalışır (ek alan listesi yok).

- [ ] **Step 1: Başarısız testi yaz**

`src-tauri/src/store.rs`, `dil_yalniz_desteklenen_degerleri_kabul_eder` testinin yanına:

```rust
#[test]
fn guncelleme_kontrolu_varsayilan_acik() {
    assert!(Settings::default().update_check, "varsayilan acik olmali");
}

#[test]
fn eski_ayar_dosyasinda_guncelleme_kontrolu_acik() {
    // `updateCheck` alani olmayan eski downloads.json okunabilmeli.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("downloads.json"),
        r#"{"version":1,"settings":{"maxConcurrent":4},"items":[]}"#,
    )
    .unwrap();
    let store = Store::load(dir.path());
    assert!(store.settings.update_check, "eski dosyada varsayilan acik");
}

#[test]
fn guncelleme_kontrolu_kapatilip_acilabilir() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::load(dir.path());
    store.settings.update_check = false;
    store.save().unwrap();
    assert!(!Store::load(dir.path()).settings.update_check, "kapali kalici");
}
```

- [ ] **Step 2: Testi koş, kırmızı olduğunu gör**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml guncelleme_kontrolu
```

Beklenen: FAIL — `no field update_check`.

- [ ] **Step 3: Alanı ekle**

`Settings` struct'ına (`language` alanının altına):

```rust
/// Acilista yeni surum var mi diye sessizce kontrol et.
#[serde(default = "default_true")]
pub update_check: bool,
```

Dosyada `default_true` yardımcısı yoksa ekle:

```rust
fn default_true() -> bool { true }
```

`Default for Settings` içine `update_check: true,`.

- [ ] **Step 4: Testi koş, yeşil olduğunu gör**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml guncelleme_kontrolu
cargo test --manifest-path src-tauri\Cargo.toml eski_ayar_dosyasinda_guncelleme
```

Beklenen: PASS (3 test).

- [ ] **Step 5: TS tipini ve API sözleşmesini güncelle**

`src/api.ts` `Settings` tipine (dil alanının yanına):

```ts
  /** Açılışta otomatik güncelleme kontrolü. */
  updateCheck: boolean;
```

`docs/API.md`, `Settings` tablosuna dil satırının altına:

```
| `updateCheck` | Açılışta otomatik güncelleme kontrolü (varsayılan `true`). Eski ayar dosyalarında alan yoksa `true` kabul edilir |
```

- [ ] **Step 6: Tam paketi koş**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml
npm.cmd run build
```

Beklenen: hepsi yeşil (`npm run build` `Settings` tipini kullanan yerlerde hata vermemeli;
`main.ts` `...settings` yayılımı kullandığı için ek alan zorunlu değil).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/store.rs src/api.ts docs/API.md
git commit -m "feat(settings): updateCheck alani (varsayilan acik)"
```

---

### Task 4: Ayar düğmesi — masaüstü + telefon

**Files:**
- Modify: `index.html` (Bildirim satırının yanına, `#set-notify` ~:152)
- Modify: `src/main.ts` (`bindSettingsForm` ~:1480, kaydetme ~:1566, `SETTING_LABELS` ~:1809)
- Modify: `webapp/index.html` (`#setNotify` ~:309)
- Modify: `webapp/app.js` (`els` ~:79, `applySettings` ~:1408, kaydetme ~:1699)
- Modify: `webapp/i18n-en.js`
- Modify: `tools/ui-harness/mock-api.mjs` (`SETTINGS` ~:13)
- Modify: `tools/web-ui-check.mjs` (settings mock ~:245)
- Test: `tools/ui-harness/run.mjs`, `tools/web-ui-check.mjs`

**Interfaces:**
- Consumes: `Settings.updateCheck` (Task 3).
- Produces: `#set-update-check` (masaüstü) ve `#setUpdateCheck` (telefon) onay kutuları;
  `SETTING_LABELS.updateCheck = "güncelleme kontrolü"`.

- [ ] **Step 1: Masaüstü işaretlemesini ekle**

`index.html`, `#set-notify` satırını içeren `<label>`'ın **hemen altına** (aynı
`field-group` içinde, aynı biçimde):

```html
<label class="check">
  <input id="set-update-check" type="checkbox" />
  <span>Güncellemeleri otomatik kontrol et</span>
</label>
```

- [ ] **Step 2: Masaüstü bağla**

`src/main.ts`, `const setNotify = $<HTMLInputElement>("#set-notify");` satırının altına
`const setUpdateCheck = $<HTMLInputElement>("#set-update-check");`

`bindSettingsForm` içinde `setNotify.checked = source.notifyOnFinish;` altına
`setUpdateCheck.checked = source.updateCheck;`

Kaydetme nesnesinde `notifyOnFinish: setNotify.checked,` altına
`updateCheck: setUpdateCheck.checked,`

`SETTING_LABELS` içine `updateCheck: "güncelleme kontrolü",`

- [ ] **Step 3: Telefon işaretlemesini ekle**

`webapp/index.html`, `#setNotify` satırını içeren `switch-row` `<label>`'ının hemen altına:

```html
<label class="switch-row">
  <span class="switch-copy">
    <span class="field-label">Güncellemeleri otomatik kontrol et</span>
    <span class="field-help">Açılışta yeni sürüm var mı diye sessizce bakar. Kapalıysa hiçbir ağ isteği gitmez.</span>
  </span>
  <input id="setUpdateCheck" class="switch" type="checkbox" role="switch" />
</label>
```

- [ ] **Step 4: Telefon bağla**

`webapp/app.js` `els` içine (`setNotify` yanına) `setUpdateCheck: $("#setUpdateCheck"),`

`applySettings` içine (`setNotify` satırının altına)
`if (els.setUpdateCheck) els.setUpdateCheck.checked = data.updateCheck !== false;`

Kaydetmede (`setNotify` satırının altına)
`if (els.setUpdateCheck) next.updateCheck = els.setUpdateCheck.checked;`

`SETTING_LABELS` içine `updateCheck: "güncelleme kontrolü",`

- [ ] **Step 5: İngilizce karşılıkları ekle**

`webapp/i18n-en.js` sözlüğüne:

```js
  "Güncellemeleri otomatik kontrol et": "Check for updates automatically",
  "Açılışta yeni sürüm var mı diye sessizce bakar. Kapalıysa hiçbir ağ isteği gitmez.":
    "Silently checks for a new version at startup. When off, no network request is made.",
  "güncelleme kontrolü": "update check",
```

```powershell
node tools/audit-i18n.mjs
```

Beklenen: `eksik: 0`, `tekrar: 0`.

- [ ] **Step 6: Mock'lara alanı ekle**

`tools/ui-harness/mock-api.mjs` `SETTINGS` nesnesine `updateCheck: true,`
`tools/web-ui-check.mjs` settings yanıtına `updateCheck: true,`

- [ ] **Step 7: Masaüstü testi yaz (ui-harness)**

`tools/ui-harness/run.mjs`, `#set-notify mock ayariyla uyumlu` kontrolünün altına:

```js
check(
  (await page.locator("#set-update-check").isChecked()) === true,
  "#set-update-check varsayilan acik",
);
```

ve settings kaydetme doğrulamalarına (`saved.notifyOnFinish` satırının altına):

```js
check(saved.updateCheck === true, "kaydetme guncelleme kontrolunu korudu");
```

- [ ] **Step 8: Telefon testi yaz (web-ui-check)**

`tools/web-ui-check.mjs`, `#setNotify` kontrolünün altına:

```js
check(await page.locator("#setUpdateCheck").isChecked(), "telefonda güncelleme kontrolü açık geliyor");
await page.locator("#setUpdateCheck").uncheck();
await page.click("#saveSettings");
// kaydedilen gövdede false gittiğini doğrula
```

(kaydedilen gövdeyi yakalamak için dosyadaki mevcut `PUT /settings` route kaydını kullan;
gövde `updateCheck` alanını içermeli.)

- [ ] **Step 9: Testleri koş**

```powershell
node tools/ui-harness/run.mjs
node tools/web-ui-check.mjs
```

Beklenen: ikisi de PASS.

- [ ] **Step 10: Commit**

```bash
git add index.html src/main.ts webapp/index.html webapp/app.js webapp/i18n-en.js \
        tools/ui-harness/mock-api.mjs tools/ui-harness/run.mjs tools/web-ui-check.mjs
git commit -m "feat(settings): guncelleme kontrolu anahtari (masaustu + telefon)"
```

---

### Task 5: `src/updater-logic.ts` — saf karar mantığı

**Files:**
- Create: `src/updater-logic.ts`
- Test: `tools/test-updater.mjs`

**Interfaces:**
- Produces (tam imzalar — Task 6 ve 7 bunları kullanır):

```ts
export type Storage = { getItem(k: string): string | null; setItem(k: string, v: string): void; removeItem(k: string): void };

/** Kullanıcı "Sonra" dediği sürüm. */
export function readSkipped(storage: Storage): string | null;
export function writeSkipped(storage: Storage, version: string): void;

/** Kurulum başlatıldı ama tamamlanmadı mı? (açılışta kontrol) */
export function readPending(storage: Storage): string | null;
export function writePending(storage: Storage, version: string): void;
export function clearPending(storage: Storage): void;

/** Şerit gösterilmeli mi? ("Sonra" denmiş sürüm için hayır) */
export function shouldPrompt(version: string, skipped: string | null): boolean;

/** İndirme yüzdesi; toplam bilinmiyorsa null. */
export function percent(downloaded: number, total: number | null): number | null;

/**
 * Kurulum başarısız mı? İşaret varsa ve uygulama hâlâ **eski** sürümdeyse
 * (yani yükseltme gerçekleşmediyse) `true`. Sürüm işaretli sürüme ulaştıysa
 * kurulum başarılıdır → `false` (ve çağıran `clearPending` yapmalı).
 */
export function installLooksFailed(pending: string | null, currentVersion: string): boolean;
```

- [ ] **Step 1: Başarısız testi yaz**

`tools/test-updater.mjs` (yeni dosya; `tools/test-i18n.mjs` biçimini izle — `pass/fail`
sayacı, `ok/bad/eq` yardımcıları, sonda `SONUC: PASS (n geçti, 0 kaldı)`):

```js
import {
  readSkipped, writeSkipped, readPending, writePending, clearPending,
  shouldPrompt, percent, installLooksFailed,
} from "../src/updater-logic.ts";

function memory() {
  const map = new Map();
  return { getItem: (k) => (map.has(k) ? map.get(k) : null),
           setItem: (k, v) => map.set(k, v),
           removeItem: (k) => map.delete(k) };
}

eq("bos hafiza: atlanan yok", readSkipped(memory()), null);
{
  const s = memory();
  writeSkipped(s, "0.8.1");
  eq("atlanan surum yazilir", readSkipped(s), "0.8.1");
}
eq("atlanan surum icin serit yok", shouldPrompt("0.8.1", "0.8.1"), false);
eq("yeni surum icin serit var", shouldPrompt("0.8.2", "0.8.1"), true);
eq("atlanan yoksa serit var", shouldPrompt("0.8.1", null), true);
{
  const s = memory();
  writePending(s, "0.8.1");
  eq("bekleyen kurulum okunur", readPending(s), "0.8.1");
  clearPending(s);
  eq("bekleyen temizlenir", readPending(s), null);
}
eq("kurulum basarisiz: surum hala eski + isaret", installLooksFailed("0.8.1", "0.8.0"), true);
eq("kurulum basarili: surum eslesti", installLooksFailed("0.8.1", "0.8.1"), false);
eq("isaret yoksa basarisiz sayilmaz", installLooksFailed(null, "0.8.0"), false);
eq("yuzde hesaplanir", percent(42, 100), 42);
eq("yuzde tam sayi", percent(1, 3), 33);
eq("toplam yoksa yuzde yok", percent(42, null), null);
eq("toplam sifirsa yuzde yok", percent(0, 0), null);
eq("%100 ustune cikmaz", percent(120, 100), 100);
```

- [ ] **Step 2: Testi koş, kırmızı olduğunu gör**

```powershell
node tools/test-updater.mjs
```

Beklenen: FAIL — `Cannot find module '../src/updater-logic.ts'`.

- [ ] **Step 3: Modülü yaz**

`src/updater-logic.ts` — saf, hiçbir import yok (node type-stripping ile test edilebilir
kalması için). Anahtarlar: `"hydra.update.skipped"`, `"hydra.update.pending"`.
`percent` yuvarlar ve `0..100` aralığına kırpar; `total` `null`/`0` ise `null`.

- [ ] **Step 4: Testi koş, yeşil olduğunu gör**

```powershell
node tools/test-updater.mjs
```

Beklenen: `SONUC: PASS (17 geçti, 0 kaldı)`.

- [ ] **Step 5: `package.json`'a kısa yol ekle**

`"test:i18n"` satırının altına:

```json
"test:updater": "node tools/test-updater.mjs",
```

- [ ] **Step 6: Commit**

```bash
git add src/updater-logic.ts tools/test-updater.mjs package.json
git commit -m "feat(updater): saf karar mantigi + 15 test"
```

---

### Task 6: Çalışma zamanı + güncelleme şeridi

**Files:**
- Create: `src/updater.ts`
- Modify: `index.html` (şerit işaretlemesi, `<body>` başı)
- Modify: `src/styles.css` (şerit stilleri)
- Modify: `src/main.ts` (başlatma + şerit bağlama)
- Modify: `webapp/i18n-en.js`
- Modify: `tools/ui-harness/run.mjs` (mock senaryosu)

**Interfaces:**
- Consumes: Task 5'in tüm fonksiyonları; `@tauri-apps/plugin-updater`'ın
  `check()` / `Update.downloadAndInstall(cb)` / `Update.close()`.
- Produces:

```ts
export type UpdaterHooks = {
  onAvailable(version: string, notes: string): void;
  onProgress(percent: number | null): void;
  onInstalling(): void;
  onError(message: string): void;
  onFailedBefore(pendingVersion: string): void;
};
export async function checkForUpdate(hooks: UpdaterHooks): Promise<void>;   // sessiz
export async function installUpdate(): Promise<void>;                       // şerit "Güncelle"
export function reportPendingFailure(hooks: UpdaterHooks): void;            // açılışta
```

- [ ] **Step 1: Şerit işaretlemesini ekle**

`index.html`, `<body>` içindeki ilk `<div class="app">` benzeri sarmalayıcının **en üstüne**:

```html
<div id="update-strip" class="update-strip" hidden>
  <span id="update-text">Hydra DL hazır</span>
  <div class="update-actions">
    <button type="button" class="primary mini" id="update-install">Güncelle</button>
    <button type="button" class="ghost mini" id="update-later">Sonra</button>
  </div>
</div>
```

- [ ] **Step 2: Stilleri ekle**

`src/styles.css` sonuna: `.update-strip` (tam genişlik, ince, `--accent` kenar),
`.update-strip.error` (kırmızı ton), `[hidden]` çalışsın diye `display: none` kuralı.
Mevcut renk değişkenlerini kullan (`var(--panel)`, `var(--line)`, `var(--danger)`).

- [ ] **Step 3: `src/updater.ts` yaz**

Eklentiyi sarar; `localStorage`'ı `Storage` olarak geçirir; `downloadAndInstall` callback'inin
`{ event: "Started" | "Progress" | "Finished", data }` biçimini `percent()`'e çevirir.
Kurulumdan **önce** `writePending(storage, version)` çağırır. `check()` `null` dönerse hiçbir
şey yapmaz (sessiz). Hata → `onError(kısa Türkçe mesaj)`.

- [ ] **Step 4: `main.ts`'e bağla**

`bootstrap()` içinde, `applyLanguage(...)` çağrısından **sonra**:

```ts
if (settings?.updateCheck !== false) {
  setTimeout(() => void checkForUpdate(updateHooks), 3000);
}
reportPendingFailure(updateHooks);
```

`updateHooks` nesnesi şeridi gösterir/gizler; "Sonra" `writeSkipped` + gizle; "Güncelle"
`installUpdate()`.

- [ ] **Step 5: İngilizce metinleri ekle**

`webapp/i18n-en.js`:

```js
  "Hydra DL %1 hazır": "Hydra DL %1 is ready",
  "Güncelle": "Update",
  "Sonra": "Later",
  "İndiriliyor… %1": "Downloading… %1",
  "Kuruluyor, uygulama yeniden açılacak": "Installing, the app will restart",
  "Güncelleme indirilemedi — internet bağlantını kontrol et":
    "Could not download the update — check your internet connection",
  "Yeniden dene": "Retry",
  "Güncelleme tamamlanmadı — yeniden dene": "Update did not complete — try again",
  "Güncelleme aranıyor…": "Checking for updates…",
  "Güncelleme yok": "No updates",
```

```powershell
node tools/audit-i18n.mjs
```

Beklenen: `eksik: 0`.

- [ ] **Step 6: Mock senaryosu yaz (ui-harness)**

`tools/ui-harness/run.mjs`: `/src/api.ts` stub'ının yanına `@tauri-apps/plugin-updater`
stub'ı ekle (`window.__hydraUpdater` ile kontrol edilir):

```js
await page.route("**/@tauri-apps/plugin-updater*", (route) =>
  route.fulfill({ status: 200, contentType: "text/javascript",
    body: `
export async function check() {
  const u = globalThis.__hydraUpdater;
  if (!u?.available) return null;
  return { version: u.version, body: u.notes ?? "",
           downloadAndInstall: async (cb) => { u.installed = true;
             cb?.({ event: "Progress", data: { chunkLength: 50, contentLength: 100 } }); },
           close: async () => {} };
}` }));
```

Senaryo:

```js
// 1) Sürüm yok → şerit gizli
await page.evaluate(() => { globalThis.__hydraUpdater = { available: false }; });
// (yeni sayfa yükle, 3.2 sn bekle)
check(await page.locator("#update-strip").isHidden(), "surum yokken serit gizli");

// 2) Sürüm var → şerit görünür, metin sürümü içerir
await page.evaluate(() => { globalThis.__hydraUpdater = { available: true, version: "0.8.1" }; });
await page.reload();
await page.waitForSelector("#update-strip:not([hidden])", { timeout: 10_000 });
check((await page.locator("#update-text").textContent()).includes("0.8.1"), "serit surumu gosteriyor");

// 3) "Sonra" → gizlenir ve hatırlanır
await page.click("#update-later");
check(await page.locator("#update-strip").isHidden(), "Sonra seridi kapatti");
await page.reload();
await page.waitForTimeout(3500);
check(await page.locator("#update-strip").isHidden(), "ayni surum icin tekrar sorulmadi");

// 4) updateCheck kapalı → kontrol hiç çağrılmaz
// mock-api SETTINGS zaten updateCheck:true; testte ayrı sayfada false yap
```

- [ ] **Step 7: Testleri koş**

```powershell
npm.cmd run build
node tools/ui-harness/run.mjs
node tools/test-updater.mjs
node tools/audit-i18n.mjs
```

Beklenen: hepsi PASS.

- [ ] **Step 8: Commit**

```bash
git add src/updater.ts src/main.ts index.html src/styles.css webapp/i18n-en.js tools/ui-harness/run.mjs
git commit -m "feat(updater): guncelleme seridi + eklenti sarmalayicisi"
```

---

### Task 7: Temiz kapanış (kurulumdan önce)

**Files:**
- Modify: `src-tauri/src/lib.rs:97-105` (kapanış kancası)
- Modify: `src-tauri/src/manager.rs` (yeni komut: tüm indirmeleri duraklat)
- Modify: `src-tauri/src/commands.rs` (komut yüzeyi)
- Modify: `src/api.ts` (yeni fonksiyon adı)
- Test: `src-tauri/src/manager.rs` (birim), `tools/ui-harness/run.mjs` (mock)

**Interfaces:**
- Consumes: mevcut `state.persist()`, `state.torrent.shutdown()`.
- Produces: `pause_all_for_shutdown()` — Rust `commands::pause_all_for_shutdown`,
  JS `api.pauseAllForShutdown(): Promise<void>`. `src/updater.ts` `installUpdate()`
  bunu indirmeden **önce** çağırır.

- [ ] **Step 1: Başarısız testi yaz (Rust)**

`src-tauri/src/manager.rs` testlerine:

```rust
#[tokio::test]
async fn kapanis_icin_duraklatma_calisanlari_durdurur() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::load(dir.path());
    let mut running = sample("a");
    running.status = Status::Downloading;
    running.downloaded_bytes = 4096;
    let mut queued = sample("b");
    queued.status = Status::Queued;
    store.items = vec![running, queued];
    store.save().unwrap();

    // ... AppState kur, pause_all_for_shutdown(&state).await ...

    let items = state.downloads();
    assert_eq!(items[0].status, Status::Paused, "calisan duraklatildi");
    assert_eq!(items[0].downloaded_bytes, 4096, "parca baytlari korundu");
    assert_eq!(items[1].status, Status::Queued, "kuyruktakine dokunulmadi");
    assert!(
        items.iter().all(|d| d.status != Status::Cancelled),
        "iptal edilmedi (parcalar silinmedi)"
    );
}
```

Testin kurulum kısmı `manager.rs`'deki mevcut `AppState` test yardımcılarıyla tamamlanır;
iddialar yukarıdaki gibidir.

- [ ] **Step 2: Testi koş, kırmızı olduğunu gör**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml kapanis_icin_duraklatma
```

Beklenen: FAIL — fonksiyon yok.

- [ ] **Step 3: Fonksiyonu yaz**

`manager.rs`:

```rust
/// Güncelleme kurulumundan önce: çalışan indirmeleri duraklat, durumu kaydet.
/// İptal DEĞİL — parçalar korunur, açılışta kaldığı yerden devam eder.
pub async fn pause_all_for_shutdown(state: &Arc<AppState>);
```

- [ ] **Step 4: Komutu bağla**

`commands.rs`'e (mevcut komut desenini izle):

```rust
#[tauri::command]
pub async fn pause_all_for_shutdown(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String>;
```

`lib.rs` `generate_handler!` listesine ekle. `src/api.ts`'e:

```ts
export async function pauseAllForShutdown(): Promise<void> {
  return invoke("pause_all_for_shutdown");
}
```

- [ ] **Step 5: Kurulum yolunu bağla**

`src/updater.ts` `installUpdate()`: `await api.pauseAllForShutdown();` → sonra
`writePending()` → `downloadAndInstall()`.

- [ ] **Step 6: Kapanış kancasını paylaş**

`lib.rs`'deki `WindowEvent::Destroyed` bloğu ile güncelleme yolu **aynı** fonksiyonu
kullanacak şekilde çıkar (davranış aynı kalır). Güncelleme yolu bu kancayı atlamaz.

- [ ] **Step 7: Testleri koş**

```powershell
cargo test --manifest-path src-tauri\Cargo.toml
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets -- -D warnings
node tools/ui-harness/run.mjs
```

Beklenen: hepsi yeşil. ui-harness mock'una `pauseAllForShutdown: async () => {}` ekle ki
şerit senaryosu patlamasın.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/manager.rs src-tauri/src/commands.rs \
        src/api.ts src/updater.ts tools/ui-harness/mock-api.mjs
git commit -m "feat(updater): kurulum oncesi temiz kapanis (duraklat + kaydet)"
```

---

### Task 8: `tools/release.mjs` + iki release + canlı E2E

**Files:**
- Create: `tools/release.mjs`
- Modify: `AGENTS.md` (komut listesi)
- Modify: `HANDOFF.md`, `PLAN.md` (dilim kapandı)

**Interfaces:**
- Consumes: `TAURI_SIGNING_PRIVATE_KEY` + `..._PASSWORD`; `gh` CLI (kurulu,
  `TitanLion72` olarak yetkili).
- Produces: `node tools/release.mjs <sürüm>` — sürümü artırır, doğrular, imzalar,
  `latest.json` yazar, commit+tag+push, release oluşturur.

- [ ] **Step 1: Aracı yaz**

Adımlar (sırayla, biri kırmızıysa **dur**):

1. Argüman `<sürüm>` doğrula (`x.y.z`, mevcut sürümden büyük).
2. Sürümü 3 dosyada değiştir: `package.json`, `src-tauri/tauri.conf.json`,
   `src-tauri/Cargo.toml`.
3. `cargo test`, `npm.cmd run build`, `node tools/security-check.mjs`.
4. Parolayı sor (`readline`, gizli giriş); `TAURI_SIGNING_PRIVATE_KEY` = anahtar dosyası
   **içeriği**, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = parola.
5. `npm.cmd run tauri build` → `.exe` + `.exe.sig`.
6. `latest.json` yaz: `version`, `notes` (argüman veya git log), `pub_date` (ISO),
   `platforms["windows-x86_64"] = { signature: <.sig içeriği>, url: <release URL, boşluk → %20> }`.
7. `git commit -am "release: v<sürüm>"`, `git tag v<sürüm>`, `git push --follow-tags`.
8. `gh release create v<sürüm> --title ... --notes ... <.exe> <.sig> <latest.json>`.

- [ ] **Step 2: Kuru koşu (release oluşturmadan)**

Araca `--dry-run` bayrağı ekle: 1-6 adımlarını yapar, 7-8'i atlar ve `latest.json`
içeriğini basar. Çalıştır, çıktıyı gözle doğrula (`url` içinde `%20` var mı, imza dolu mu).

- [ ] **Step 3: v0.8.0'ı yayınla (bootstrap)**

```powershell
node tools/release.mjs 0.8.0
```

Sonra **elle** kur (updater'lı ilk sürüm; eski sürüm endpoint'i bilmiyor):

```powershell
Start-Process "src-tauri\target\release\bundle\nsis\Hydra DL_0.8.0_x64-setup.exe" `
  -ArgumentList '/S',"/D=$env:LOCALAPPDATA\Hydra DL" -Wait
```

- [ ] **Step 4: v0.8.1'i yayınla (gerçek E2E)**

```powershell
node tools/release.mjs 0.8.1
```

- [ ] **Step 5: Canlı E2E kanıtı**

Uygulamayı aç → ~3 sn içinde şerit "v0.8.1 hazır" → **Güncelle** → ilerleme → "Kuruluyor"
→ yeniden açılır → `GET /health` `"version":"0.8.1"`. Ayrıca: güncelleme öncesi bir
indirme başlat, kurulumdan sonra `paused` → devam ediyor olmalı, parçalar korunmuş olmalı.

- [ ] **Step 6: Belgeleri güncelle**

`AGENTS.md` komut listesine `node tools/release.mjs <sürüm>`; `PLAN.md`'de
"Otomatik güncelleme" işaretlenir; `HANDOFF.md`'ye 5 satır; `SECURITY.md`'ye tedarik
zinciri bölümü: imza doğrulaması, anahtarın yeri/yedeği, rollback (release'i draft'a
çekme / daha yüksek sürüm yayınlama).

- [ ] **Step 7: Commit + push**

```bash
git add tools/release.mjs AGENTS.md PLAN.md HANDOFF.md
git commit -m "feat(release): tek komut yayin araci + v0.8.0/v0.8.1"
git push
```

---

## Spec kapsam kontrolü

| Spec maddesi | Görev |
|---|---|
| Açılışta sessiz kontrol | 6 |
| Şerit + Güncelle/Sonra | 6 |
| İmza doğrulama | 2 (eklenti), 8 (E2E kanıt) |
| "Sonra" hatırlanır | 5, 6 |
| `updateCheck` ayarı (masaüstü + telefon) | 3, 4 |
| Anahtar üretimi + yedek | 1 |
| Pubkey + endpoint + izin | 2 |
| Temiz kapanış | 7 |
| Sessiz başarısız kurulum şeridi | 5 (`installLooksFailed`), 6 |
| Tek komut yayın | 8 |
| İki release + kalıcı tag | 8 |
| Telefon: şerit yok, API değişmez | 4 (yalnız ayar anahtarı) |
| SECURITY.md tedarik zinciri | 8 (Step 6'ya eklenir) |

## Yapıcıya bırakılanlar

- Şeridin tam görsel yerleşimi (mevcut `.pill`/`.toast` diliyle uyumlu).
- Kontrol gecikmesi 3 sn (spec önerisi), indirme zaman aşımı yok ama iptal edilebilir.
- `latest.json`'ın `notes` alanının metni (sürüm notları git log'dan özetlenebilir).
