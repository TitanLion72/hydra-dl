# HANDOFF

> Yeni oturum: **önce bu dosyayı oku**, sonra `git status`. Bitirdiğin dilimi en alta 10 satır yaz.

**Son güncelleme:** 2026-10-31 · v0.3.4 · AGPL-3.0-or-later

---

## Nerede kaldık

v0.2 kapsamı çalışıyor ve **gerçek içerikle doğrulandı**: segmentli HTTP, akıllı kaynak bulucu,
yt-dlp medya indirme (tek tıkla kurulum dahil), BitTorrent motoru, masaüstü kalite paneli,
telefon için web arayüzü, tarayıcı eklentisi (panel + avcı modu).

**Repo public:** <https://github.com/TitanLion72/hydra-dl> (AGPL-3.0, ilk commit `9495fd4`).

**Kurulu sürüm:** v0.3.4, `%LOCALAPPDATA%\Hydra DL` (eski geçici kurulumlar ve kayıt
girdisi temizlendi, kullanıcının indirmeleri korundu). Kurulum paketi:
`src-tauri\target\release\bundle\nsis\Hydra DL_0.3.4_x64-setup.exe`.

## Doğrulanmış (bu oturum, kanıt var)

| Ne | Kanıt |
|---|---|
| Rust testleri | `cargo test` → **127 passed** (1 ağ testi `#[ignore]`) |
| Clippy | `cargo clippy --all-targets -- -D warnings` → 0 uyarı |
| Arayüz derlemesi | `npm run build` → 21.84 kB JS |
| Segmentli indirme | `node tools/e2e.mjs` → 16 MB, sha256 eşleşti |
| Web API + SSE | `node tools/web-smoke.mjs --port 6790` → "TÜMÜ GEÇTİ" |
| **yt-dlp kurulumu** | `POST /ytdlp/install` → 7 sn, `2026.10.21` sürümü doğrulandı |
| **YouTube probe** | 4K video → 49 format, başlık çözüldü, dosya adı başlıktan |
| **YouTube indirme** | 360p, 33 MB, `ftyp isom` MP4, ses+video birleşik, doğru dosya adı |
| **Torrent (yasal)** | Ubuntu 24.04 `.torrent` 3.1 GB @5 MB/s, 800 peer; Big Buck Bunny magnet 264 MB tamamlandı |
| **Torrent duraklat/devam** | `paused` → hız 0, `resume` → indirme sürdü |
| **Açılışta kurtarma** | 4 yarım indirme varken açılış çökmeden devam etti |
| LAN erişimi | `http://192.168.1.106:6790/health` → 200 |
| Masaüstü UI | `tools/ui-harness` → torrent dosya seçimi dahil, konsol hatası yok |
| Eklenti (statik) | `node tools/test-extension.mjs` → 19 OK |
| Telefon arayüzü | `node tools/web-ui-check.mjs` → PASS (yt-dlp + torrent dosya seçimi + retry) |
| **Yeniden deneme** | Yerel axum: 503×2→başarı (2 tekrar), `Retry-After: 1` → 1 sn bekleme, 404 → tekrar yok, 5. denemede vazgeçme |
| Retry (canlı, kurulu 0.2.4) | probe 2×503 → `add` 2,0 sn sonra başarılı; segment 2×503 → `/downloads` `retry {attempt:2,max:5,waitMs:2000}`, sha256 doğru |
| **Yol okuma açığı (kapandı)** | Canlı: `GET /C:/Windows/win.ini` → 400, `GET /C:/Users/.../web.rs` → 400, `/..%2f..%2fAGENTS.md` → 400, `GET /app.js` → 200 |
| **Güvenlik taraması** | `node tools/security-check.mjs` → PASS (koruma regresyonu + sır/yol) |
| **cargo audit** | `cd src-tauri && cargo audit` → exit 0 (1 gerekçeli istisna: ringbuf/librqbit-utp) |
| e2e 503 senaryosu | `tools/e2e.mjs` → 16 MB + 503'lü 2 MB kaynak, iki hash de doğru |
| **Tray (canlı, kurulu 0.3.0)** | `/health` → `"tray":true`; WM_CLOSE → pencere gizlendi, süreç + API yaşamaya devam etti (indirmeler sürer) |
| **Geçmişte arama/sıralama (0.3.1)** | `tools/ui-harness` → arama 1 kayda indirdi, `ARSİV` → 2 kayıt (Türkçe/büyük harf), Esc temizledi, boyut/ad sıralaması doğru, tercih yeniden yüklemede korundu |
| **Telefon araması (0.3.1)** | `node tools/web-ui-check.mjs` → arama 1 karta indirdi, sayaç daraldı, eşleşmeyende bilgi metni, temizleyince 2 kart geri geldi |
| **Ayarlar hızı (0.3.2)** | yt-dlp `--version` her açılışta 2,2 sn sürüyordu → 5 dk TTL önbellek + çekmece anında açılıyor: `ui-harness` 1,5 sn gecikmeli API'de **58 ms** |
| **HLS (0.3.3)** | `cargo test` → 13 HLS testi (master/varyant, AES-128 NIST vektörü, DRM reddi, EXT-X-MAP, BYTERANGE reddi) |
| **HLS canlı (kurulu 0.3.3)** | yerel test sunucusu: şifresiz akış 3×64 KB sha256 ✓, AES-128 çözüldü sha256 ✓, DRM (`SAMPLE-AES`) → 400 "DRM indirmez", duraklat→paused (bayt artmadı)→devam→completed sha256 ✓ |
| **DASH (0.3.4)** | `cargo test` → 9 MPD testi (SegmentTemplate/Timeline/List, BaseURL zinciri, `$Number%05d$`, DRM reddi) + motor testi |
| **Depo sağlığı (0.3.4)** | `node tools/hygiene.mjs` → temiz ağaç, .git 2,8 MB, izlenen büyük dosya yok, disk eşiği |

