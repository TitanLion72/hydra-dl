# HANDOFF

> Yeni oturum: **önce bu dosyayı oku**, sonra `git status`. Bitirdiğin dilimi en alta 10 satır yaz.

**Son güncelleme:** 2026-10-26 (akşam) · v0.2.1 · AGPL-3.0-or-later

---

## Nerede kaldık

v0.2 kapsamı çalışıyor ve **gerçek içerikle doğrulandı**: segmentli HTTP, akıllı kaynak bulucu,
yt-dlp medya indirme (tek tıkla kurulum dahil), BitTorrent motoru, masaüstü kalite paneli,
telefon için web arayüzü, tarayıcı eklentisi (panel + avcı modu).

**Repo public:** <https://github.com/TitanLion72/hydra-dl> (AGPL-3.0, ilk commit `9495fd4`).

## Doğrulanmış (bu oturum, kanıt var)

| Ne | Kanıt |
|---|---|
| Rust testleri | `cargo test` → **56 passed** |
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
| Masaüstü UI | `tools/ui-harness` → 38 OK, 4 ekran görüntüsü, konsol hatası yok |
| Eklenti (statik) | `node tools/test-extension.mjs` → 19 OK |

## Bu oturumda düzeltilen gerçek hatalar

0. **YouTube indirmesi `OSError: [Errno 22] Invalid argument` (kullanıcı bildirdi).**
   Video başlığında `:` vardı; Windows dosya adında yasak, `--windows-filenames` bile
   temizlemiyor, NTFS'te alternate data stream oluşup 0 baytta patlıyordu. Artık yt-dlp
   yalnız `%(id)s.%(ext)s` ile indiriyor, görünen adı `util::sanitize_filename` kuruyor
   (`rename_to_display_name`, testli). 1080p 401 MB gerçek indirmeyle doğrulandı.
1. **Video-only format sessiz iniyordu.** `yt-dlp` listesindeki video-only akışlar
   (`vcodec≠none, acodec=none`) seçilince ses gelmiyordu. Artık format kimliği
   `137+bestaudio/137` biçiminde üretiliyor (`extract.rs`, testli).
2. **Eklenti: `/probe` 4 sn zaman aşımına takılıyordu** (yt-dlp'li sayfalarda panel hata
   veriyordu) → `/probe` için 30 sn bütçe (`shared.js`).
3. **Eklenti: başarısız `POST /add` her 4xx'te `GET /add`'e düşüyordu** → çift ekleme riski;
   artık yalnız 404/405'te yedek yol (`shared.js`).
4. **Eklenti: sahte format kimliği** (`?? 0`) gönderiliyordu → gerçek kimlik yoksa alan hiç
   konmuyor (`panel.js`).
5. **Eklenti: `[hidden]` CSS'i eziliyordu** → `[hidden]{display:none!important}` (`panel.css`).
6. **Avcı modu: `blob:` adresi gönderilince sunucu 400 veriyordu.** Artık blob seçilirse
   **sayfa adresi** gönderilir, Hydra sayfayı tarayıp gerçek kaynağı bulur (`hunter.js`).
7. **Avcı modu: iframe'lerde üst çubuk çok kez çiziliyordu** → yalnız üst frame.
8. **Eklenti izinleri**: kullanılmayan `activeTab`/`scripting` kaldırıldı.
9. **Açılışta çökme (en kritik).** `setup()` ana thread'de çalışıyor, `spawn_worker`
   ise `tokio::spawn` çağırıyordu → yarım kalan indirme varken her açılışta
   `there is no reactor running` paniği ve 0xc0000409 (sessiz kapanma). Artık
   `tauri::async_runtime::spawn`. 4 yarım indirmeyle açılış test edildi.
10. **Çok dosyalı torrent'te `filePath` boş kalıyordu** (dosya/klasör aç çalışmıyordu).
    Yol artık torrentin dosya listesinden çözülüyor: tek dosya → dosya, çok dosya →
    indirme klasörü (`pick_completed_path`, testli).
11. **`GET /torrents` dokümanda vardı, kodda yoktu** (405). Uç nokta eklendi.
12. **`.torrent`/magnet listesinde ad "watch" görünüyordu**: medya sayfasında başlık elde
    olduğu halde URL'den ad üretiliyordu → `media_filename_from_title` (testli).
13. **Eklenti paneli indirmeye basınca 1.1 sn sonra kendini kapatıyordu** ("sayfa gidiyor").
    Artık açık kalır, sonucu gösterir; düğme "Eklendi" olur, ikinci tıklama çift eklemez.

## Bilinen açık uçlar

- **Firefox hiç çalıştırılmadı.** Paket ve `compat.js` statik + VM simülasyonuyla doğrulandı
  (19 test). Gerçek Firefox'ta `about:debugging` ile yüklenip denenmeli.
- **Torrent etik sınırı:** motor gerçek ve yasal torrentlerle (Ubuntu, Big Buck Bunny)
  doğrulandı; telifli içerik test edilmedi, edilmeyecek.
- **Eklenti Chrome/Edge'de gerçek tarayıcıda doğrulandı** (Edge 156 + Playwright, 17 madde;
  `tools/ext-check/REPORT.md`). Google Chrome markalı derlemede `--load-extension` kapalı
  olduğu için Chrome yerine Edge kullanıldı; elle yükleme yolu değişmedi.
- `webapp/` içinde yt-dlp kurulum düğmesi **yok** (API var, arayüz bağlanmadı).
- Avcı modunda ağ isteği dinleme (`webRequest`) yok; HLS/manifest yakalama v0.3'te.

## Paketleme (bu oturumda bulunan ve düzeltilen hata)

İlk NSIS paketi kurulduğunda web arayüzü **404** veriyordu: `resources: ["../webapp"]`
dosyaları `resource/_up_/webapp` altına koyuyor, kod ise `resource/webapp` arıyordu.
Düzeltme: `resources` nesne biçimine çevrildi (`{"../webapp": "webapp"}`) + `webapp_dir`
artık `_up_/webapp`, exe yanı ve cwd'yi de sırayla deniyor (`pick_webapp_dir`, testli).
**Kurulum paketi her değişiklikten sonra gerçekten kurulup denenmeli** — sadece `tauri dev`
yeterli değil.
- Masaüstü kurulum paketi (NSIS) denenmedi: `npm run tauri build`.

## Sıradaki iş

1. Kullanıcının bildirdiği **eklenti** ve **torrent** semptomlarını netleştir (aşağıdaki
   düzeltmeler bunları kapatmış olabilir; yeni 0.2.1 paketiyle tekrar denenmeli).
2. Telefondan `http://<LAN-IP>:6790` aç, QR eşleşmesini gör.
3. `webapp/`'e yt-dlp kurulum düğmesi ekle (`POST /ytdlp/install`).
4. v0.3: çerez/oturum aktarımı, `Retry-After` + backoff, proxy, yt-dlp'siz HLS/DASH.

## Oturum kesilirse ilk komut

```powershell
cd C:\Users\titan\Projeler\hydra-dl
git status --short
cargo test --manifest-path src-tauri\Cargo.toml
```
