# AGENTS.md — hydra-dl

Kısa, güncel ve bağlayıcı. Uzun anlatım `docs/`, durum `HANDOFF.md`, yol haritası `PLAN.md`.
Üst klasör kuralları için `..\AGENTS.md` geçerli (Türkçe yollar, `npm.cmd`, PS 5.1 tuzakları).

## Komutlar

```powershell
npm.cmd install
npm.cmd run tauri dev                        # geliştirme (Vite + Tauri)
npm.cmd run build                            # yalnız arayüz (tsc + vite)
cargo fmt   --manifest-path src-tauri\Cargo.toml
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets -- -D warnings
cargo test  --manifest-path src-tauri\Cargo.toml
node tools/e2e.mjs --capture-port 6712 --dir "$env:TEMP\hydra-e2e"   # uygulama açıkken
node tools/web-smoke.mjs --port 6790                                 # web API'si (opsiyonel)
```

**Bitiş tanımı:** `cargo test` + `cargo clippy -D warnings` + `npm run build` yeşil.
Ağ/indirme davranışına dokunduysan `tools/e2e.mjs` de koşar. Üçünden biri kırmızıysa iş bitmedi.

## Mimari (tek cümlede her dosya)

| Yol | Sorumluluk |
|---|---|
| `src-tauri/src/model.rs` | `Download`, `Segment`, `Status`, `Kind`; **segment planlama saf fonksiyonu** |
| `src-tauri/src/store.rs` | `downloads.json`, `Settings`, kuyruk seçimi |
| `src-tauri/src/engine/http.rs` | probe, Range indirme, parça birleştirme |
| `src-tauri/src/extract.rs` | **akıllı kaynak bulucu**: HTML/JSON-LD/meta/video tarama, HLS varyantları, yt-dlp format listesi |
| `src-tauri/src/ytdlp.rs` | yt-dlp bulma, `-J` format listesi, süreç + ilerleme |
| `src-tauri/src/torrent.rs` | librqbit motoru: magnet/.torrent, pause/resume, swarm |
| `src-tauri/src/manager.rs` | kuyruk/eşzamanlılık/ticker, HTTP+medya+torrent yönlendirme, olaylar |
| `src-tauri/src/web.rs` | **tek API yönlendiricisi** + statik `webapp/` servisi + token/CORS |
| `src-tauri/src/capture.rs` | aynı yönlendiriciyi `127.0.0.1:6712`'de açar (eklenti) |
| `src-tauri/src/commands.rs` | Tauri komut yüzeyi |
| `src/` | masaüstü arayüzü (vanilla TS); durum yalnız `downloads:changed` |
| `webapp/` | telefonda çalışan statik arayüz (derleme yok, SSE) |
| `extension/` | Chrome/Edge MV3: panel, avcı modu, sağ tık |
| `tools/` | ikon üretici, e2e, web smoke |
| `docs/API.md` | **API sözleşmesi — kırıcı değişiklik önce buraya** |

## Değişmezler (bozulursa indirmeler bozulur)

1. **Segment planı**: parçalar arasında boşluk/çakışma yok, `end` dahil, toplam = `total`.
   `model::tests` korur.
2. **Parça dosyaları**: `parts/<id>/<index>.part`, sıralı ekleme; kayıt/diskop uyuşmazsa
   diskteki gerçek boy esas.
3. **Duraklat ≠ iptal**: duraklatma parçaları saklar, iptal siler.
4. **Atomik bitiş**: `<hedef>.hydra-tmp` → rename; bitişte parça boyutları **diskten**
   doğrulanır, eksikse `failed` ve parçalar korunur.
5. **Olay sözleşmesi**: `downloads:changed` her zaman tam `Download[]` (camelCase).
6. **Durum şeması**: `downloads.json` alanları `#[serde(default)]` ile geriye uyumlu;
   alan sil/yeniden adlandır → `STATE_VERSION` + migrasyon.
