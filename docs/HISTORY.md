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
24. **HTTPS yazıldı ama hiç açılmıyordu** (canlı test): `webTls` açılıyor, ayar
    kaydediliyor, sunucu düz HTTP servis etmeye devam ediyordu. İki ayrı hata vardı:
    (a) `axum_server::bind_rustls` kullanılıyordu → bind hatası spawn edilen task içinde
    yutuluyordu (eski dinleyici portu bırakmadan yenisi deniyordu, sessiz başarısızlık);
    artık `bind_with_retry` bind'i **dışarıda** yapıyor (30 × 100 ms, `SO_REUSEADDR` yok).
    (b) `put_settings`/`save_settings` yeniden başlatma koşulunda `web_tls` **yoktu** →
    ayar değişince sunucu hiç yeniden başlamıyordu; artık tek saf fonksiyon
    `web::web_restart_needed` + regresyon testi (yeni alan eklemeyi unutmayı yakalar).
25. **HTTPS açılınca uygulama tamamen kapanıyordu** (canlı test, `0xc0000409`): rustls
    0.23 süreç düzeyindeki CryptoProvider'ı seçemiyordu (`reqwest` → `aws-lc-rs`,
    `rcgen` → `ring`, ikisi de açık) → `ServerConfig` kurulurken **panic**; release profili
    `panic = "abort"` olduğu için süreç öldü. Artık `tls::install_crypto_provider()`
    açılışta + `rustls_config` içinde açıkça `aws-lc-rs` kuruyor; regresyon testi
    (`tls_yapilandirmasi_saglayici_kurulunca_olusturulur`) panik'lerse kırmızı olur.
26. **yt-dlp sürüm karşılaştırması sessizce yanlış cevap verebilirdi** (tasarım aşamasında
    yakalandı, canlıya çıkmadı): saf `>` karşılaştırması `2026.10.26.1` (yeniden yayın) ile
    `2026.10.26`'yı ayırt etmiyor ve `2026.10` ile `2026.10.0`'ı farklı sanıyordu. Artık
    `version_parts` + bileşen bileşen `is_newer`: eksik bileşen `0`, ek bileşen yeni sayılır;
    kurulu sürüm okunamıyorsa "güncellenebilir", **son** sürüm okunamıyorsa "güncelleme yok"
    (yanlış yönlendirme yapmıyoruz). 6 birim testi sabitliyor.
27. **`GET /ytdlp/latest` ağ hatasında isteği düşürüyordu** (tasarım düzeltmesi): istemci
    "kontrol edilemedi" ile "sürüm yok"u ayırt edemiyor, ayarlar ekranı hata gösteriyordu.
    Artık ağ hatası **200 + `error` alanı** döner; `current` bilgisi korunur.
28. **GitHub API bütçesi** (token'sız 60 istek/saat): her ayar açılışında istek atmak
    bütçeyi bitirirdi → 30 dk `LATEST_CACHE`. Ölçüm: 571 ms → **4 ms**.
    Güncelleme sonrası iki önbellek de temizlenir (kontrol tazelenir).
29. **Dışa aktarma bayrağı auth token'ıyla çakışacaktı** (tasarımda yakalandı): ayar
    dışa aktarmada `?token=1` kullanmak, kimlik doğrulama anahtarını (`auth_middleware`
    `?token=`) bool sanıp isteği 400'e düşürürdü. Bayrak `includeToken` olarak ayrıldı;
    regresyon testi (`disa_aktarma_bayragi_auth_tokeniyla_cakismaz`).
30. **Token'sız ayar yedeği erişim anahtarını siliyordu** (tasarımda yakalandı): içe
    aktarma "dosyayı aynen yaz" olsaydı, token dışa aktarılmadığı için geri yüklemede
    token boşalırdı → kullanıcı telefondan kilitlenirdi. Artık içe aktarma **mevcut
    ayarların üzerine birleştirir**; test `ice_aktarma_token_yoksa_mevcut_token_korunur`.