# Eklenti çift tarayıcı doğrulaması — 26.10.2026

Kapsam: yalnız okuma/test. `extension/**`, `src/**`, `src-tauri/**`, `webapp/**`, `docs/**`,
kök `*.md` ve `package.json` değiştirilmedi.

## Ortam ve yöntem

- `build/extensions/chrome` ve `build/extensions/firefox` **kaynaktan birebir üretilmiş**
  (26 dosya karşılaştırıldı, fark yok) → testler güncel kodu ölçtü.
- Chromium tarayıcı: **Microsoft Edge 156.0.4314.42** (Chromium çekirdeği, MV3),
  `launchPersistentContext` + `--disable-extensions-except` + `--load-extension`, headless.
  Eklenti kimliği: `hfgbeajafkakjihadfadoncjjpfnkcoh`.
- **Google Chrome 156 markalı derlemede `--load-extension` çalışmıyor** (M137'den beri
  Google bu anahtarı markalı derlemelerde kapatıyor; service worker hiç kaydolmadı).
  Tam Chromium/Chrome for Testing kurulu değil. Bu yüzden gerçek tarayıcı kanıtı Edge
  üzerinden alındı — aynı Chromium 156 motoru ve aynı `chrome.*` API yüzeyi.
- Sahte yakalama sunucusu: `http://127.0.0.1:6799` istekleri Playwright route ile
  karşılandı (gerçek `fetch`, gerçek extension origin). Hydra masaüstü uygulaması koşmadı.
- Firefox kurulu değil → Firefox bölümü **statik + VM simülasyonu**.

## Chrome/Edge — çalışma anı

| # | Madde | Sonuç | Kanıt |
|---|---|---|---|
| 1 | Service worker hatasız kaydoldu | OK | `chrome-extension://…/background.js`; `hasHX:"object"`, `manifestVersion:"0.2.0"`, SW konsolunda hata yok |
| 2 | `HX.api` + `chrome.runtime.getURL` + panel açılıyor | OK | panel: `getURL:"chrome-extension://…/panel.html"`, `version:"Hydra DL eklentisi v0.2.0"` |
| 3 | Sağ tık menüleri | OK | SW içinde `createMenus()` iki kez çağrıldı: `"ok"`, `consoleDuringMenus: []` |
| 4 | `runtime.sendMessage` yönlendirmesi | OK | panel → `{ok:false,error:"…Failed to fetch",code:"network"}`; sahte sunucu açıkken `{ok:true,data:{…}}` — "Receiving end does not exist" yok |
| 5 | `popup.html` | OK | `version:"v0.2.0"`, `statusText:"uygulama kapalı"`, 0 konsol hatası |
| 6 | `options.html` kaydet → storage | OK | `stored:{port:6799,showPanel:false,hunterEnabled:false,askOnDownload:true}`, `result:"Bağlı — hydra-dl v9.9.9 · 2 indirme, 1 aktif"` |
| 7 | `#permission-row` Chrome/Edge'de gizli | OK | `rowHidden:true`, `permissions.contains(origins):true` |
| 8 | `hunter.js` enjeksiyonu | OK | `__hydraHunter` isolated world'de olduğu için DOM'dan kanıtlandı: `hydra-set` yanıtı `{ok:true,active:true}`, `#hydra-hunter-host` DOM'da, gölge `.bar` display `flex` |
| 9 | Avcı → panel | OK | link tıklaması panel açtı: `url=…/file.zip`, `referer=…/watch.html`, `title=Hydra+Test` |
| 10 | `blob:` medya → sayfa adresi | OK | video `src=blob:http://127.0.0.1:6799/…` iken panel `url=http://127.0.0.1:6799/watch.html` (`isBlobInPanelUrl:false`) |
| 11 | Panel format akışı | OK | yt-dlp probe: 2 satır `["137","140"]`, seçim boyutu 150 MB → 4,00 MB; `POST /add` gövdesi `format:"140"` |
| 12 | Düz dosyada sahte format yok | OK | `formatRows:1`, radio `value:""`, gövde: `{"url":…,"filename":…,"dir":…}` — `format` alanı yok |
| 13 | `POST /add` 400 → GET'e düşmemeli | OK | yalnız `POST /add`; mesaj `"Eklenemedi: sunucu reddetti"`, hata ekranı yok (çift ekleme yok) |
| 14 | `POST /add` 404 → GET yedeği | OK | `POST /add` + tek `GET /add?url=…` |
| 15 | `/probe` 404 → geri dönüş | OK | hata ekranı `display:none`, `#fallback-note` `display:block`, 1 format satırı |
| 16 | `[hidden]` CSS ezilmiyor | OK | `#candidates-box`→`none`, `#error-screen`→`none`, `#dir-row`→`flex`, `#resume-chip`→`block` |
| 17 | Konsol hatası listesi | OK | 0 `pageerror`; yalnız uygulama kapalıyken `net::ERR_CONNECTION_REFUSED` kaynak hataları (beklenen) |

