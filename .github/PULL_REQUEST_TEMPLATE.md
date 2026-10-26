## Özet

<!-- Ne değişti, tek paragrafta. -->

## Neden

<!-- Hangi sorunu çözüyor? İlgili issue: "Closes #123" -->

## Değişiklik türü

- [ ] Hata düzeltmesi
- [ ] Yeni özellik
- [ ] Davranış değişikliği (kırıcı olabilir)
- [ ] Refactor (davranış değişmiyor)
- [ ] Dokümantasyon
- [ ] Test / CI
- [ ] Bağımlılık güncellemesi

## Nasıl test edildi?

<!-- Komutlar ve sonuçlar. Ağ davranışı değiştiyse e2e çıktısını ekleyin. -->

```powershell
cargo test --manifest-path src-tauri\Cargo.toml
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets
npm.cmd run build
```

## Kontrol listesi

- [ ] `cargo test`, `cargo clippy` ve `npm run build` yerelde yeşil.
- [ ] Yeni/değişen davranış için test eklendi veya gerekçesi yazıldı.
- [ ] `cargo fmt` uygulandı.
- [ ] Değişmezler korundu (segment planı, `.part` düzeni, atomik bitiş, olay sözleşmesi).
- [ ] Kullanıcıya görünen metinler Türkçe, tanımlayıcılar İngilizce.
- [ ] Gerekli dokümanlar güncellendi (README / ARCHITECTURE / API / FAQ).
- [ ] Kırıcı API veya durum şeması değişikliği varsa `STATE_VERSION` / migrasyon düşünüldü.
- [ ] Secret, token, kişisel yol veya `.env` eklenmedi.

## Ekran görüntüleri / ek notlar

<!-- Önce/sonra görselleri, tasarım kararları, geriye dönük uyumluluk notları. -->

## Yasal kontrol

- [ ] Değişiklik DRM atlatma, reklam/ücretli üyelik atlama veya izinsiz dağıtım içermiyor.
