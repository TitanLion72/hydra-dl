# Hydra DL

[![License: AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSE)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.77%2B-000000?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey)](#kurulum)
[![CI](https://github.com/TitanLion72/hydra-dl/actions/workflows/ci.yml/badge.svg)](https://github.com/TitanLion72/hydra-dl/actions/workflows/ci.yml)

**Hydra DL**, IDM'nin açık kaynak alternatifi olarak tasarlanmış çok bağlantılı bir
indirme yöneticisidir. Tauri v2 + Rust çekirdeği dosyayı parçalara bölüp paralel indirir,
kaldığı yerden devam eder, kuyruğu yönetir; tarayıcı eklentisi ve telefondan yönetilen
yerel web arayüzüyle birlikte gelir. Arayüz Türkçe ve koyu temalıdır.

> 📷 **Ekran görüntüleri yakında.** `docs/screenshots/` klasörü hazırlanıyor; kırık
> görsel yerine bu notu bırakıyoruz.

## Özellikler

### İndirme çekirdeği

- **Segmentli / paralel HTTP indirme** — dosya başına 1–16 Range bağlantısı; 1 MiB'in
  altındaki parçalara bölme yapılmaz. Sunucu `Range` desteklemiyorsa otomatik olarak tek
  bağlantıya düşer.
- **Duraklat / devam / yeniden dene** — yarım parçalar diskte (`parts/`) tutulur; uygulama
  kapansa bile indirme kaldığı yerden sürer.
- **Kuyruk ve eşzamanlılık** — aynı anda 1–10 indirme; kalanlar sırada bekler.
- **Global hız limiti** — tüm indirmelerin paylaştığı token bucket (KB/s); `0` sınırsız.
- **Atomik bitiş** — hedef dosya doğrudan yazılmaz; `<dosya>.hydra-tmp` üzerinden taşınır.
  Yarıda kalan bir indirme, tamamlanmış dosya gibi görünmez.

### Medya ve kaynak bulma

- **yt-dlp entegrasyonu** — YouTube, Instagram, TikTok, X, Vimeo, Twitch, Dailymotion,  SoundCloud ve yt-dlp'nin desteklediği diğer siteler; kalite/format seçimi, ilerleme ve
  iptal desteklenir.
- **Akıllı kaynak bulucu** — reklam ve ara sayfaları atlayıp gerçek medya kaynağını
  çıkarır: `og:video`, JSON-LD, `<video>` / `<source>` etiketleri ve HLS/DASH manifestleri.
- **HLS indirme (yt-dlp olmadan)** — `.m3u8` akışları doğrudan indirilir: segmentler
  paralel çekilip birleştirilir, master playlist'te en yüksek kalite seçilir,
  `AES-128` şifrelemesi çözülür ve duraklat/devam çalışır. **DRM reddedilir.**

### P2P

- **BitTorrent** — `magnet:` bağlantıları ve `.torrent` dosyaları; torrent'ler normal
  indirme listesinde görünür, peer/seeder bilgisiyle birlikte izlenir.

### Tarayıcı ve telefon

- **Chrome / Edge (MV3) eklentisi** — bağlantıya, videoya veya sayfaya sağ tık →
  “Hydra DL ile indir”; araç çubuğunda IDM tarzı indirme paneli.
- **Avcı modu** — sayfadaki videoyu/dosyayı tıklayarak seçip indirmeye gönderme.
  İstenirse panel tamamen kapatılabilir.
- **Telefon / tablet arayüzü** — yerel ağda çalışan web arayüzü (`webapp/`), QR kodla
  eşleşme; aynı listeyi, kuyruğu ve ayarları telefondan yönetme.

### Arayüz

- Türkçe, koyu tema, durum filtreleri, canlı hız / ilerleme / ETA, klasörde göster ve
  dosyayı aç.
- **Geçmişte arama ve sıralama** — dosya adı ya da adrese göre anında süzme
  (`Ctrl+F` veya `/`), tarih / ad / boyut / duruma göre sıralama (tercih hatırlanır).

## IDM'den farkı

| | Hydra DL | Internet Download Manager |
|---|---|---|
| Kaynak | Açık kaynak (AGPL-3.0-or-later) | Kapalı kaynak, ticari |
| Ücret / reklam | Ücretsiz, reklam yok | Ücretli lisans, deneme süresi |
| Teknoloji | Tauri v2 + Rust + TypeScript | Windows'a özel, kapalı bileşenler |
| Medya siteleri | Gömülü yt-dlp + format seçimi | Ayrı video yakalama modülü |
| BitTorrent | Var (magnet + `.torrent`) | Yok |
| Genişletilebilirlik | Yerel HTTP API, açık eklenti ve web arayüzü | Sınırlı, kapalı |
| Platform | Windows birinci sınıf; Linux/macOS deneysel | Windows |

Hydra DL, IDM'nin yerini birebir doldurmayı değil; aynı temel akışı (parçalı indirme,
devam, tarayıcıdan yakalama) açık ve denetlenebilir bir kod tabanıyla sunmayı hedefler.

## Kurulum

### Gereksinimler

- **Node.js 20+** (CI'da 22 kullanılır)
- **Rust 1.77+** (`rustup` ile `stable` yeterli)
- **WebView2** — Windows 11'de hazır gelir; Windows 10'da
  [Evergreen Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) kurun.
- Windows birinci sınıf hedeftir; Linux ve macOS deneysel kabul edilir.

### Geliştirme

```powershell
git clone https://github.com/TitanLion72/hydra-dl.git
cd hydra-dl
npm.cmd install
npm.cmd run tauri dev
```

> PowerShell'de `npm` yerine `npm.cmd` kullanın (`npm.ps1` execution policy'ye takılabilir).

### Doğrulama

```powershell
npm.cmd install
npm.cmd run tauri dev
npm.cmd run build
cargo test --manifest-path src-tauri\Cargo.toml
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets
node tools/e2e.mjs --capture-port 6712 --dir "$env:TEMP\hydra-e2e"
```

Son komut, uygulama açıkken yerel bir Range sunucusuna indirme yapıp SHA-256'yı doğrular.
`--dir` verilen klasör geçici olmalıdır; test gerçek dosya yazar.

### Üretim derlemesi

```powershell
npm.cmd run tauri build
# çıktı: src-tauri\target\release\bundle\nsis\
```

Depoda imzalı ikili dosya dağıtılmaz; kurulum paketini kendiniz üretirsiniz.

## Tarayıcı eklentisi

Tek kaynak `extension/` klasörüdür. İki tarayıcı için ayrı paket üretilir (manifestleri
farklı: Chrome service worker kullanır, Firefox arka plan betiklerini):

```powershell
node tools/build-extension.mjs            # ikisi
# çıktı: build/extensions/chrome, build/extensions/firefox
```

**Chrome / Edge / Brave**

1. `chrome://extensions` (Edge'de `edge://extensions`) → **Geliştirici modu**.
2. **Paketlenmemiş öğe yükle** → `build/extensions/chrome` klasörünü seçin.
3. Eklenti ayarlarından portu Hydra DL ile aynı yapın (varsayılan `6712`).

**Firefox**

1. `about:debugging#/runtime/this-firefox` → **Geçici Eklenti Yükle**.
2. `build/extensions/firefox/manifest.json` dosyasını seçin.
3. Araç çubuğundaki Hydra DL simgesine tıklayın → **Ayarlar** → *Yerel erişim izni ver*
   (Firefox, MV3'te yerel adres iznini kurulumda otomatik vermez; bu düğme tek tıkla ister).
4. Portu `6712` yapın ve **Bağlantıyı test et**'e basın.

> Geçici eklenti Firefox kapanınca silinir. Kalıcı kurulum için eklenti Mozilla
> (addons.mozilla.org) tarafından imzalanmalı ya da `xpinstall.signatures.required`
> kapatılmış bir Firefox sürümü kullanılmalıdır. Bu adım henüz otomatikleştirilmedi.

Kullanım:

- Bağlantı / video / görsel üzerine sağ tık → **Hydra DL ile indir**.
- Sayfa üzerine sağ tık → **Bu sayfayı Hydra DL ile indir**.
- Araç çubuğu düğmesi: o anki sekmeyi veya elle yapıştırılan bağlantıyı gönderir.
- **Avcı modu**: sayfadaki öğeyi tıklayarak seçin; adres, sayfa `Referer` bilgisiyle
  birlikte uygulamaya iletilir.

Eklenti yalnız `http://127.0.0.1/*` adresine erişir; uzak sunucuya veri göndermez.

## Telefon (yerel ağ web arayüzü)

Masaüstü uygulaması, aynı Wi-Fi ağındaki telefon ve tabletler için `webapp/` arayüzünü
yerel ağda sunabilir. Sunucu varsayılan olarak **kapalıdır** ve `6790` portunu dinler:

1. Masaüstünde yerel ağ sunucusunu açın (`Settings.webEnabled`; adres ve token ayarları
   aynı yerde).
2. Telefondan gösterilen adresi (`http://<bilgisayar-ip>:6790`) açın veya QR kodu okutun.
3. Güvenlik için bir **token** belirleyin; adres `?token=...` ya da `X-Hydra-Token`
   başlığıyla açılmalıdır. Token boşsa aynı ağdaki herkes erişebilir.

Sunucu yalnız yerel ağ arayüzüne (`0.0.0.0:6790`) bağlanır, internete açılmaz. Ağın
güvenilir olduğundan emin değilseniz sunucuyu kapalı tutun. Telefon arayüzünde de
**arama** var: ad ya da adrese göre süzme, sayaçlar aramaya göre daralır.

## yt-dlp

Video indirmeleri için `yt-dlp` gerekir. Uygulama şu sırayla arar:

1. Ayarlardaki **yt-dlp yolu**
2. `PATH`
3. Uygulama kaynak/çalıştırılabilir klasörünün yanındaki `bin\yt-dlp.exe`
4. `%APPDATA%\com.hydradl.app\bin\yt-dlp.exe`

**En kolay yol:** Ayarlar → *Medya (yt-dlp)* → **“yt-dlp'yi indir ve kur”**. Uygulama resmî
son sürümü indirir, `%APPDATA%\com.hydradl.app\bin\yt-dlp.exe` altına koyar ve `--version`
ile çalıştığını doğrular. İlerleme canlı gösterilir; hata olursa dosya geri alınır.

Elle kurmak istersen:

```powershell
winget install yt-dlp.yt-dlp
# alternatif: https://github.com/yt-dlp/yt-dlp/releases -> yt-dlp.exe
```

`yt-dlp` bulunamazsa yalnız medya indirmeleri devre dışı kalır; düz HTTP indirmeleri
çalışmaya devam eder.

> Seçtiğin kalite video-only bir akışsa Hydra sesi otomatik ekler (`137+bestaudio/137`),
> yani sessiz video inmez.

## Mimari

| Katman | Yer | Sorumluluk |
|---|---|---|
| Veri modeli | `src-tauri/src/model.rs` | `Download`, `Segment`, `Status`; saf ve testli segment planlama |
| Kalıcılık | `src-tauri/src/store.rs` | `downloads.json`, ayarlar, kuyruk seçimi |
| İndirme motoru | `src-tauri/src/engine/` | Range indirme, parça birleştirme, iptal sinyali, hız limiti |
| Orkestrasyon | `src-tauri/src/manager.rs` | Kuyruk, eşzamanlılık, ticker, olay yayını |
| Medya / P2P | `src-tauri/src/extract/`, `torrent.rs` | Kaynak çıkarma, yt-dlp, BitTorrent |
| Yerel sunucular | `src-tauri/src/capture.rs`, `web/` | Eklenti yakalama API'si, LAN web arayüzü |
| Arayüz | `src/` | Vanilla TypeScript; durum yalnız `downloads:changed` olayından |
| İstemciler | `extension/`, `webapp/` | MV3 eklentisi, telefon arayüzü |

Ayrıntılı veri akışı, kalıcılık şeması ve değişmezler: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).
Yerel HTTP sözleşmesi: [`docs/API.md`](docs/API.md).

## Bilinen sınırlar

- Kimlik doğrulamalı / çerez gerektiren indirmeler eklentiyle aktarılabilir (çerez
  `Cookie` başlığı olarak gider); HLS'te de kullanılır.
- HLS'te canlı akış (`#EXT-X-ENDLIST` yok) ve `EXT-X-BYTERANGE` henüz desteklenmez;
  DASH (`.mpd`) için yt-dlp gerekir.
- Yakalama sunucusu yalnız `127.0.0.1` dinler ve CORS'a açıktır; yerel makinedeki herhangi
  bir sayfa indirme ekleyebilir. Güvenilmeyen makinelerde ayarlardan kapatın.
- Hız limiti kaba bir global bütçedir; bağlantı başına adil paylaşım yapmaz.
- TLS doğrulaması `rustls` varsayılanıdır; kurumsal proxy/CA senaryoları test edilmedi.
- Linux/macOS paketleri deneyseldir; birinci sınıf hedef Windows'tur.

## Yol haritası

Sıradaki işler ve tamamlananlar [`PLAN.md`](PLAN.md) dosyasında. Kısa vadede: çerez
aktarımı, `Retry-After` desteği, proxy ayarları, indirme geçmişinde arama, bildirimler ve
imzalı kurulum.

## Güvenlik

Yerel makinede çalışır ama **LAN'a açılan** bir HTTP sunucusu çalıştırdığı için güvenlik
kuralları pazarlık konusu değildir. Tehdit modeli ve bağlayıcı kurallar:
[`SECURITY.md`](SECURITY.md).

- Web API'si **varsayılan olarak her yolu** token ile korur; yalnız `/health` ve gerçek statik
  dosyalar serbesttir. Yeni eklenen uç otomatik olarak korunur.
- Kullanıcı girdisinden dosya yolu kurulmaz: `..`, `\`, sürücü öneki ve UNC reddedilir;
  çözümlenen yol kök klasörün altında olmalı (`Path::join` tek başına yetmez).
- Tarayıcı isteklerinde `Origin` denetlenir (rastgele bir web sayfası API'yi çağıramaz).
- CI: test, clippy, fmt, `cargo audit`, `npm audit`, CodeQL ve
  `node tools/security-check.mjs` (sır/kişisel yol/XSS taraması) yeşil olmadan merge yok.

Hassas açıkları herkese açık issue olarak **açmayın**; GitHub özel güvenlik bildirimini kullanın.

## Katkı

Katkılar memnuniyetle karşılanır. Başlamadan önce:

- [`CONTRIBUTING.md`](CONTRIBUTING.md) — geliştirme akışı, kod stili, test beklentileri
- [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) — topluluk kuralları
- [`SECURITY.md`](SECURITY.md) — güvenlik açığı bildirimi (herkese açık issue açmayın)

Hata bildirimi ve özellik önerileri için issue şablonlarını kullanın. Sorular için
[`docs/FAQ.md`](docs/FAQ.md) ve Discussions.

## Lisans

Telif hakkı © Hydra DL katkıcıları. Bu proje
[**GNU Affero General Public License v3.0 veya sonrası**](LICENSE) (AGPL-3.0-or-later)
ile lisanslanmıştır.

AGPL bir **açık kaynak** lisanstır: kodu ticari olarak da kullanabilir, değiştirebilir ve
dağıtabilirsiniz. Koşul, dağıtılan (veya ağ üzerinden sunulan) sürümlerin kaynak kodunun
da AGPL-3.0-or-later altında paylaşılmasıdır. Kapalı kaynak bir ürüne gömüp dağıtmak
lisansla bağdaşmaz.

## Yasal not

Hydra DL yalnızca kullanıcının indirme yetkisine sahip olduğu içerikler için bir araçtır.

- **DRM korumalı içerik indirilemez.** DRM atlatma özelliği yoktur ve eklenmeyecektir.
- İndirdiğiniz içeriğin telif ve kullanım koşullarına uymak **sizin** sorumluluğunuzdadır;
  bulunduğunuz ülkenin telif kuralları geçerlidir.
- Bu proje hiçbir platformun (YouTube, Instagram, TikTok, X vb.) resmî aracı değildir ve
  onlar tarafından desteklenmez. Platformların kullanım şartlarına uyun.
- Yazılım "olduğu gibi" sunulur; AGPL-3.0 kapsamındaki garanti reddi geçerlidir.
