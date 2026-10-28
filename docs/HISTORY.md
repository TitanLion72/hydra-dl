# Düzeltilen Gerçek Hatalar — Arşiv

Bu dosya `HANDOFF.md`'den taşındı (HANDOFF kısa kalmalı). Her madde **kullanıcı bildirimi
veya canlı test** sonucu bulunmuş, düzeltilmiş ve doğrulanmış gerçek bir hatadır.
Numaralar kronolojiktir; yeni hata eklerken en alta ekle.

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
9. **Açılışta çökme** (`setup()` ana thread'de `tokio::spawn` → "no reactor running",
   0xc0000409). `tauri::async_runtime::spawn` kullanılıyor.
10. **`.torrent` indirme sayfası** (dosyadrive.vip gibi siteler HTML dönüyordu): tarayıcı
    UA'sı + çerez ile tekrar deneme, ardından sayfadaki gerçek `.torrent`/`magnet:` linkini
    izleme (`torrent.rs`, yerel axum testleri).
11. **Torrent'te dosya yolu boştu** (çok dosyalı torrentte "dosyayı aç" ölüydü).
12. **Eşzamanlı indirmelerde bağlantı patlaması**: 10 indirme × 16 bağlantı = 160 soket.
    Toplam bütçe eklendi (`fair_connections`), sonra 64'e çıkarıldı.
13. **Medyada hız limiti uygulanmıyordu** → `--limit-rate`; parça eşzamanlılığı
    `--concurrent-fragments 4`.
14. **Ayarlardaki "hız N'e bölünür" metni yanlıştı.** Uygulama hızı bölmüyor; sınır hattın
    kapasitesi ve sunucunun bağlantı başına limiti. Metin düzeltildi.
15. **Telefon arayüzünde yt-dlp kurulum düğmesi yoktu** → `POST /ytdlp/install` bağlandı,
    kurulum sonrası sürüm tazeleniyor, hata kırmızı mesajla gösteriliyor.
16. **yt-dlp durumu önbellekte kalıyordu** (ayarlar ikinci açılışta "—" gösteriyordu) ve
    yavaş `--version` sırasında kur düğmesi tıklanabiliyordu → "kontrol ediliyor…" + kilit.
17. **Torrent'te dosya seçimi yoktu** (kullanıcı bildirdi) → `POST /torrent/files`,
    `AddTorrentOptions.only_files`, indirme durmadan `update_only_files`; masaüstü + telefon.
18. **DHT başlatılamazsa torrent motoru tamamen düşüyordu** → DHT'siz devam.
19. **Geçici sunucu hataları ölümcüldü** (429/503'te indirme anında `failed`,
    `Retry-After` yok sayılıyordu) → segment başına 5 deneme, üstel geri çekilme,
    `Retry-After` desteği; `probe` aşamasında da. `Download.retry` arayüzde gösteriliyor.
20. **İndirme bitince/hata olunca bildirim yoktu** → `notify-rust` + AUMID kaydı,
    `notifyOnFinish` ayarı, "Test bildirimi" düğmesi.
21. **GÜVENLİK — web sunucusu kök klasör dışını servis ediyordu.** Canlı kanıt:
    `GET /C:/Windows/win.ini` → 200, `GET /C:/Users/.../src-tauri/src/web.rs` → 200
    (27 KB kaynak kod). Sunucu LAN'a açık. Sebep: `Path::join` Windows'ta mutlak yolu ezip
    kökü atlıyor; koruma yalnız `..`/`\` arıyordu. `resolve_static_path` eklendi
    (bileşen doğrulaması + `canonicalize` sonrası kök altı kontrolü, symlink dahil).
22. **GÜVENLİK — token koruması "izinli prefix listesi" ile yapılıyordu** → `/torrent/files`
    ve `/notify/test` korumasız kaldı. Artık **varsayılan reddet**: yeni her uç otomatik
    korunur, yalnız `/health` + gerçek statik dosyalar serbest.
23. **GÜVENLİK — localhost CSRF açığı**: rastgele bir web sayfası `127.0.0.1:6712/add`
    çağırabiliyordu → `origin_guard` (aynı köken + eklenti şemaları; `null`/`data:`/`file:`
    reddedilir).
