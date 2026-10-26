# Katkı Rehberi

Hydra DL'ye katkı sağlamak istediğiniz için teşekkürler. Bu belge, değişikliğinizin
sorunsuz kabul edilmesi için izlenmesi gereken akışı anlatır.

Katkıda bulunarak, katkınızın [AGPL-3.0-or-later](LICENSE) altında lisanslanmasına ve
[Davranış Kuralları](CODE_OF_CONDUCT.md)'na uyacağınızı kabul etmiş olursunuz.

## Başlamadan önce

- Küçük hata düzeltmeleri ve dokümantasyon için doğrudan PR açabilirsiniz.
- Yeni özellik, mimari değişiklik veya davranış değişikliği için **önce issue açın**.
  Böylece aynı işi iki kişi yapmaz ve tasarım erkenden netleşir.
- Güvenlik açıklarını **asla** herkese açık issue olarak bildirmeyin; [`SECURITY.md`](SECURITY.md)
  izleyin.
- Büyük refactor'larda tek PR'ı küçük, gözden geçirilebilir parçalara bölün.

## Geliştirme ortamı

Gereksinimler: Node.js 20+, Rust 1.77+, WebView2 (Windows).

```powershell
git clone https://github.com/TitanLion72/hydra-dl.git
cd hydra-dl
npm.cmd install
npm.cmd run tauri dev
```

> Windows PowerShell'de `npm` komutu execution policy'ye takılırsa `npm.cmd` kullanın.

Faydalı komutlar:

```powershell
npm.cmd run build                              # tsc + vite (arayüz)
npm.cmd run test                               # cargo test kısayolu
cargo test --manifest-path src-tauri\Cargo.toml
cargo fmt --manifest-path src-tauri\Cargo.toml
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets
node tools/e2e.mjs --capture-port 6712 --dir "$env:TEMP\hydra-e2e"
```

`tools/e2e.mjs` çalışırken uygulamanın açık olması gerekir ve gerçek dosya yazar.

## Değişiklik akışı

1. Depoyu fork'layın, `main` üzerinden anlamlı adlı bir dal açın:
   `feat/hiz-limiti`, `fix/parca-birlesme`, `docs/kurulum` gibi.
2. Değişikliği küçük commit'lerle yapın. Commit mesajları **Conventional Commits**
   biçiminde olsun: `feat: ...`, `fix: ...`, `docs: ...`, `refactor: ...`, `test: ...`,
   `chore: ...`.
3. PR açmadan önce yerel kontrolleri çalıştırın (aşağıya bakın).
4. PR şablonunu doldurun: ne değişti, neden, nasıl test edildi, ekran görüntüsü (varsa).

## Kalite kapıları

Bir PR'ın birleşmesi için:

- `cargo test` yeşil olmalı.
- `cargo clippy --all-targets` uyarısız olmalı.
- `cargo fmt --check` temiz olmalı.
- `npm.cmd run build` hatasız tamamlanmalı.
- Ağ davranışı değiştiyse `tools/e2e.mjs` ile gerçek indirme doğrulanmalı.
- Yeni davranış için test eklenmeli; hata düzeltmesinde hatayı yakalayan test yazılmalı.

CI bu adımları Windows üzerinde çalıştırır (`.github/workflows/ci.yml`). Yerelde yeşil
olmayan bir dalın CI'da yeşil olması beklenmez.

## Kod stili

### Rust

- `cargo fmt` çıktısına dokunmayın; biçimlendirme tartışma konusu değildir.
- Clippy uyarıları bastırılmaz. Gerçekten yanlış pozitif varsa gerekçesiyle
  `#[allow(...)]` ekleyin.
- Async fonksiyonlarda `std::sync::Mutex` guard'ını `.await` üzerinden taşımayın.
- Saf mantığı (segment planlama, dosya adı, hız hesabı) `model`/`util` içinde tutun ve
  ağ katmanından önce testleyin.
- Yorum satırları Türkçe, tanımlayıcılar İngilizce olabilir; mevcut dosyanın diline uyun.
- `unwrap()` yalnız testlerde ve gerçekten imkânsız durumlarda; üretim kodunda hata
  döndürün.

### TypeScript / arayüz

- Vanilla TS + Vite. Yeni bağımlılık eklemeden önce issue'da gerekçelendirin.
- Arayüz durumu **yalnız** `downloads:changed` olayından gelir; başka bir kaynaktan
  ilerleme okumayın.
- Kullanıcıya görünen metinler Türkçe, DOM kimlikleri ve değişkenler İngilizce.
- `npm.cmd run build` içindeki `tsc` hatasız geçmeli; `any` yerine tipleri kullanın.
- Erişilebilirlik: klavye ile gezinme, odak görünürlüğü, `aria-*` etiketleri.

## Bozulmaması gereken değişmezler

Bu kurallar indirmelerin doğruluğunu korur; değiştirecekseniz testleri ve ilgili
dokümanı da güncelleyin.

1. **Segment planı** — parçalar arasında boşluk/çakışma yok, `end` dahil, uzunluk toplamı
   `total`a eşit. `model::tests` bunu korur.
2. **Parça dosyaları** — `parts/<id>/<segment index>.part`, sıralı ekleme. Kayıtlı `done`
   ile dosya boyu uyuşmazsa diskteki gerçek boy esas alınır.
3. **Duraklatma ≠ iptal** — duraklatma parçaları saklar, iptal siler.
4. **Atomik bitiş** — hedef dosya doğrudan yazılmaz; `.hydra-tmp` → rename.
5. **Olay sözleşmesi** — `downloads:changed` her zaman tam `Download[]` (camelCase).
6. **Durum şeması** — `downloads.json` alanları geriye uyumlu olmalı; alan silerken
   `STATE_VERSION` ve migrasyon düşünün.
7. **Yakalama API'si** — sözleşme [`docs/API.md`](docs/API.md)'de; kırıcı değişiklikte
   eklentiyi ve web arayüzünü de güncelleyin.

## Dokümantasyon

- Kullanıcı davranışı değiştiyse `README.md` ve `docs/FAQ.md` güncellenmeli.
- Mimari değiştiyse `docs/ARCHITECTURE.md`.
- Yeni HTTP uç noktası veya alan eklediyseniz `docs/API.md`.
- Yeni ayar eklediyseniz arayüz metni, `store.rs` varsayılanı ve README birlikte güncellenir.

## Commit ve PR

- Bir PR tek bir konuya odaklanmalı.
- “Neden”i açıklayın; “ne” zaten diff'te görünüyor.
- Kullanıcıya görünen değişikliklerde önce/sonra ekran görüntüsü ekleyin.
- Gözden geçirenin istediği değişiklikleri aynı dalda yapın; geçmişi zorla
  rewrite etmeyin (force-push yerine ek commit tercih edilir).
- Birleşme squash ile yapılır; PR başlığı commit mesajı olarak kullanılır.

## Sürümleme

Proje [Semantic Versioning](https://semver.org/lang/tr/) kullanır: `MAJOR.MINOR.PATCH`.
Kırıcı durum şeması veya API değişikliği `MAJOR`, yeni özellik `MINOR`, hata düzeltmesi
`PATCH` artırır. Sürüm numarası `package.json`, `src-tauri/Cargo.toml` ve
`src-tauri/tauri.conf.json` dosyalarında **birlikte** güncellenir.

## Sorular

Issue açmadan önce [`docs/FAQ.md`](docs/FAQ.md) ve mevcut issue'lara bakın. Davranış
kuralları ihlali için [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) içindeki iletişim yolunu
kullanın.
