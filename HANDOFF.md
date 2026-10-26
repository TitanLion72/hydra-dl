# HANDOFF

> Yeni oturum: **önce bu dosyayı oku**, sonra `git status`. Bitirdiğin dilimi en alta 10 satır yaz.

**Son güncelleme:** 2026-10-26 (sabah) · v0.2.0 · AGPL-3.0-or-later

---

## Nerede kaldık

v0.2 kapsamı çalışıyor ve **gerçek içerikle doğrulandı**: segmentli HTTP, akıllı kaynak bulucu,
yt-dlp medya indirme (tek tıkla kurulum dahil), BitTorrent motoru, masaüstü kalite paneli,
telefon için web arayüzü, tarayıcı eklentisi (panel + avcı modu).

**Repo public:** <https://github.com/TitanLion72/hydra-dl> (AGPL-3.0, ilk commit `9495fd4`).

## Doğrulanmış (bu oturum, kanıt var)

| Ne | Kanıt |
|---|---|
| Rust testleri | `cargo test` → **50 passed** |
| Clippy | `cargo clippy --all-targets -- -D warnings` → 0 uyarı |
| Arayüz derlemesi | `npm run build` → 21.84 kB JS |
| Segmentli indirme | `node tools/e2e.mjs` → 16 MB, sha256 eşleşti |
| Web API + SSE | `node tools/web-smoke.mjs --port 6790` → "TÜMÜ GEÇTİ" |
| **yt-dlp kurulumu** | `POST /ytdlp/install` → 7 sn, `2026.10.21` sürümü doğrulandı |
| **YouTube probe** | 4K video → 49 format, başlık çözüldü |
| **YouTube indirme** | 360p, 33 MB, `ftyp isom` MP4, ses+video birleşik, doğru dosya adı |
| LAN erişimi | `http://192.168.1.106:6790/health` → 200 |
| Masaüstü UI | `tools/ui-harness` → 38 OK, 4 ekran görüntüsü, konsol hatası yok |

## Bu oturumda düzeltilen gerçek hatalar

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

## Bilinen açık uçlar

- **Torrent gerçek magnet ile denenmedi.** Parse + motor testleri var; etik/yasal nedenle
  canlı torrent indirilmedi. Kullanıcı onayıyla denenmeli.
- **Eklenti Chrome'a yüklenip elle denenmedi.** Sözleşme ve DOM testleri geçti; gerçek
  tarayıcıda sağ tık → panel akışı bir kez gözle görülmeli.
- **Web arayüzü gerçek telefonda açılmadı.** LAN + SSE sunucu tarafında doğrulandı.
- `webapp/` içinde yt-dlp kurulum düğmesi **yok** (API var, arayüz bağlanmadı).
- Avcı modunda ağ isteği dinleme (`webRequest`) yok; HLS/manifest yakalama v0.3'te.
- Masaüstü kurulum paketi (NSIS) denenmedi: `npm run tauri build`.

## Sıradaki iş

1. Eklentiyi Chrome'a yükle, gerçek sitede panel + avcı modunu gözle doğrula.
2. Telefondan `http://<LAN-IP>:6790` aç, QR eşleşmesini gör.
3. `webapp/`'e yt-dlp kurulum düğmesi ekle (`POST /ytdlp/install`).
4. Onay gelirse bir magnet ile torrent akışını doğrula.
5. `npm run tauri build` ile NSIS paketini üret ve kur.

## Oturum kesilirse ilk komut

```powershell
cd C:\Users\titan\Projeler\hydra-dl
git status --short
cargo test --manifest-path src-tauri\Cargo.toml
```