7. **API sözleşmesi** (`docs/API.md`): `/probe`, `/add`, `/downloads/{id}/...`,
   `/events`, `/settings`, `/torrents`. Masaüstü, web ve eklenti aynı sözleşmeyi kullanır.
8. **Tauri argümanları** JS'ten camelCase (`deleteFile`, `format`, `media`).
9. **Torrent kuyruğu ayrı**: torrent'ler `take_queued`'ya girmez, kendi motorunda sürer.
10. **Web sunucusu LAN'a açılır**: `0.0.0.0` + token opsiyonu. Token boşsa uyarı metni
    arayüzde kalmalı; varsayılan **kapalı**.
11. **Avcı modu varsayılan kapalı**: içerik betiği yalnız ayar açıkken etkinleşir
    (kullanıcı rahatsız olmasın diye). Bu varsayılanı değiştirme.

## Kurallar (kullanıcı talebi)

- **Commit/push yalnız kayda değer gelişmede.** Her küçük denemede commit yok; çalışan,
  doğrulanmış bir dilim bitince tek anlamlı commit.
- **Proje dizininden çıkma.** Tüm komutlar `hydra-dl/` içinde; `..\` altındaki diğer
  projelere dokunma, onları commit etme.
- **Workspace şişmesin.** `target/`, `node_modules/`, `dist/`, loglar, ekran görüntüleri
  repoya girmez. Geçici çıktı `%TEMP%` altına. `git status` temiz okunmalı.
- **AGENTS/PLAN/HANDOFF her zaman güncel ve kısa.** Bir kural ölürse sil; dosya büyüyorsa
  `docs/`a taşı. Üç dosya da 150 satırı geçmesin.
- **Oturum büyüse de yönetilebilir kal.** Yeni oturum → önce `HANDOFF.md` oku; bitince
  10 satır yaz. Uzun geçmişi yapıştırma.
- **Lisans AGPL-3.0-or-later.** "Ticari kullanılamaz" yazma; ticari kullanım serbest,
  kapalı dağıtım yasak. Katkılar aynı lisansla gelir (DCO, imza zorunlu değil).
- **Yasal sınır**: DRM kırma, ödeme duvarı/reklam atlatma, kaynak gizleme yok. Torrent
  için "yalnız izin verilen içerik" uyarısı arayüzde durur.
- **Arayüz Türkçe, kod İngilizce.** Kullanıcıya görünen her metin Türkçe ve doğru yazım.

## Tuzaklar

- `npm.ps1` engelli → `npm.cmd`.
- PowerShell 5.1 `Set-Content -Encoding UTF8` BOM ekler; Rust dosyalarında BOM bırakma.
  Dosyaları `write`/`edit` araçlarıyla düzenle.
- Vite portu 1420 `strictPort`. Yakalama 6712, web 6790 varsayılan; çakışırsa ayarlardan değiştir.
- `webapp/` statik: derleme adımı yok. Tauri paketine `bundle.resources` ile girer.
- yt-dlp yoksa medya indirmesi çalışmaz; arayüz bunu "bulunamadı" olarak göstermeli, sessiz düşmemeli.
- Torrent testleri gerçek ağ trafiği üretir; CI'da yalnız kaynak tanıma testleri koşar.
- `src-tauri/target`, `node_modules`, `dist`, `*.log` üretim/geçici — git'e girmez.
- Yakalama API'si `127.0.0.1`'e kilitli; web API'si LAN'a açılır. İkisini karıştırma.

## Yeni özellik akışı

1. Saf mantıksa `model`/`util`/`extract` içinde **önce test**.
2. Ağ katmanı `engine`/`torrent`/`ytdlp`.
3. Komut + olay `manager`/`commands`.
4. API sözleşmesi `docs/API.md` + `web.rs`.
5. Arayüzler: `src/` (masaüstü), `webapp/` (mobil), `extension/` (tarayıcı).
6. Doğrula: üç komut + e2e; sonra `HANDOFF.md`'ye yaz.
