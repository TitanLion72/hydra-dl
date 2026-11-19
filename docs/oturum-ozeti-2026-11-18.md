# Oturum özeti — 15–19 Kasım 2026

Önceki özetler: `docs/oturum-ozeti-2026-11-11.md` (v0.5.0), `docs/oturum-ozeti-2026-11-15.md`
(v0.6.0–0.6.1). Bu dosya dayanıklılık dilimlerinin 2–5'ini kapsıyor.

## Tek cümle

**v0.6.7:** dayanıklılık testleri bitti — 5 dilim, **4 gerçek hata** bulundu ve kapatıldı.
`cargo test` **172 geçti**, hepsi canlı doğrulandı.

## Bu oturumdaki sürümler

| Sürüm | Ne | Kanıt |
|---|---|---|
| 0.6.0 | `verify_parts` saf fonksiyona çıkarıldı; kopan bağlantı testi; `.hydra-tmp` sızıntısı | cargo test 164, e2e 16 MB sha256 |
| 0.6.1 | Disk dolu testi; `tools/soak.mjs`; test artığı temizliği | 20 dosya / 122 MB / 1.1 sn |
| 0.6.2 | **HLS/DASH parça yeniden denemesi** (öncesinde hiç yoktu) | 503×2 → parça indi |
| 0.6.3 | Yeniden başlatma sonrası çoklu yarım indirme tutarlılığı | cargo test 169 |
| 0.6.4 | Yanıt + gövde zaman aşımı (indirme yolu) | 136 sn asılma → hata |
| 0.6.5 | Askıda kalma bütçesi 5 → 3 deneme | — |
| 0.6.6 | **`probe` zaman aşımı** (hiç yoktu) | 185 sn → sınırlı |
| 0.6.7 | Askıda kalmada **tek deneme** + net mesaj | canlı: 62 sn'de failed |

## Bulunan gerçek hatalar (hepsi düzeltildi)

1. **`.hydra-tmp` sızıntısı** — birleştirme hatasında geçici dosya diskte kalıyordu.
2. **HLS/DASH'te yeniden deneme yoktu** — tek anlık ağ hatası yüzlerce parçalık akışı
   çöpe atıyordu. Artık 4 deneme, üstel geri çekilme; kalıcı hatalar (404/403) beklemez.
3. **Askıda kalan sunucu** — bağlanıp gövde göndermeyen sunucuda indirme **sonsuza kadar**
   asılı kalıyordu (ölçüldü: 136 sn, hata yok). Artık 30 sn yanıt + 30 sn gövde sessizliği
   sınırı; askıda kalmada tekrar denenmez (anlık blip değil, boşuna bekletir).
4. **`probe` hiç zaman aşımı kullanmıyordu** — canlı ölçüm 185 sn'ye çıktı; asıl sebep
   buydu. Artık HEAD/range istekleri de sınırlı.
5. **e2e.mjs kapsam hatası** — `beforeIds` fonksiyon dışından görünmüyordu (sessiz hata).
6. **Test artıkları** kullanıcının indirme geçmişini kirletiyordu; e2e ve soak artık kendi
   kayıtlarını siler, kullanıcınınkine dokunmaz.
7. **Kişisel yol sızıntısı** özet dosyasında — `security-check` yakaladı, kaldırıldı.

## Dayanıklılık kapsamı — tamamlandı

| Senaryo | Durum |
|---|---|
| Kopan bağlantı (gövde yarıda) | ✅ kaldığı yerden devam, dosya birebir |
| Eksik parçada hedef dosya oluşmaması | ✅ atomik bitiş |
| Birleştirme hatasında geçici dosya temizliği | ✅ |
| Disk dolu / yazma hatası | ✅ yeniden deneme yok, panik yok |
| Eşzamanlı 10–20 indirme | ✅ `soak.mjs` |
| HLS/DASH parça kopması | ✅ retry + sınıflandırma |
| Yeniden başlatma (çoklu yarım indirme) | ✅ ilerleme korunur, idempotent |
| Askıda kalan sunucu | ✅ yanıt/gövde/probe sınırlı, tekrar denenmez |

## Canlı doğrulama (kurulu v0.6.7)

`tools/hang-check.mjs` gerçek askıda sunucuyu senaryolaştırır (başlık gönderir,
gövde göndermez):

```
[hang] 0.0 sn: downloading · indirilen 0
[hang] 62.0 sn: failed · sunucu 30 sn boyunca veri gondermedi (1 deneme sonrası vazgeçildi)
[hang] hedef dizinde dosya: yok (dogru)
```

Öncesi: 136 sn asılı / 185 sn (probe sınırsızdı). Kullanıcının 8 gerçek indirmesi korundu.

## Bilinen sınırlar

- Zaman aşımı **sabit 30 sn**; ayarlardan değiştirilemiyor (zaman aşımını ayara bağlamak
  sıradaki küçük işlerden).
- Torrent akışı bu dilimlerde değişmedi (librqbit kendi retry'ini yapar).
- Çeviri: Rust tarafındaki hata metinleri hâlâ Türkçe.

## Sıradaki iş

1. **v1.0 — otomatik güncelleme:** imzalı manifest, anahtar kullanıcıda.
2. Kod imzalı NSIS (SmartScreen uyarısını azaltır).
3. Zaman aşımını ayara bağlamak (istenirse).

## Nasıl devam edilir

```powershell
cd <proje dizini>
git status --short
cargo test --manifest-path src-tauri\Cargo.toml
npm.cmd run build
node tools/security-check.mjs
```

Yeni oturumda önce `HANDOFF.md`, sonra bu dosya.