## Bu oturumda düzeltilen gerçek hatalar

Tam liste ve kök nedenler: [`docs/HISTORY.md`](docs/HISTORY.md) (23 madde).
Bu oturumun öne çıkanları:

- **Güvenlik**: web sunucusu kök dışını servis ediyordu (`/C:/Windows/win.ini` → 200);
  token koruması prefix listesi yüzünden 2 ucu atlıyordu; localhost CSRF açıktı. Üçü de
  kapatıldı + `SECURITY.md` / AGENTS kuralları + CodeQL/audit/security-check CI'da.
- Geçici sunucu hatalarında yeniden deneme (429/503 + `Retry-After`).
- Torrent'te dosya seçimi, bağlantı bütçesi 64, DHT'siz devam.
- Bitiş/hata bildirimi (notify-rust) — toast görünürlüğü kullanıcı teyidi bekliyor.


## Bilinen açık uçlar

- **Firefox/Zen**: `compat.js` + `HX.api` + `background.scripts` + gecko id ile paket hazır;
  19 statik test + kullanıcı Zen'de yükleyip çalıştırdı ("çözdüm şimdi oldu"). MV3 arka plan
  sayfası boşta about:debugging'de "Stopped" görünür — normal, hata değil.
- **Torrent etik sınırı:** motor gerçek ve yasal torrentlerle (Ubuntu, Big Buck Bunny)
  doğrulandı; telifli içerik test edilmedi, edilmeyecek.
- **Eklenti Chrome/Edge'de gerçek tarayıcıda doğrulandı** (Edge 156 + Playwright, 17 madde;
  `tools/ext-check/REPORT.md`). Google Chrome markalı derlemede `--load-extension` kapalı
  olduğu için Chrome yerine Edge kullanıldı; elle yükleme yolu değişmedi.
- `webapp/` içinde yt-dlp kurulum düğmesi **var** (0.2.3). Telefondan gerçek kurulum
  denenmedi: arayüz mock API ile, sunucu tarafı gerçek istekle doğrulandı.
- Torrent dosya listesi/seçimi **kurulu uygulamada canlı doğrulandı** (0.2.3, yerel 3 dosyalı
  `.torrent`): `POST /torrent/files` → 3 dosya + doğru boyutlar; `POST /add` `files:[0,2]` →
  `totalBytes=400000`; `POST /torrent/files {id,files:[1]}` → `totalBytes=200000`,
  `a.bin:false, b.bin:true, c.bin:false`. Kayıt silindi, artık bırakılmadı.
- **Windows bildirimi çalışıyor** (kullanıcı teyit etti, 2026-10-29): biten indirmede toast
  görünüyor. AUMID kaydı + `notify-rust` yeterliydi. Telefon tarafı kullanıcı tarafından
  henüz denenmedi (sunucu tarafı test edildi).
- **Çerez/oturum**: eklenti `cookies` izniyle, kullanıcı indirmeyi başlattığında hedef
  adresin çerezlerini toplar ve **yalnız kullanıcının kendi Hydra sunucusuna** gönderir.
  Firefox'ta tüm siteler için host izni kullanıcı onayı ister; izin yoksa indirme
  **çerezsiz** sürer (sessiz hata yok, akış bozulmaz). Çerez `downloads.json`'da saklanır
  (duraklat/devam için) — dosyayı paylaşırken dikkat.
