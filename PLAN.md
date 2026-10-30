# PLAN

v0.2 = "IDM'den iyi" temeli. Sıra bağımlılığa göre; üstteki bitmeden alttakine geçilmez.

## v0.2 — tamamlanan

- [x] Segmentli HTTP indirme, devam, kuyruk, hız limiti (v0.1 çekirdeği)
- [x] yt-dlp: format listesi (`-J`), kalite seçimi, ilerleme, iptal
- [x] **Akıllı kaynak bulucu** (`extract.rs`): meta/og/JSON-LD/video/iframe/script tarama,
      HLS varyant çözümü, reklam ve `blob:`/`data:` ayıklama, bilinen medya siteleri
- [x] **BitTorrent/P2P** (librqbit): magnet + .torrent, pause/resume, peer sayısı,
      açılışta yeniden bağlanma
- [x] **Kalite/format paneli** (masaüstü): İncele → format tablosu, dosya adı, diğer kaynaklar
- [x] **Telefon için web arayüzü**: `webapp/` + LAN sunucusu, SSE, PWA, token, QR eşleşme
- [x] **Tek API** : yakalama ve web sunucusu aynı yönlendiriciyi kullanır (`docs/API.md`)
- [x] Tarayıcı eklentisi: IDM tarzı panel, avcı modu, paneli kapatma ayarı
- [x] **yt-dlp tek tıkla kurulum** (GitHub releases → `<app_data>/bin`, doğrulamalı)
- [x] **Video-only formatlara otomatik ses ekleme** (`137+bestaudio/137`)
- [x] AGPL-3.0 lisansı, GitHub şablonu, CI, README/ARCHITECTURE/FAQ
- [x] Görsel/UI koşum takımı: `tools/ui-harness` (mock API + Playwright)
- [x] Sağlamlık: `.torrent` indirme sayfası takibi (çerez/UA), bağlantı bütçesi
      (toplam 64 soket), medyada parça eşzamanlılığı + hız limiti
- [x] Web arayüzünde yt-dlp kurulum düğmesi + `tools/web-ui-check.mjs`
- [x] **Torrent'te dosya seçimi**: `POST /torrent/files` (liste + seçim), indirme
      durmadan seçim değiştirme, masaüstü + telefon arayüzü

## v0.3 — bir sonraki dilim

- [x] **Çerez/oturum aktarımı**: eklenti `cookies` izni → `/probe` + `/add` `cookie` →
      `Cookie` başlığı + yt-dlp `--add-header`; CRLF/8 KB doğrulaması
- [x] `Retry-After` + üstel geri çekilme (segment başına 5 deneme, `Download.retry`)
- [x] **Proxy** (HTTP/HTTPS/SOCKS5): tek ayar, HTTP + probe + yt-dlp; torrent hariç
- [x] **HLS indirme (yt-dlp olmadan)**: `.m3u8` → segmentler paralel çekilir,
      AES-128 şifre çözme, master playlist'te en yüksek varyant, fMP4 `EXT-X-MAP`,
      duraklat/devam; DRM (`SAMPLE-AES` vb.) **reddedilir**
- [ ] DASH (`.mpd`) indirme (yt-dlp olmadan)
- [ ] Avcı modunda ağ isteği dinleme (`webRequest`) ile blob/manifest yakalama
- [x] Bildirimler (tamamlandı/hata) — `notify-rust` + AUMID, kullanıcı teyit etti
- [x] **Tray simgesi**: menü (Göster/Gizle, Tümünü duraklat/devam, Çıkış), sol tık
      pencereyi açar, pencere kapatınca tepsiye küçülür, tooltip'te aktif indirme + hız
- [ ] Web arayüzünde torrent ekleme ve tohum oranı
- [ ] Toplu içe aktarma: bağlantı listesi, panodan otomatik yakalama (opt-in)
- [x] **Geçmişte arama + sıralama**: ad/adres araması (Türkçe/büyük-küçük harf
      duyarsız), sıralama (tarih/ad/boyut/durum, tercih kalıcı), Ctrl+F ve `/`
      kısayolları, aramada boş durum metni
- [ ] Geçmişte toplu seçim, sanal liste (>200 kayıt)
- [ ] yt-dlp otomatik güncelleme (sürüm kontrolü + tek tık)

## Güvenlik — tamamlanan (2026-10-28)

- [x] **Yol okuma açığı kapatıldı** (`resolve_static_path`: bileşen doğrulama +
      `canonicalize`; symlink dahil)
- [x] **Token koruması varsayılan reddet** (`needs_token`) — yeni uç otomatik korunur
- [x] **Origin kontrolü** (`origin_guard`) — localhost CSRF kapalı
- [x] `SECURITY.md` tehdit modeli + 10 bağlayıcı kural; AGENTS'te "Güvenlik değişmezleri"
- [x] CI: **CodeQL** (Rust + JS/TS), `cargo audit`, `npm audit`,
      `node tools/security-check.mjs`
- [ ] CodeQL ilk analiz sonuçlarını gözden geçir
- [ ] Rate limit (dakikada N istek) — token kaba kuvvetine karşı
- [ ] Token karşılaştırmasını sabit zamanlı yap (`subtle::ConstantTimeEq`)
- [ ] İsteğe bağlı HTTPS (yerel ağda self-signed) — token düz metin gitmesin

## v1.0 — dağıtım

- [ ] Otomatik güncelleme (tauri-plugin-updater + imzalı manifest)
- [ ] Kod imzalı NSIS kurulumu, sürüm notları
- [ ] TR/EN yerelleştirme, ayarların dışa/içe aktarımı
- [ ] Dayanıklılık: güç kesintisi, disk dolu, ağ değişimi otomatik testleri
- [ ] Linux/macOS deneysel hedefler

## Bilinçli olarak yapılmayacaklar

- DRM korumalı içerik indirme
- Ödeme duvarı / reklam atlatma
- Kaynağı gizleyen veya platform şartlarını ihlal eden toplu kazıyıcılar
