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
- [x] **DASH indirme (yt-dlp olmadan)**: `.mpd` → `SegmentTemplate` (süre/sayı ve
      `SegmentTimeline`), `SegmentList`, tek dosyalı `SegmentBase`, `BaseURL` zinciri,
      en yüksek varyant; `ContentProtection` (DRM) **reddedilir**. Ses ayrı akıştaysa
      ikinci dosya üretilir (birleştirme ffmpeg ile).
- [x] **Avcı modunda ağ isteği dinleme (`webRequest`) ile blob/manifest yakalama**:
      `m3u8`/`mpd`/`mp4`/ses istekleri sekme başına yakalanır, panelde listelenir,
      tek tıkla indirilir; HLS/DASH parçaları listeye alınmaz (gürültü olmasın).
      Varsayılan **kapalı**, yalnız medya istekleri saklanır.
- [x] Bildirimler (tamamlandı/hata) — `notify-rust` + AUMID, kullanıcı teyit etti
- [x] **Tray simgesi**: menü (Göster/Gizle, Tümünü duraklat/devam, Çıkış), sol tık
      pencereyi açar, pencere kapatınca tepsiye küçülür, tooltip'te aktif indirme + hız
- [x] **Web arayüzünde torrent ekleme**: telefondan `.torrent` yükleme
      (`POST /torrent/upload`, base64 + bencode doğrulama, kalıcı klasör) ve
      tohum oranı (`↑ 1.25`) masaüstü + telefon satırlarında
- [x] Toplu içe aktarma: yapıştırılan listeden bağlantı çıkarma, yinelenen eleme,
      bağlantı olmayan satırları gösterme, sıralı ekleme + başarısızları kutuda bırakma.
      Ayrıştırıcı **tek kaynak** (`webapp/links.js`), masaüstü + telefon aynı kodu kullanır.
      Panoda birden çok bağlantı varsa "Yapıştır" doğrudan toplu pencereyi açar; Ctrl+Shift+V.
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
- [x] CodeQL analizi yapıldı; bulgular ya düzeltildi ya `SECURITY.md`'de gerekçeyle
      kabul edildi (SSRF, download_dir)
- [x] Rate limit (10 dk / 20 hatalı token → 429) — `AuthLimiter` (0.2.8)
- [x] Token karşılaştırmasını sabit zamanlı yap (`subtle`/elle) — `token_matches` (0.2.8)
- [x] İsteğe bağlı HTTPS (yerel ağda self-signed): `tls.rs` — sertifika
      `<app_data>/tls/` altında, LAN adreslerini kapsar, IP değişince yenilenir;
      ayarlarda "HTTPS kullan" anahtarı (varsayılan kapalı)

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