## Firefox — statik + VM

| # | Madde | Sonuç | Kanıt |
|---|---|---|---|
| 1 | `manifest.json` geçerli JSON | OK | `JSON.parse` başarılı |
| 2 | `background.scripts` var, `service_worker` yok | OK | `["compat.js","shared.js","background.js"]`, `hasServiceWorker:false` |
| 3 | `gecko.id` / `strict_min_version` | OK | `hydra-dl@titanlion72`, `115.0` (≥115) |
| 4 | Background dosyaları mevcut, ilk eleman `compat.js` | OK | `missingBackgroundFiles: []` |
| 5 | `content_scripts[0].js[0] === "compat.js"` | OK | manifest alanı |
| 6 | Tüm HTML'lerde ilk `<script src>` = `compat.js` | OK | panel/popup/options × (chrome, firefox) = 6/6 |
| 7 | `compat.js` mantığı | OK | izole realm: yalnız `chrome` → `api===chrome, isFirefox:false`; yalnız `browser` → `api===browser, isFirefox:true`; ikisi → `browser` seçilir |
| 8 | Doğrudan `chrome.*` çağrısı yok | OK | `extension/**` ve `build/extensions/firefox/**` taraması: eşleşme yalnız `compat.js` **yorum** satırları |
| 9 | `options.js` izin akışı | OK | `refreshPermission()` ilk satırı `if (!HX.isFirefox) return;` (`extension/options.js:99`) |

## Bulgular (kod düzeltmesi yapılmadı)

1. **`HX.isFirefox` Edge'de yanlış `true`** — Edge, eklenti bağlamında promise tabanlı bir
   `browser` nesnesi de tanımlıyor (`browser !== chrome`, `await browser.storage.sync.get(…)`
   çalışıyor). `compat.js:16` yalnız `typeof globalThis.browser` baktığı için Edge
   "Firefox" sayılıyor (`isFirefox:true`, `api===chrome:false`).
   **Etki:** Edge'de işlevsel kırılma yok (`api` promise döndüğü için her şey çalışıyor;
   `permissions.contains()` `true` olduğu için izin satırı gizli kalıyor). Yine de ayırt
   etme ölçütü kırılgan: `browser` sunup promise sunmayan bir Chromium sürümü çıkarsa
   `HX.api` callback'e düşer. Daha güvenli ölçüt: `!!browser?.runtime?.getBrowserInfo`.
2. `compat.js`'in "API yok" erken çıkış yolu Chromium'da test edilemiyor: normal sayfada da
   `window.chrome` (uzantısız stub) tanımlı olduğu için `HX` yine kuruluyor — zararsız,
   yalnız test gözlemi.

## Sınırlar

- Bu oturumda **kabuk (shell) aracı yok**: `npm`/`node`/`git` çalıştırılamadı. Bu yüzden
  - `node --check` yerine aynı işi yapan sözdizimi kontrolü yapıldı: derlenen 9 `.js`
    (7 eklenti + `webapp/app.js`, `webapp/sw.js`) `new Function(src)` ile parse edildi → hepsi temiz.
  - `tools/*.mjs` (ESM) parse edilemedi → doğrulanmadı.
  - `git status --porcelain` koşulamadı. Depoya yazılan tek yol `tools/ext-check/`;
    `build/extensions/` ve `node_modules/` `.gitignore`'da.
- Gerçek Firefox ve gerçek Google Chrome denenmedi (kurulu değil / `--load-extension` kapalı).

## Sonradan yapılan düzeltme (bu raporun ardından)

Raporun 1. riski (**compat.js Edge'de rowser gördüğü için kendini Firefox sanıyor**)
düzeltildi: tespit artık `!!browser?.runtime?.getBrowserInfo` üzerinden yapılıyor
(yalnız Firefox'ta bulunur). Kalıcı test: `node tools/test-extension.mjs` → 19 kontrol,
Edge benzeri bağlamda `isFirefox === false` doğrulanıyor.
Bu rapordaki ölçümler düzeltme **öncesi** koda aittir; düzeltme sonrası Chrome/Edge'de
yeniden koşum yapılmadı (Edge + Playwright ortamı tekrar kurulmalı).
