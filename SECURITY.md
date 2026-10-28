# Güvenlik Politikası — Hydra DL

Hydra DL bir **indirme yöneticisidir**: yerel makinede çalışır, kullanıcının dosya
sistemine yazar ve **LAN'a açılan** bir HTTP sunucusu çalıştırır. Bu yüzden tehdit modeli
sıradan bir masaüstü uygulamasından daha geniştir: aynı ağdaki herkes ve ziyaret ettiğin
her web sayfası potansiyel saldırgandır.

## Desteklenen sürümler

Yalnız **en son sürüm** güvenlik düzeltmesi alır (`main` dalı + son NSIS paketi).

## Açık bildirimi

- **Hassas açıklar için herkese açık issue açma.** GitHub'ın
  [özel güvenlik bildirimi](https://github.com/TitanLion72/hydra-dl/security/advisories/new)
  akışını kullan (Security → Report a vulnerability).
- 72 saat içinde ilk yanıt hedeflenir.
- Hangi sürüm, hangi işletim sistemi, tekrar üretme adımları ve mümkünse en küçük
  yeniden üretim (komut/URL) ekle. Kullanıcı verisi/`.env`/token gönderme.

## Tehdit modeli (kısa)

| Varlık | Korunacak şey |
|---|---|
| Yerel dosya sistemi | Web sunucusu üzerinden kök klasör dışına **hiçbir** okuma olmamalı |
| İndirme hedefleri | Yol/`.part` yazımı kök klasör dışına çıkmamalı, `..` kabul edilmemeli |
| Ağ (LAN) | Web API'si varsayılan olarak token ister; token yoksa arayüz uyarır |
| Tarayıcı | Rastgele bir web sayfası `localhost` API'sini çağıramaz (Origin kontrolü) |
| Torrent | Kullanıcı onayı olmadan dışa açık dosya paylaşımı yok, DHT ayarları belgelenmiş |
| Güncelleme | Kurulum paketleri yalnız depodaki imzalı/derlenmiş çıktılardan |

**Kapsam dışı:** DRM kırma, ödeme duvarı/reklam atlatma. Bunlar ürünün "yapılmayacaklar"
listesindedir ve güvenlik açığı olarak kabul edilmez.

## Değişmez güvenlik kuralları (kod)

Bu kurallar `AGENTS.md`'de bağlayıcıdır; ihlal eden PR birleştirilmez.

1. **Varsayılan reddet.** Yeni bir HTTP yolu eklendiğinde **otomatik olarak token
   korumasına girer**. Serbest bırakma yalnız `needs_token()` içinde açıkça yapılır
   (`/health` + gerçek statik dosyalar). "İzinli prefix listesi" yaklaşımı yasak — bir kez
   `/torrent/files` ve `/notify/test` bu yüzden korumasız kaldı.
2. **Kullanıcı girdisiyle dosya yolu kurmak yasak.** Her yerde `resolve_static_path`
   benzeri bir doğrulama kullanılmalı: `..`, `\`, sürücü öneki (`C:`), UNC (`\\host`),
   kök bileşeni reddedilir; `canonicalize` sonrası sonuç kökün altında olmalı.
   `Path::join` tek başına **yetmez** (Windows'ta mutlak yolu ezer).
3. **Origin kontrolü.** Tarayıcıdan gelen isteklerde `Origin` beyaz listeye göre
   denetlenir: aynı köken + tarayıcı eklentisi şemaları. `null`/`data:`/`file:` reddedilir.
   **İki sunucu da** (`127.0.0.1:6712` yakalama + `0.0.0.0:6790` web) `with_origin_guard`
   kullanır — yakalama sunucusu token'sız olduğu için asıl CSRF riski oradaydı.
4. **Token karşılaştırması sabit zamanlı olmalı** (ileride: `subtle::ConstantTimeEq`).
   Token yalnız `[A-Za-z0-9]`, en fazla 64 karakter; URL'de taşınabilir (QR) ama
   loglanmaz.
5. **Sırlar repoya girmez.** `.env`, token, kişisel yol, ekran görüntüsü, `downloads.json`
   commit edilmez (`.gitignore` + CI kontrolü).
6. **Dış komut çağrıları argüman listesiyle** (`Command::arg`), kabuk yorumlamasıyla
   değil. Kullanıcı URL'i/başlığı asla kabuğa verilmez. Dosya adları
   `sanitize_filename` + `unique_path` ile normalize edilir.
7. **HTML'e kullanıcı verisi yazarken kaçış zorunlu.** `innerHTML` yalnız sabit şablonla;
   dış veri `textContent` veya `escapeHtml`/`escapeAttr` ile.
8. **Ayrıştırıcılar (bencode/HTML/JSON) sınırlı.** Boyut, özyineleme ve derinlik sınırı;
   panik yok, hata döner. Ağdan gelen her şey güvenilmezdir.
9. **Tauri komut yüzeyi dar.** Yeni komut eklerken argümanları doğrula; `deleteFile` gibi
   yıkıcı bayraklar varsayılan `false`.
10. **Bağımlılık hijyeni.** Yeni crate/npm paketi eklendiğinde gerekçe PR'da yazılır;
    `cargo audit` ve `npm audit` kırmızıysa düzeltme aynı PR'da gelir.

## Otomatik kontroller (CI)

| Kontrol | Ne yapar |
|---|---|
| `cargo test` | Yol çözümü, token kapsamı, Origin, ayrıştırıcı, segment planı testleri |
| `cargo clippy -- -D warnings` | Panik/unwrap riskleri, şüpheli dönüşümler |
| `cargo fmt --check` | Biçim (gözden kaçan karmaşayı azaltır) |
| `npm run build` (tsc) | Tip güvenliği |
| **CodeQL** (`.github/workflows/codeql.yml`) | `security-and-quality` sorguları, Rust + JS/TS |
| `cargo audit` | Bilinen CVE/RUSTSEC uyarıları |
| `npm audit --audit-level=high` | npm güvenlik açıkları |
| `node tools/security-check.mjs` | Repoda sır/kişisel yol taraması + yol koruma regresyonu |

Kırmızı kontrol **birleştirmeyi engeller**. "Sonra düzeltirim" yok.

## Bilinen sınırlar (kabul edilmiş risk)

- Web sunucusu LAN'a açılabilir; **token boşsa ağdaki herkes erişir**. Bu bilinçli bir
  kolaylık tercihidir, arayüz uyarı gösterir ve varsayılan olarak web sunucusu **kapalıdır**.
- Token URL sorgusunda taşınabilir (QR ile eşleştirme); aynı ağdaki biri HTTP trafiğini
  izleyebiliyorsa token'ı görebilir. HTTPS yok (yerel ağ); ileride TLS değerlendirilecek.
- Torrent motoru DHT'ye katılır (P2P doğası gereği). Kullanıcı ayarlardan kapatabilir.
