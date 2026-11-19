# Otomatik güncelleme — tasarım

**Tarih:** 2026-11-19 · **Hedef sürüm:** v0.8.0 (bootstrap) → v1.0.0
**Durum:** onaylandı (yaklaşım + kullanıcı kararları)

## Neden

Kullanıcı v1.0'a kadar "elle indir, kur" adımını istemiyor. Şu an her sürümde NSIS
paketini elle indirip kurmak gerekiyor; güvenlik yamaları da ancak kullanıcı repoyu
takip ediyorsa ulaşıyor. Otomatik güncelleme bunu kapatıyor.

## Hedefler

1. Uygulama açılışta **sessizce** yeni sürüm var mı diye bakar (ağ yoksa sessiz geçer).
2. Yeni sürüm varsa arayüzde **şerit** çıkar: "v0.8.1 hazır — Güncelle / Sonra".
   (`Settings.updateCheck` kapalıysa bu kontrol hiç yapılmaz.)
3. Kullanıcı "Güncelle" derse: indir → **imzayı doğrula** → sessiz kur → yeniden başlat.
4. Kullanıcı "Sonra" derse o sürüm için bir daha sormaz (hatırlanır).
5. Sürüm yayınlamak **tek komut** (`node tools/release.mjs`), tekrar edilebilir.
6. İndirme hatası/güncelleme yoksa **hiçbir şey bozulmaz**; mevcut indirmeler etkilenmez.

## Hedef olmayanlar

- Beta/prerelease kanalı (yalnız kararlı sürümler).
- Kademeli yayın (staged rollout), telemetri, "kaç kişi güncelledi" ölçümü.
- Otomatik (onaysız) kurulum — kullanıcı düğmeye basmadan hiçbir şey değişmez.
- Linux/macOS paketleri (v1.0 sonrası).
- Uygulama içinden kanal değiştirme.

## Kötü senaryolar (başarısız sayılır)

- İmza doğrulanmadan bir ikili çalıştırılır (tedarik zinciri açığı — kabul edilemez).
- Sunucu 404/erişilemezken uygulama açılışta hata gösterir veya yavaşlar.
- Güncelleme yarıda kalır ve uygulama açılmaz hale gelir.
- Özel anahtar repoya/CI'ya sızar.
- Kullanıcının süren indirmeleri güncelleme yüzünden bozulur.

## Kısıtlar

- **Platform:** Windows x64, NSIS (`bundle.targets: ["nsis"]`).
- **Tauri 2** + `tauri-plugin-updater` 2.x (sürüm `Cargo.toml`'da sabitlenir, `"2"` gibi
  gevşek bırakılmaz — bu kod yolu uzaktan çalıştırma yapar).
- **Anahtar:** `~/.tauri/hydra-dl-updater.key` (özel, yedeklenecek) +
  `~/.tauri/hydra-dl-updater.key.pub` (repoda `tauri.conf.json` içinde).
- **Release kaynağı:** GitHub Releases (`TitanLion72/hydra-dl`). Şu an 0 release var;
  ilk release bu işin parçası.
- Mevcut değişmezler geçerli: token varsayılan korumalı, kullanıcı dizinlerine dokunma,
  `security-check` yeşil olmadan commit yok.

## Yaklaşımlar

### A) `tauri-plugin-updater` + GitHub Releases — **seçilen**

Resmî eklenti sürümü manifestten okur, imzayı doğrular, paketi indirir, NSIS'i sessiz
kurar ve uygulamayı yeniden açar. İmza doğrulama ve kurulum akışı Tauri ekibi tarafından
denetlenmiş; biz yalnız UX ve yayın adımını yazıyoruz.

- Artı: en riskli kodu (imza doğrulama + kurulum) elle yazmıyoruz; PLAN.md'deki
  "imzalı manifest, anahtar kullanıcıda" hedefiyle birebir uyumlu.
- Eksi: uzaktan kod çalıştırma yolunda yeni bir bağımlılık — sürüm sabitlenir ve
  `cargo audit` kapsamına girer.

### B) Kendi güncelleme motorumuz (mevcut HTTP katmanı + kendi imza kontrolü)

