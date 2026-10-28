# HANDOFF

> Yeni oturum: **önce bu dosyayı oku**, sonra `git status`. Bitirdiğin dilimi en alta 10 satır yaz.

**Son güncelleme:** 2026-10-28 · v0.2.6 · AGPL-3.0-or-later

---

## Nerede kaldık

v0.2 kapsamı çalışıyor ve **gerçek içerikle doğrulandı**: segmentli HTTP, akıllı kaynak bulucu,
yt-dlp medya indirme (tek tıkla kurulum dahil), BitTorrent motoru, masaüstü kalite paneli,
telefon için web arayüzü, tarayıcı eklentisi (panel + avcı modu).

**Repo public:** <https://github.com/TitanLion72/hydra-dl> (AGPL-3.0, ilk commit `9495fd4`).

**Kurulu sürüm:** v0.2.6, `%LOCALAPPDATA%\Hydra DL` (eski geçici kurulumlar ve kayıt
girdisi temizlendi, kullanıcının indirmeleri korundu). Kurulum paketi:
`src-tauri\target\release\bundle\nsis\Hydra DL_0.2.6_x64-setup.exe`.

## Doğrulanmış (bu oturum, kanıt var)

| Ne | Kanıt |
|---|---|
| Rust testleri | `cargo test` → **85 passed** (1 ağ testi `#[ignore]`) |
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
- **Windows bildirimi bu makinede görünmüyor (açık uç).** Kod tarafı tamam: `notify-rust`
  ile gönderim, AUMID kaydı (`HKCU\Software\Classes\AppUserModelId\com.hydradl.app`,
  DisplayName + IconUri yazıldı), `show()` hata döndürürse arayüzde gösteriliyor.
  **Teşhis (2026-10-28)**: PowerShell'in kendi toast'u da doğrulanamıyor —
  `ToastNotificationManager.History.GetHistory()` her AUMID için `0x80070490`
  (ERROR_NOT_FOUND) döndürüyor. Yani bu makinede bildirim **geçmişi** API'si çalışmıyor;
  programatik doğrulama mümkün değil, **görsel teyit gerekiyor**. Kullanıcıya sorulacak:
  biten bir indirmede toast görünüyor mu? Görünmüyorsa sıradaki adım kısayola
  `System.AppUserModel.ID` yazmak (IPropertyStore) veya Windows Ayarlar → Bildirimler'de
  Hydra DL'in kapalı olup olmadığına bakmak. Bildirim görünmese de indirme akışı bozulmuyor.
- Avcı modunda ağ isteği dinleme (`webRequest`) yok; HLS/manifest yakalama v0.3'te.
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

1. **Kullanıcı teyidi:** biten indirmede Windows toast görünüyor mu? (Kod + AUMID hazır;
   bu makinede bildirim geçmişi API'si çalışmadığı için programatik doğrulanamadı.)
   Görünmüyorsa: kısayola `System.AppUserModel.ID` yaz (IPropertyStore).
2. **Telefondan** `http://<LAN-IP>:6790`: torrent dosya seçimi + yt-dlp kur düğmesi
   gerçek cihazda.
3. **Dış PR'lar (`smart-screen`, bizim repo değil):** #3 yol enjeksiyonu düzeltmesi
   **doğru** (ham soketle 10/10 test geçti); #1 (tokio/vite güvenlik) `main`'deki
   `audit` kırmızısını gideriyor; #2 (#1 ile çakışıyor, lockfile'da alakasız sürüm
   değişikliği var) kapatılmalı. Kullanıcı onayı bekliyor.
4. **v0.3 devam:** çerez/oturum aktarımı (eklenti → uygulama), proxy (HTTP/SOCKS5),
   HLS/DASH'i yt-dlp olmadan indirme.
5. **CodeQL ilk analizi** bitince açık uyarı var mı diye bak (`gh api .../code-scanning/alerts`).
6. `ringbuf` muafiyeti: `librqbit-utp` 0.4.9+/0.5.2+ çıkınca `src-tauri/.cargo/audit.toml`
   istisnasını kaldır.

## Oturum kesilirse ilk komut

```powershell
cd C:\Users\titan\Projeler\hydra-dl
git status --short
cargo test --manifest-path src-tauri\Cargo.toml
```
