# HANDOFF

> Yeni oturum: **önce bu dosyayı oku**, sonra `git status`. Bitirdiğin dilimi en alta 10 satır yaz.

**Son güncelleme:** 2026-11-12 · v0.6.0 · AGPL-3.0-or-later

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
| Rust testleri + clippy | `cargo test` → **167 passed** (1 ağ testi `#[ignore]`); clippy `-D warnings` → 0 |
| Arayüz derlemesi | `npm run build` → temiz (≈65.7 kB JS) |
| Segmentli indirme | `node tools/e2e.mjs` → 16 MB, sha256 eşleşti; 503'lü kaynak da ✓; kendi kayıtlarını siler |
| Web API + SSE | `node tools/web-smoke.mjs --port 6790` → "TÜMÜ GEÇTİ" |
| Yerelleştirme | `test-i18n` 25/25, `audit-i18n` envanter 293/293; canlı `PUT /settings {language}` kalıcı |
| **yt-dlp kurulumu / YouTube** | kurulum 7 sn (`2026.10.21`); 4K probe 49 format; 360p 33 MB MP4 indi |
| **Torrent (yasal)** | Ubuntu 24.04 3.1 GB @5 MB/s, 800 peer; Big Buck Bunny magnet 264 MB; 4 yarım indirmeyle çökmeden devam |
| **Torrent duraklat/devam** | `paused` → hız 0, `resume` → sürdü |
| LAN erişimi | `http://<LAN-IP>:6790/health` → 200 |
| Masaüstü UI | `tools/ui-harness` → dil senaryosu + torrent dosya seçimi, konsol hatası yok |
| Telefon arayüzü | `node tools/web-ui-check.mjs` → PASS (EN→TR, yt-dlp, torrent, retry) |
| **Yeniden deneme** | 503×2→başarı, `Retry-After` beklendi, 404 → tekrar yok |
| **Yol okuma açığı (kapandı)** | `GET /C:/Windows/win.ini` → 400, `/..%2f..%2fAGENTS.md` → 400 |
| **Güvenlik taraması** | `node tools/security-check.mjs` → PASS |
| **cargo audit** | exit 0 (1 gerekçeli istisna: ringbuf/librqbit-utp) |
| **Tray (canlı)** | `/health` → `"tray":true`; WM_CLOSE → pencere gizlendi, indirmeler sürdü |
| **Geçmişte arama/sıralama** | masaüstü + telefon harness: süzme, sıralama, tercih kalıcı |
| **Ayarlar hızı** | yt-dlp `--version` 2,2 sn → 5 dk önbellek + anında çekmece: **58 ms** |
| **HLS / DASH** | 13 HLS + 9 MPD testi; canlı HLS: şifresiz, AES-128, DRM→400, duraklat→devam |
| **Ağ yakalama** | `net-capture-check.mjs` gerçek Edge: m3u8/mpd/mp4 yakalandı, 16/16 |
| **HTTPS** | `check-web-tls.mjs` → 21/21 (SAN, http reddi, token 401→200) |
| **Toplu içe aktarma / sanal liste** | `test-links` 27, `test-virtual` 28; 300 kayıtta DOM'da **14 satır** |
| **Dayanıklılık (0.6.x)** | kopan bağlantı, atomik bitiş, `.hydra-tmp` sızıntısı, disk dolu, HLS parça retry; `soak.mjs` 20 dosya / 122 MB sha256 ✓ |

## Bu oturumda düzeltilen gerçek hatalar

Tam liste ve kök nedenler: [`docs/HISTORY.md`](docs/HISTORY.md) (25 madde).

- **Güvenlik**: web sunucusu kök dışını servis ediyordu; token koruması 2 ucu atlıyordu;
  localhost CSRF açıktı.
- **Dayanıklılık (0.6.x)**: `.hydra-tmp` sızıntısı, HLS/DASH'te hiç yeniden deneme
  olmaması, askıda kalan sunucuda sonsuz asılma (indirme **ve** probe).