- Artı: yeni bağımlılık yok; retry/timeout/i18n davranışımız aynen geçerli.
- Eksi: imza doğrulama + kurulum + geri dönüş kodunu biz yazıyoruz. Burada yapılan tek
  hata tüm makineyi riske atar. **Reddedildi:** denenmiş yolu bırakıp en kritik kodu elle
  yazmak kötü takas.

### C) Aynı, ama imzalama CI'da (private key GitHub Secrets'ta)

- Eksi: kullanıcı anahtarın kendisinde kalmasını istedi; CI'da tutmak bununla çelişir.
  Ayrıca anahtar sızarsa CI sağlayıcısına bağımlı oluruz. **Reddedildi.**

## Kararlar

| Karar | Seçim | Gerekçe |
|---|---|---|
| Motor | `tauri-plugin-updater` | A yaklaşımı; imza/kurulum denenmiş kod |
| Anahtar yeri | `~/.tauri/hydra-dl-updater.key` | Kullanıcı kararı; repoya girmez, yedeklenir |
| **Anahtar parolası** | **Parolalı**; release komutu build sırasında sorar | Makinedeki her süreç imzalayamasın |
| Public key | `tauri.conf.json → plugins.updater.pubkey` | Tauri standardı |
| Endpoint | GitHub Releases `latest/download/latest.json` | Ücretsiz, imzalı, mevcut altyapı |
| Kontrol zamanı | Açılışta (+ elle "Güncelleme ara") | Sessiz kontrol |
| **Otomatik kontrol ayarı** | **`Settings.updateCheck`, varsayılan AÇIK**, kapatılabilir | Proje kültürü: ağa çıkan şey kapatılabilir olmalı |
| Kullanıcı onayı | **Zorunlu** | Uzaktan kod çalıştıran özellikte onaysız kurulum yok |
| **Kurulum anı** | **Önce temiz kapanış**: indirmeler duraklatılır, durum diske yazılır, torrent oturumu düzgün kapatılır, sonra kurulum | Açılışta kaldığı yerden devam; torrent re-hash'i önlenir |
| "Sonra" davranışı | Sürüm numarası `localStorage`'da | Aynı sürüm için tekrar sormaz |
| **Sessiz başarısız kurulum** | Bir sonraki açılışta kırmızı şerit: "güncelleme tamamlanmadı — yeniden dene" | Sessiz no-op kullanıcıyı şaşırtır |
| Kanal | Yalnız kararlı | v1.0 için yeterli |
| Başlangıç | İlk updater'lı sürüm elle kurulur | Bootstrap: eski sürüm yeni endpoint'i bilmiyor |
| **Yayın sayısı** | **0.8.0 (bootstrap) + 0.8.1 (gerçek E2E kanıtı)**; iki tag de kalıcı | 0.6.7 endpoint'i bilmiyor, tek release ile E2E kanıtlanamaz |

### Ortam değişkenleri (Tauri 2 adları)

```
TAURI_SIGNING_PRIVATE_KEY          = anahtar dosyasının içeriği (base64 metin)
TAURI_SIGNING_PRIVATE_KEY_PASSWORD = parola (release script sorar)
```

`TAURI_SIGNING_PRIVATE_KEY_PATH` **v1 adıdır**, kullanılmıyor.

## İlk kurulum (bir kez, elle)

```
tauri signer generate -w ~/.tauri/hydra-dl-updater.key      # parola sorar
# public key'i tauri.conf.json → plugins.updater.pubkey içine kopyala
# ~/.tauri/hydra-dl-updater.key dosyasını şifreli bir yere YEDEKLE
```

Anahtar kaybolursa eski istemciler yeni imzayı doğrulayamaz; o sürüm elle dağıtılır.

## Nasıl çalışacak (kullanıcı gözüyle)

```
Açılış
  └─ updateCheck açık mı? (hayır → hiçbir şey yapma)
       └─ arka planda sessiz kontrol (3 sn gecikme, ağ hatası = sessiz geç)
            └─ yeni sürüm var mı?
                 ├─ yok → hiçbir şey görünmez
                 └─ var → üstte ince şerit:
                      "Hydra DL v0.8.1 hazır · [Güncelle] [Sonra]"
                           ├─ Sonra → şerit kapanır, bu sürüm unutulur
                           └─ Güncelle → önce temiz kapanış (duraklat + kaydet + torrent kapat)
                                └─ şerit "İndiriliyor… %42" olur
                                     └─ indi → "Kuruluyor, uygulama yeniden açılacak"
                                          └─ NSIS sessiz kurulum → yeniden açılır → devam eder
```

