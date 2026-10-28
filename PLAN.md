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

- [ ] Çerez/oturum aktarımı: eklenti → uygulama (giriş isteyen sayfalar)
- [x] `Retry-After` + üstel geri çekilme; 429/503 yönetimi (segment başına 5 deneme,
      `Download.retry` arayüzde)
- [ ] `/probe` ve `.torrent` çözümlemede de yeniden deneme (şimdilik tek deneme)
- [ ] Proxy (sistem + elle HTTP/SOCKS5); torrent için de geçerli
- [ ] HLS/DASH'i yt-dlp olmadan indirme (segment birleştirme)
- [ ] Avcı modunda ağ isteği dinleme (`webRequest`) ile blob/manifest yakalama
- [ ] Bildirimler (tamamlandı/hata), tray simgesi, arka planda küçültme
- [ ] Web arayüzünde torrent ekleme, tohum oranı ve **yt-dlp kurulum düğmesi**
- [ ] Toplu içe aktarma: bağlantı listesi, panodan otomatik yakalama (opt-in)
- [ ] İndirme geçmişinde arama, sıralama, toplu seçim, sanal liste (>200 kayıt)
- [ ] yt-dlp otomatik güncelleme (sürüm kontrolü + tek tık)

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
