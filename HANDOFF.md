# HANDOFF

> Yeni oturum: **önce bu dosyayı oku**, sonra `git status`. Bitirdiğin dilimi en alta 10 satır yaz.

**Son güncelleme:** 2026-11-06 · v0.4.1 · AGPL-3.0-or-later

---

## Nerede kaldık

v0.2 kapsamı çalışıyor ve **gerçek içerikle doğrulandı**: segmentli HTTP, akıllı kaynak bulucu,
yt-dlp medya indirme (tek tıkla kurulum dahil), BitTorrent motoru, masaüstü kalite paneli,
telefon için web arayüzü, tarayıcı eklentisi (panel + avcı modu).

**Repo public:** <https://github.com/TitanLion72/hydra-dl> (AGPL-3.0, ilk commit `9495fd4`).

**Kurulu sürüm:** v0.4.1, `%LOCALAPPDATA%\Hydra DL` (eski geçici kurulumlar ve kayıt
girdisi temizlendi, kullanıcının indirmeleri korundu). Kurulum paketi:
`src-tauri\target\release\bundle\nsis\Hydra DL_0.4.1_x64-setup.exe`.

## Doğrulanmış (bu oturum, kanıt var)

| Ne | Kanıt |
|---|---|
| Rust testleri + clippy | `cargo test` → **136 passed** (1 ağ testi `#[ignore]`); clippy `-D warnings` → 0 |
| Arayüz derlemesi | `npm run build` → 21.84 kB JS |
| Segmentli indirme | `node tools/e2e.mjs` → 16 MB, sha256 eşleşti |
| Web API + SSE | `node tools/web-smoke.mjs --port 6790` → "TÜMÜ GEÇTİ" |
| **yt-dlp kurulumu** | `POST /ytdlp/install` → 7 sn, `2026.10.21` sürümü doğrulandı |
| **YouTube probe** | 4K video → 49 format, başlık çözüldü, dosya adı başlıktan |
| **YouTube indirme** | 360p, 33 MB, `ftyp isom` MP4, ses+video birleşik, doğru dosya adı |
| **Torrent (yasal)** | Ubuntu 24.04 `.torrent` 3.1 GB @5 MB/s, 800 peer; Big Buck Bunny magnet 264 MB tamamlandı |
| **Torrent duraklat/devam** | `paused` → hız 0, `resume` → indirme sürdü; 4 yarım indirmeyle açılış çökmeden devam etti |
| LAN erişimi | `http://192.168.1.106:6790/health` → 200 |
| Masaüstü UI | `tools/ui-harness` → torrent dosya seçimi dahil, konsol hatası yok |
| Telefon arayüzü | `node tools/web-ui-check.mjs` → PASS (yt-dlp + torrent dosya seçimi + retry) |
| **Yeniden deneme** | 503×2→başarı, `Retry-After` beklendi, 404 → tekrar yok; canlı 0.2.4'te sha256 doğru |
| **Yol okuma açığı (kapandı)** | Canlı: `GET /C:/Windows/win.ini` → 400, kaynak dosya → 400, `/..%2f..%2fAGENTS.md` → 400 |
| **Güvenlik taraması** | `node tools/security-check.mjs` → PASS (koruma regresyonu + sır/yol) |
| **cargo audit** | `cd src-tauri && cargo audit` → exit 0 (1 gerekçeli istisna: ringbuf/librqbit-utp) |
| e2e 503 senaryosu | `tools/e2e.mjs` → 16 MB + 503'lü 2 MB kaynak, iki hash de doğru |
| **Tray (canlı, kurulu 0.3.0)** | `/health` → `"tray":true`; WM_CLOSE → pencere gizlendi, süreç + API yaşamaya devam etti (indirmeler sürer) |
| **Geçmişte arama/sıralama (0.3.1)** | `ui-harness` → arama 1 kayda indi, `ARSİV` → 2, Esc temizledi, boyut/ad sırası doğru, tercih kalıcı |
| **Telefon araması (0.3.1)** | `web-ui-check.mjs` → arama 1 karta indi, sayaç daraldı, eşleşmeyende bilgi metni |
| **Ayarlar hızı (0.3.2)** | yt-dlp `--version` 2,2 sn sürüyordu → 5 dk önbellek + çekmece anında: **58 ms** |
| **HLS (0.3.3)** | `cargo test` → 13 HLS testi (master/varyant, AES-128 NIST vektörü, DRM reddi, EXT-X-MAP, BYTERANGE reddi) |
| **HLS canlı (0.3.3)** | yerel sunucu: şifresiz 3×64 KB sha256 ✓, AES-128 çözüldü ✓, DRM → 400, duraklat→devam→completed ✓ |
| **DASH (0.3.4)** | `cargo test` → 9 MPD testi (SegmentTemplate/Timeline/List, BaseURL zinciri, DRM reddi) |
| **Depo sağlığı (0.3.4)** | `tools/hygiene.mjs` → temiz ağaç, .git 2,8 MB, izlenen büyük dosya yok |
| **Torrent yükleme (0.3.5)** | `web-ui-check.mjs` → dosya seçici `.torrent` gönderdi, "Eklendi: arsiv-2026" ipucu |
| **Tohum oranı (0.3.5)** | `cargo test` → oran hesabı + eski kayıt uyumu (eksik alanlar `0`, oran "—") |
| **Orphan .torrent (0.3.5)** | Canlı testte bulundu: kayıt silinince `.torrent` diskte kalıyordu → `remove_uploaded_torrent` (yalnız `torrents/` altı) |
| **Ağ yakalama (0.3.6)** | `net-capture-check.mjs` gerçek Edge+eklenti: m3u8/mpd/mp4 yakalandı, ts/css/js/png/json elendi, yakalanan HLS completed — 16/16 |
| **Eklenti birim (0.3.6)** | `test-extension.mjs` → **30 geçti** (9 yeni netlog testi) |
| **HTTPS (0.3.7)** | `check-web-tls.mjs`: HTTP→HTTPS geçişi, SAN'lar, http reddi, token 401→200, sertifika sabit — **21/21** |
| **Toplu içe aktarma (0.3.8)** | `test-links.mjs` 27; ui-harness + web-ui-check yeni senaryolar; canlı: 3 dosya indi, sha256 ✓ — **12/12** |
| **Sanal liste (0.3.9)** | `test-virtual.mjs` 28; ui-harness: 300 kayıtta DOM'da **14 satır**, sona kaydırma son kaydı çizdi, seçim→duraklat API'ye gitti |
| **yt-dlp güncelleme (0.4.0)** | canlı: kontrol 571 ms → ikinci 4 ms (önbellek), gerçek güncelleme 13 sn → "güncelleme yok" |
| Rust testleri | `cargo test` → **142 passed** (1 ağ testi `#[ignore]`); 6 yeni sürüm karşılaştırma testi |