Hata durumunda şerit kırmızıya döner ve **ne olduğunu söyler** ("indirilemedi — internet
bağlantını kontrol et"), ayrıca "Yeniden dene" sunar. Hata hiçbir zaman modal değil;
kullanıcı indirmelerine devam edebilir.

## Yayın akışı (geliştirici gözüyle)

```
node tools/release.mjs 0.8.1
  ├─ sürümü 3 yerde artır (package.json, tauri.conf.json, Cargo.toml)
  ├─ doğrula: cargo test, npm run build, security-check   (kırmızıysa dur)
  ├─ anahtar parolasını sor, TAURI_SIGNING_PRIVATE_KEY(_PASSWORD) ile tauri build
  │     → "Hydra DL_0.8.1_x64-setup.exe" + ".exe.sig"
  ├─ latest.json yaz (signature = .sig içeriği, url = %20'li release adresi)
  ├─ git commit + tag v0.8.1 + push
  └─ gh release create v0.8.1 --notes … (.exe + .sig + latest.json)
```

`latest.json` biçimi (Tauri standardı):

```json
{
  "version": "0.8.1",
  "notes": "…",
  "pub_date": "2026-11-19T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<.sig dosyasının içeriği>",
      "url": "https://github.com/TitanLion72/hydra-dl/releases/download/v0.8.1/Hydra%20DL_0.8.1_x64-setup.exe"
    }
  }
}
```

> Dosya adı boşluk içeriyor (`Hydra DL_0.8.1_x64-setup.exe`); URL'de `%20` olmalı.
> Release'e **kurulum paketi + `.sig` + `latest.json`** yüklenir.

## Dokunulacak yerler

| Yol | Değişiklik |
|---|---|
| `src-tauri/Cargo.toml` | `tauri-plugin-updater = "2.x.y"` (tam sürüm, sabit) |
| `src-tauri/src/lib.rs` | `.plugin(tauri_plugin_updater::Builder::new().build())`; **kurulum öncesi temiz kapanış kancası** |
| `src-tauri/tauri.conf.json` | `bundle.createUpdaterArtifacts: true`, `plugins.updater` (pubkey + endpoint) |
| `src-tauri/capabilities/default.json` | `updater:default` izni |
| `src-tauri/src/store.rs` | `Settings.update_check: bool` (`#[serde(default)] = true`), normalize, export/import |
| `src/api.ts` | `Settings.updateCheck` alanı |
| `package.json` | `@tauri-apps/plugin-updater` |
| `src/updater.ts` | **yeni**: kontrol/indir/kur sarmalayıcı, "sonra" hafızası, bekleyen kurulum işareti |
| `src/main.ts` + `index.html` + `src/styles.css` | güncelleme şeridi + ayar anahtarı ("Güncellemeleri otomatik kontrol et") |
| `webapp/index.html` + `webapp/app.js` | aynı ayar anahtarı (telefon `PUT /settings` ile değiştirir) |
| `webapp/` güncelleme şeridi | **yok** — telefon yalnız `/health`'teki sürümü gösterir (API değişmez) |
| `webapp/i18n-en.js` | yeni metinlerin İngilizcesi |
| `tools/release.mjs` | **yeni**: tek komut yayın (parola sorar) |
| `docs/API.md` | `Settings.updateCheck` satırı |
| `SECURITY.md` | tedarik zinciri bölümü: imza, anahtar, rollback |
| `.github/workflows/ci.yml` | değişmez (`cargo audit` yeni bağımlılığı kapsar) |

### Temiz kapanış (kurulum öncesi)

Kullanıcı "Güncelle" derse, kurulumdan **önce** sırayla:

1. Çalışan indirmeler `paused` durumuna alınır (içerik kaybı yok, parçalar korunur).
2. `store.save()` ile durum diske yazılır (`writing` yarıda kalmaz: parçalar diskte).
3. Torrent oturumu düzgün kapatılır (metainfo + dosyalar kalır).
4. Kurulum başlar; uygulama kapanır.
5. Yeni sürüm açılışta `resume_pending`/`resume_torrents` ile kaldığı yerden devam eder.

Bu sıra `lib.rs`'deki mevcut kapanış kancasıyla (bugün yalnız `WindowEvent::Destroyed`) aynı
fonksiyonu paylaşır; güncelleme yolu o kancayı **atlamaz**.

## Güvenlik

- **İmza doğrulanmadan hiçbir ikili çalıştırılmaz.** Doğrulama eklentinin içinde;
  biz `Updater::download_and_install` çağırırız, o `pubkey` ile kontrol eder.
- Özel anahtar **repoda yok**, `%TEMP%`'e kopyalanmaz; yalnız build ortam değişkeniyle
  okunur ve `security-check` bu dosya adını izler.
- Endpoint HTTPS; GitHub Releases imzalı manifest sunar. Manifest imzası ayrıca
  `.sig` ile kontrol edilir (Tauri biçimi).
- Kullanıcı onayı zorunlu → sessiz kurulum yok.
- `cargo audit` + CodeQL mevcut kapılarda; yeni bağımlılık sürümü sabitlendiği için
  habersiz yükselme olmaz.

## Geri dönüş (rollback)

- Bozuk bir sürüm yayınlanırsa: GitHub'da release'i **draft**'a çek (updater 404 görür,
  kimse indirmez) veya düzeltilmiş sürümü **daha yüksek** numarayla yayınla
  (updater yalnız yukarı gider; aşağı sürüm "güncelleme" sayılmaz).