- **Tray**: pencere kapandığında uygulama tepside çalışmaya devam eder (indirmeler sürer);
  gerçekten çıkmak için tray menüsünden **Çıkış**. Tooltip'te aktif indirme + toplam hız.
  Windows'ta tray simgesi gizli simge kutusunda olabilir (Ayarlar → Görev çubuğu).
- **HLS (`.m3u8`) yt-dlp olmadan**: segmentler paralel indirilir, AES-128 çözülür,
  master playlist'te en yüksek varyant seçilir, duraklat/devam çalışır; DRM reddedilir.
- **DASH (`.mpd`) yt-dlp olmadan**: `SegmentTemplate` (süre/sayı ve `SegmentTimeline`),
  `SegmentList`, tek dosyalı `SegmentBase`, `BaseURL` zinciri. Ses ayrı akıştaysa
  **iki dosya** üretilir (`film.mp4` + `film.ses.m4a`); birleştirme ffmpeg gerektirir.
  DASH'te duraklatma dosyayı **saklamaz** (segmentler sıralı yazıldığı için yarım dosya
  anlamsız), devam ederken baştan indirir. Canlı akış ve `EXT-X-BYTERANGE` desteklenmez.
- **Depo sağlığı**: `node tools/hygiene.mjs` — git durumu, izlenen büyük dosya, `.git`
  boyutu, `target/` şişmesi, geçici klasörler, disk eşiği, güvenlik taraması. `--clean`
  geçici `hydra-*` klasörlerini siler. CI'da değil; elle/haftalık koşulur.- Avcı modunda ağ isteği dinleme (`webRequest`) yok; sayfa içi manifest yakalama
  (avcı modunda blob/manifest) henüz yok.
- Yeniden deneme HTTP indirmesinde **ve** `probe` (HEAD/Range keşfi) aşamasında geçerli.
  `.torrent` çözümleme tek deneme yapar (kullanıcı yeniden basar); medya (yt-dlp) kendi
  retry'ini yapar.

## Paketleme (bu oturumda bulunan ve düzeltilen hata)

İlk NSIS paketi kurulduğunda web arayüzü **404** veriyordu: `resources: ["../webapp"]`
dosyaları `resource/_up_/webapp` altına koyuyor, kod ise `resource/webapp` arıyordu.
Düzeltme: `resources` nesne biçimine çevrildi (`{"../webapp": "webapp"}`) + `webapp_dir`
artık `_up_/webapp`, exe yanı ve cwd'yi de sırayla deniyor (`pick_webapp_dir`, testli).
**Kurulum paketi her değişiklikten sonra gerçekten kurulup denenmeli** — sadece `tauri dev`
yeterli değil.
- Masaüstü kurulum paketi (NSIS) denenmedi: `npm run tauri build`.

## Sıradaki iş

1. **Telefon testi** (kullanıcı): `http://<LAN-IP>:6790` — torrent dosya seçimi, yt-dlp
   düğmesi, proxy alanı. Sunucu tarafı doğrulandı, cihazda denenmedi.
2. **Dış PR'lar (`smart-screen`, bizim repo değil):** #3 yol enjeksiyonu **doğru** (ham
   soketle 10/10); #1 `audit` kırmızısını gideriyor; #2 çakışıyor ve lockfile'da alakasız
   sürüm değişikliği var. Kullanıcı onayı bekliyor.
3. **Kalan güvenlik sertleştirmesi:** isteğe bağlı HTTPS (yerel ağda self-signed) —
   token düz metin gitmesin. (Sabit zamanlı token + hız sınırı 0.2.8'de bitti.)
4. **v0.3 devam:** web arayüzünde torrent ekleme, avcı modunda manifest yakalama,
   DASH'te kaldığı yerden devam (segment dosyalarını saklayarak). (Çerez 0.2.9,
   tray 0.3.0, arama/sıralama 0.3.1, HLS 0.3.3, DASH 0.3.4'te bitti.)
5. `ringbuf` muafiyeti: `librqbit-utp` 0.4.9+/0.5.2+ çıkınca `src-tauri/.cargo/audit.toml`
   istisnasını kaldır.
6. Torrent proxy desteği: librqbit sürümü proxy kazanırsa `proxyUrl` oraya da bağlanır.

## Oturum kesilirse ilk komut

```powershell
cd C:\Users\titan\Projeler\hydra-dl
git status --short
cargo test --manifest-path src-tauri\Cargo.toml
```