## Bu oturumda düzeltilen gerçek hatalar

Tam liste ve kök nedenler: [`docs/HISTORY.md`](docs/HISTORY.md) (25 madde).

- **Güvenlik**: web sunucusu kök dışını servis ediyordu (`/C:/Windows/win.ini` → 200);
  token koruması prefix listesi yüzünden 2 ucu atlıyordu; localhost CSRF açıktı.
- Geçici sunucu hatalarında yeniden deneme (429/503 + `Retry-After`).
- Torrent'te dosya seçimi, bağlantı bütçesi 64, DHT'siz devam.
- Ağ yakalama: paralel istekler aynı listeyi ezmesin diye sekme başına yazma kuyruğu.

## Bilinen açık uçlar

- **Firefox/Zen**: `compat.js` + `HX.api` + `background.scripts` + gecko id ile paket hazır;
  19 statik test + kullanıcı Zen'de yükleyip çalıştırdı ("çözdüm şimdi oldu"). MV3 arka plan
  sayfası boşta about:debugging'de "Stopped" görünür — normal, hata değil.
- **Torrent etik sınırı:** motor gerçek ve yasal torrentlerle (Ubuntu, Big Buck Bunny)
  doğrulandı; telifli içerik test edilmedi, edilmeyecek.
- **Eklenti Chrome/Edge'de gerçek tarayıcıda doğrulandı** (Edge 156 + Playwright, 17 madde;
  `tools/ext-check/REPORT.md`). Chrome markalı derlemede `--load-extension` kapalı olduğu
  için Edge kullanıldı; elle yükleme yolu değişmedi.
- Torrent dosya seçimi canlı doğrulandı (0.2.3): liste + `files:[0,2]` doğru boyutlar.
- **Windows bildirimi çalışıyor** (kullanıcı teyit etti): biten indirmede toast görünüyor;
  AUMID + `notify-rust` yeterliydi. Telefon tarafı cihazda denenmedi.
- **Çerez/oturum**: eklenti `cookies` izniyle, kullanıcı indirmeyi başlattığında hedef
  adresin çerezlerini toplar ve **yalnız kullanıcının kendi Hydra sunucusuna** gönderir.
  Firefox'ta tüm siteler için host izni kullanıcı onayı ister; izin yoksa indirme
  **çerezsiz** sürer. Çerez `downloads.json`'da saklanır (duraklat/devam için) — dikkat.
- **Tray**: tepside sürer (indirmeler devam eder); çıkmak için tray → **Çıkış**.
  Tooltip'te aktif indirme + hız. Simge gizli kutuda olabilir.
- **HLS (`.m3u8`) yt-dlp olmadan**: segmentler paralel indirilir, AES-128 çözülür,
  master playlist'te en yüksek varyant seçilir, duraklat/devam çalışır; DRM reddedilir.
- **DASH (`.mpd`) yt-dlp olmadan**: `SegmentTemplate` (süre/sayı, `SegmentTimeline`),
  `SegmentList`, tek dosyalı `SegmentBase`, `BaseURL` zinciri. Ses ayrı akıştaysa **iki
  dosya** üretilir; birleştirme ffmpeg ister. Duraklatma dosyayı saklamaz (baştan indirir).
  Canlı akış ve `EXT-X-BYTERANGE` desteklenmez.