- Kullanıcı tarafında: önceki NSIS elle kurulabilir; ayarlar `downloads.json`'da olduğu
  için korunur.
- Anahtar kaybolursa: yeni anahtar üretilir, `pubkey` güncellenir ve **o sürüm elle
  dağıtılır** (eski istemciler yeni imzayı doğrulayamaz). Bu yüzden anahtar yedeklenir.

## Doğrulama

1. **Birim:** `src/updater.ts` saf kısımları ("sonra" hafızası, bekleyen kurulum işareti,
   sürüm karşılaştırma) — `tools/test-updater.mjs`.
2. **Entegrasyon (mock):** `tools/ui-harness` içinde sahte updater (`__hydraUpdater`)
   → şerit görünür, "Sonra" hatırlar, "Güncelle" ilerleme gösterir, ayar kapalıyken
   kontrol çağrısı **hiç gitmez**.
3. **Ayar kalıcılığı:** `PUT /settings {updateCheck:false}` → `GET` doğrular; masaüstü ve
   telefon anahtarı aynı değeri gösterir (`web-ui-check`).
4. **Gerçek uçtan uca (iki release):**
   - 0.8.0 yayınlanır, **elle** kurulur (bootstrap).
   - 0.8.1 yayınlanır → uygulama şeridi gösterir → "Güncelle" → `/health` `0.8.1` olur,
     indirmeler korunur, süren indirme duraklatılıp devam eder.
   - İki tag de repoda kalıcıdır (release notları bunu açıkça söyler).
5. **Hata yolları:** release yokken (bugünkü durum) açılış sessiz geçer; endpoint 404 →
   hata gösterilmez; kurulum başarısızsa sonraki açılışta kırmızı şerit.
6. Kapılar: `cargo test`, clippy, fmt, `npm run build`, `test-i18n`, `security-check`,
   `test-updater`.

## Yapıcıya bırakılanlar

- Şeridin tam görsel yerleşimi (mevcut `.pill`/`.toast` diliyle uyumlu olacak).
- `latest.json` üretimini `tauri build` çıktısından mı yoksa elle mi yapacağımız
  (Tauri 2 `createUpdaterArtifacts` `.sig` üretir; manifest'i script yazar).
- Kontrol gecikmesi (öneri: açılıştan 3 sn sonra, ilk indirme listesi çizildikten sonra).
- Zaman aşımı değerleri (mevcut `Timeouts` ruhuyla: kontrol 15 sn, indirme zaman aşımı yok
  ama iptal edilebilir).