- Diğer: 429/503 + `Retry-After`, torrent dosya seçimi, ağ yakalamada sekme başına kuyruk.

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
3. **Tamamlanan dilimler:** 0.3.8 toplu içe aktarma (`webapp/links.js`, 12/12 canlı) ·
   0.3.9 toplu seçim + sanal liste (300 kayıtta DOM'da 14 satır) · 0.4.0 yt-dlp güncelleme
   (`GET /ytdlp/latest`, 30 dk önbellek) · 0.4.1 ayar yedekleme (token varsayılan dışa
   yazılmaz, içe aktarma birleştirir) · 0.5.0 TR/EN yerelleştirme (envanter 293/293).
4. **Dayanıklılık testleri — 1. dilim (0.6.0) BİTTİ.** Bitiş doğrulaması saf
   `model::verify_parts`'a çıkarıldı (parça indeksi + boyut, bilinmeyen boyut atlanır) ve
   manager onu kullanıyor. Yeni testler: kopan bağlantı (sunucu gövdeyi yarıda keser →
   tekrar dene + kaldığı yerden devam → dosya birebir), eksik parçada hedef dosya
   **oluşmaz**, birleştirme hatasında `.hydra-tmp` artığı kalmaz (gerçek sızıntıydı,
   düzeltildi). `cargo test` 164.
5. **Dayanıklılık — 2. dilim BİTTİ.** Disk dolu / yazma hatası testi (G/Ç hatası yeniden
   denenmez, panik yok — regresyon kilidi). `tools/soak.mjs`: N eşzamanlı indirme, her
   dosya farklı desen+boyut, sha256 birebir; 20 dosya / 122 MB / 1.1 sn. Test artıkları
   artık geçmişte birikmiyor (e2e + soak kendi kayıtlarını siler, kullanıcınınkine
   dokunmaz). `cargo test` 165.
6. **HLS/DASH parça dayanıklılığı (0.6.2) — BİTTİ.** `engine/fetch.rs` ortak segment
   yoluna yeniden deneme eklendi: geçici hata (ağ, gövde kesintisi, 5xx, 429) → 4 denemeye
   kadar üstel geri çekilme (200/400/800 ms); kalıcı hata (404/403, sınır aşımı) hemen
   döner. Önceden HLS'te tek parça hatası tüm indirmeyi öldürüyordu. `is_transient` saf +
   testli; uçtan uca test: 503×2 → parça indi. `cargo test` 167.
7. **Yeniden başlatma tutarlılığı (0.6.3) — BİTTİ.** Çoklu yarım indirme testi: açılışta
   yalnız `downloading` olanlar kuyruğa döner, `paused`/`queued` dokunulmaz, indirilen bayt
   ve segment ilerlemesi korunur, hiçbiri sahte `completed` olmaz. Ayrıca kaydet/yükle
   döngüsü idempotent (her açılışta durum daha fazla bozulmaz). `cargo test` 169.
8. **Askıda kalan sunucu (0.6.4–0.6.7) — BİTTİ, canlı doğrulandı.** Sunucu bağlanıp gövde
   göndermezse indirme **sonsuza kadar asılı kalıyordu** (testte 136 sn). Sınırlar: yanıt
   başlıkları 30 sn, gövde sessizliği 30 sn, **probe da zaman aşımlı** (probe hiç sınırsızdı;
   canlı ölçümün uzamasının sebebi). Askıda kalmada **tekrar denenmez** (30 sn sessizlik
   anlık blip değildir; mesaj: "sunucu yanıt vermedi, tekrar denenmedi"). `tools/hang-check.mjs`
   gerçek asılı sunucuyu senaryolaştırır. `cargo test` 172.
9. **v1.0 sırada:** otomatik güncelleme (imzalı manifest, anahtar kullanıcıda) → kod
   imzalı NSIS.
10. Küçükler: `ringbuf` muafiyeti `librqbit-utp` 0.4.9+/0.5.2+ çıkınca kalkar; torrent proxy
    desteği librqbit proxy kazanınca `proxyUrl`'e bağlanır.

## Oturum kesilirse ilk komut

```powershell
cd C:\Users\titan\Projeler\hydra-dl
git status --short
cargo test --manifest-path src-tauri\Cargo.toml
```
