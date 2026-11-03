# Tuzaklar — ayrıntı

`AGENTS.md` kısa kalsın diye buraya taşındı. Kural değil, **deneyim**; hepsi bir kez
gerçekten ısırdı. Yeni tuzak eklerken tek satırla buraya, özeti `AGENTS.md`'ye.

## Derleme / disk

- `npm.ps1` execution policy'de engelli → **`npm.cmd`** kullan.
- **Disk dolabilir.** `src-tauri\target` birikimli **50 GB+** olabiliyor. Disk dolduğunda
  belirti alışılmadık: `os error 112` değil, `failed to build archive ... .lib` ya da
  `rustc-LLVM ERROR: IO failure on output stream: no space on device`. Çözüm:
  `node tools/hygiene.mjs --clean` + `cargo clean`, sonra derle.
- Hızlı denetim için `CARGO_TARGET_DIR=%TEMP%\hydra-target` (release paketi için değil;
  `tauri build` kendi `target/`ını bekler).
- PowerShell 5.1 `Set-Content -Encoding UTF8` **BOM** ekler; Rust dosyalarında BOM
  derlemeyi bozar. Dosyaları `write`/`edit` araçlarıyla düzenle.

## Çalışma zamanı

- Vite portu 1420 `strictPort`. Yakalama 6712, web 6790 varsayılan; çakışırsa ayarlardan değiştir.
- **`setup()` ana thread'de çalışır.** Orada `tokio::spawn` çağırma → *"there is no reactor
  running"* paniği; süreç `0xc0000409` ile **sessizce** kapanır. `tauri::async_runtime::spawn` kullan.
- `[profile.release] panic = "abort"`: herhangi bir panic tüm süreci öldürür, geri iz yok.
  Yeni bir bağımlılık panic atıyorsa stderr'i dosyaya yönlendirip yakala
  (`Start-Process -RedirectStandardError`); yoksa "uygulama kapanıyor, sebep yok" olur.
- Yakalama API'si `127.0.0.1`'e kilitli; web API'si LAN'a açılır. İkisini karıştırma.

## Paketleme

- `webapp/` statik, derleme adımı yok. Tauri paketine `resources` ile **hedef adı verilerek**
  girer (`{"../webapp": "webapp"}`). Kaynak `src-tauri` dışındaysa Tauri dosyaları
  `resource/_up_/` altına koyar; kod iki yolu da dener (`web::pick_webapp_dir`).
- **Kurulum paketi ayrı test edilmeli:** `tauri dev` çalışsa bile NSIS paketi bozuk olabilir
  (bir kez oldu: web arayüzü 404). `npm run tauri build` → `setup.exe /S /D=%TEMP%\hydra-test`
  → **farklı cwd'den** çalıştır → `/health` + `/` kontrol et.

## Medya / ağ

- yt-dlp yoksa medya indirmesi çalışmaz; arayüz "bulunamadı" göstermeli, sessiz düşmemeli.
- Torrent testleri gerçek ağ trafiği üretir; CI'da yalnız kaynak tanıma testleri koşar.
- rustls 0.23 süreç düzeyi CryptoProvider'ı features'tan **seçemez** (`reqwest` → `aws-lc-rs`,
  `rcgen` → `ring`). Kurulmazsa `ServerConfig` panic atar. `tls::install_crypto_provider()`.
- HTTPS'te bind **spawn edilen task içinde** yapılmamalı: hata yutulur, sunucu sessizce
  açılmaz. `bind_with_retry` bind'i dışarıda yapar. `SO_REUSEADDR` kullanma (Windows'ta
  port çalmaya izin verir).

## Tarayıcı eklentisi

- Chrome `chrome.*`, Firefox `browser.*` (promise tabanlı). Kod `chrome.` değil **`HX.api.`**
  kullanır; `extension/compat.js` her bağlamda **ilk** yüklenmeli (`importScripts`,
  `content_scripts.js[0]`, HTML'de ilk `<script>`). Callback'li `chrome.*` ekleme.
- **Firefox MV3 host izni** kurulumda otomatik gelmez: ayarlardaki *Yerel erişim izni ver*
  düğmesi `permissions.request` çağırır. Bu düğmeyi kaldırma.
- `build/extensions/*` üretilen çıktıdır; elle düzenleme, `extension/`'ı düzenle.

## Depo / araç

- `src-tauri/target`, `node_modules`, `dist`, `*.log`, ekran görüntüleri git'e girmez.
  Geçici çıktı `%TEMP%` altına.
- `.ps1` betikleri **ASCII** olmalı (PS 5.1 Türkçe karakterleri bozar); UTF-8 kaynak
  dosyaları `write`/`edit` ile yaz.
- `tools/hygiene.mjs` tek sağlık komutu; `--clean` yalnız geçici `hydra-*` klasörlerini siler.