- **Depo sağlığı**: `node tools/hygiene.mjs` — git, büyük dosya, `.git` boyutu, `target/`
  şişmesi, geçici klasörler, disk eşiği, güvenlik. `--clean` geçici `hydra-*` klasörlerini
  siler. CI'da değil; elle/haftalık.
- **Telefondan torrent**: `.torrent` base64 ile `POST /torrent/upload`'a gider ve
  `<app_data>/torrents/<uuid>.torrent` olarak saklanır; kayıt silinince kopya gider.
  Tohum oranı `↑ 1.25`. **HTTPS** ayarlardan açılır; sertifika `<app_data>/tls/` altında,
  LAN adlarını kapsar, IP değişince yenilenir. Kendinden imzalı → tarayıcı uyarır
  (**Gelişmiş → devam et**). Yakalama sunucusu (`6712`) **HTTP kalır**.
- Avcı modunda `webRequest` yok; sayfa içi blob/manifest yakalama henüz yok.
- Yeniden deneme HTTP indirmesinde **ve** `probe` (HEAD/Range) aşamasında geçerli;
  `.torrent` çözümleme tek deneme yapar, medya (yt-dlp) kendi retry'ini yapar.

## Paketleme

İlk NSIS paketi web arayüzünü **404** veriyordu (`resources` `_up_/` altına koyuyordu);
nesne biçimi + `pick_webapp_dir` ile düzeldi. Paket her değişiklikten sonra **gerçekten
kurulup** denenmeli — `tauri dev` yetmez (`docs/TRAPS.md`).

## Sıradaki iş

1. **Telefon testi** (kullanıcı): `http://<LAN-IP>:6790` — torrent dosya seçimi, yt-dlp
   düğmesi, proxy alanı. Sunucu tarafı doğrulandı, cihazda denenmedi.
2. **Dış PR'lar (`smart-screen`, bizim repo değil):** #3 yol enjeksiyonu **doğru** (ham
   soketle 10/10); #1 `audit` kırmızısını gideriyor; #2 çakışıyor ve lockfile'da alakasız
   sürüm değişikliği var. Kullanıcı onayı bekliyor.
3. **Toplu içe aktarma (0.3.8) — BİTTİ (12/12 canlı).** Tek kaynak `webapp/links.js`:
   satır/virgül, `<sarmal>`, noktalama, `magnet`, yinelenen eleme, 500 sınırı.
   "Toplu" düğmesi + Ctrl+Shift+V; telefonda katlanır kutu; başarısızlar kutuda kalır.
4. **Toplu seçim + sanal liste (0.3.9) — BİTTİ.** Seçim kutuları, seçim çubuğu
   (duraklat/devam/iptal/çıkar), Ctrl+A. 150+ kayıtta yalnız görünen pencere çizilir
   (`src/virtual.ts`: değişken yükseklik, overscan, kaydırma kırpma). Kanıt: 300 kayıtta
   DOM'da 14 satır, sona kaydırma son kaydı çizdi, seçim→duraklat API'ye gitti.
5. **yt-dlp güncelleme (0.4.0) — BİTTİ, gerçek ağla doğrulandı.** `GET /ytdlp/latest`
   (GitHub `tag_name`, 30 dk önbellek) + saf `is_newer` (eksik bileşen 0, `.1` yeni).
   Masaüstü + telefonda "Güncelleme var mı?" / "Güncelle (sürüm)". Kanıt: kontrol 571 ms,
   ikinci kontrol 4 ms (ağa çıkmadı), gerçek güncelleme 13 sn → "güncelleme yok".
   **v0.3 dilimi 14/14 tamam.**
6. **Ayar yedekleme (0.4.1) — BİTTİ, canlı 21/21.** `GET /settings/export` +
   `POST /settings/import` (imza `hydra-dl-settings`, sürüm 1). Token varsayılan olarak
   **dışa aktarılmaz** (`includeToken=1` ile isteğe bağlı; `token` adı auth anahtarı,
   karıştırılmıyor). İçe aktarma birleştirir → token'sız yedek anahtarı silmez; bozuk/
   yabancı/gelecek sürüm 400 ve ayarlar değişmez; klasör oluşturulamazsa önceki korunur.
7. **v1.0 sırada:** TR/EN yerelleştirme → dayanıklılık testleri → otomatik güncelleme
   (imzalı manifest, anahtar kullanıcıda) → kod imzalı NSIS.
8. Küçükler: `ringbuf` muafiyeti `librqbit-utp` 0.4.9+/0.5.2+ çıkınca kalkar; torrent proxy
   desteği librqbit proxy kazanınca `proxyUrl`'e bağlanır.

## Oturum kesilirse ilk komut

```powershell
cd C:\Users\titan\Projeler\hydra-dl
git status --short
cargo test --manifest-path src-tauri\Cargo.toml
```
