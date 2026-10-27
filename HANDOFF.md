# HANDOFF

> Yeni oturum: **önce bu dosyayı oku**, sonra `git status`. Bitirdiğin dilimi en alta 10 satır yaz.

**Son güncelleme:** 2026-10-27 (2) · v0.2.3 · AGPL-3.0-or-later

---

## Nerede kaldık

v0.2 kapsamı çalışıyor ve **gerçek içerikle doğrulandı**: segmentli HTTP, akıllı kaynak bulucu,
yt-dlp medya indirme (tek tıkla kurulum dahil), BitTorrent motoru, masaüstü kalite paneli,
telefon için web arayüzü, tarayıcı eklentisi (panel + avcı modu).

**Repo public:** <https://github.com/TitanLion72/hydra-dl> (AGPL-3.0, ilk commit `9495fd4`).

**Kurulu sürüm:** v0.2.3, `%LOCALAPPDATA%\Hydra DL` (eski geçici kurulumlar ve kayıt
girdisi temizlendi, kullanıcının indirmeleri korundu). Kurulum paketi:
`src-tauri\target\release\bundle\nsis\Hydra DL_0.2.3_x64-setup.exe`.

## Doğrulanmış (bu oturum, kanıt var)

| Ne | Kanıt |
|---|---|
| Rust testleri | `cargo test` → **67 passed** (1 ağ testi `#[ignore]`) |
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
14. **`.torrent` indirme sayfası torrent sanılıyordu** (kullanıcı bildirdi: "bu linkten indirme
    yapmıyor"). Bazı siteler `.torrent` adresinde ilk istekte HTML döndürüp gerçek dosyayı
    yalnızca çerez + tarayıcı UA ile veriyor. Artık `resolve_add_source`: magnet/info-hash →
    doğrudan, yerel dosya → okunur, `http(s)` → bencode mu? değilse çerezle tekrar, olmazsa
    sayfadaki gerçek `.torrent`/`magnet:` bağlantısı izlenir (`looks_like_torrent`,
    `find_torrent_link`, testli; yerel axum sunucusuyla iki uçtan uca test).
    Gerçek dosyadrive.vip linki: **79878 bayt geçerli .torrent** (yalnız metadata çekildi,
    içerik indirilmedi). Yasal Ubuntu `.torrent` ile `/add` → ad + 3,3 GB boyut doğrulandı.
15. **Eşzamanlı indirmelerde bağlantı patlaması.** "10 indirme × 16 bağlantı = 160 soket"
    olabiliyordu; her akış yavaşlıyor, toplam hız düşüyordu. Artık toplam 32 bağlantı bütçesi
    açık indirmelere paylaştırılıyor (`fair_connections`, testli: 1→16, 4→8, 10→3).
16. **Medya indirmeleri paralel parça çekmiyordu** (yt-dlp varsayılanı tek parça = yavaş)
    → `--concurrent-fragments 4`. Ayrıca **hız limiti medyada uygulanmıyordu** →
    `--limit-rate <bayt/sn>` (limit 0 ise eklenmez).
17. Ayarlarda "eşzamanlı indirme" açıklaması: hat tektir, N indirme = hız/N; tek dosya tam
    hız için 1 yap (`index.html`, `webapp/index.html`). webapp'te bağlantı sınırı 64→16.
18. **Telefon arayüzünde yt-dlp kurulum düğmesi yoktu** (API vardı, arayüz bağlanmamıştı).
    Ayarlar → Sunucu bölümünde durum + "yt-dlp kur" düğmesi: yoksa görünür, tıklayınca
    `POST /ytdlp/install` (120 sn zaman aşımı), kurulunca sürüm tazelenir ve düğme gizlenir;
    hata olursa kırmızı mesaj + yeniden denenebilir. `api()` artık istek başına zaman aşımı
    alıyor. Ayarlar ekranı ikinci kez açıldığında da yt-dlp durumu tazeleniyor (eski durum
    önbellekte kalmıyor); yanıt gelene kadar satır "kontrol ediliyor…" der ve kur düğmesi
    kilitli (sunucu `yt-dlp --version` çalıştırdığı için ilk yanıt ~1 sn gecikebiliyor).
    Yeni QA: `node tools/web-ui-check.mjs` (statik sunucu + mock API, 20 kontrol).

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

1. Telefondan `http://<LAN-IP>:6790` aç: QR eşleşmesi, indirme ekleme ve **yt-dlp kur**
   düğmesini gerçek cihazda dene.
2. Kullanıcı hız bölünmesini "Aynı anda indirme"=1 ile ölçsin; tek dosya hızı raporlansın.
3. v0.3: çerez/oturum aktarımı, `Retry-After` + backoff, proxy, yt-dlp'siz HLS/DASH.

## Oturum kesilirse ilk komut

```powershell
cd C:\Users\titan\Projeler\hydra-dl
git status --short
cargo test --manifest-path src-tauri\Cargo.toml
```
