# Mimari

Bu belge Hydra DL'nin katmanlarını, veri akışını ve bozulmaması gereken sözleşmeleri
anlatır. Yerel HTTP uç noktaları için [`API.md`](API.md), katkı kuralları için
[`../CONTRIBUTING.md`](../CONTRIBUTING.md) dosyalarına bakın.

## Genel bakış

Hydra DL tek bir Tauri sürecidir: Rust çekirdeği indirmeleri yürütür, arayüz yalnızca
komutlarla istek gönderir ve olaylarla güncel listeyi alır. Tarayıcı eklentisi ve telefon
arayüzü aynı çekirdeğe yerel HTTP üzerinden bağlanır.

```
src/ (vanilla TS) ──invoke──► commands.rs ──► manager.rs ──► engine/http.rs
                                                        ├─► ytdlp.rs
extension/ (MV3) ──HTTP──► capture.rs ──────────────────┤
webapp/ (telefon) ──HTTP──► web.rs ─────────────────────┘
                                                        └─► torrent.rs

manager.rs ──► model.rs / store.rs / util.rs   (plan, kalıcılık, yardımcılar)
manager.rs ──downloads:changed──► src/, webapp/
```

## Modüller

| Dosya | Sorumluluk | Not |
|---|---|---|
| `src-tauri/src/main.rs` | İnce giriş noktası; `hydra_dl_lib::run()` çağırır | |
| `src-tauri/src/lib.rs` | Tauri Builder, eklentiler, durum kurulumu, komut kaydı | Tek örnek kilidi burada |
| `src-tauri/src/model.rs` | `Download`, `Segment`, `Status`, `Kind`, `Swarm`; **saf** segment planlama ve ilerleme hesabı | Ağ erişimi yok; birim testlerin merkezi |
| `src-tauri/src/store.rs` | `downloads.json` yükleme/kaydetme, `Settings`, kuyruk seçimi, `parts/` yolları | Atomik yazma (`.json.tmp` → rename) |
| `src-tauri/src/util.rs` | Dosya adı temizleme, `Content-Disposition` çözme, çakışma numaralama, MIME tahmini | Saf; Windows rezerve adları |
| `src-tauri/src/engine/mod.rs` | `CancelToken` (watch kanalı), global `RateLimiter` (token bucket) | Tüm indirmeler paylaşır |
| `src-tauri/src/engine/http.rs` | `probe` (HEAD / 1 baytlık Range), Range indirme, `.part` yazımı, parça birleştirme | Birleştirme `.hydra-tmp` → rename |
| `src-tauri/src/manager.rs` | `AppState`, kuyruk/eşzamanlılık, 400 ms ticker, hız ölçümü, pause/resume/cancel/remove, olay yayını, kurtarma | Orkestrasyonun tek sahibi |
| `src-tauri/src/extract.rs` | Akıllı kaynak bulucu: magnet tespiti, sayfa HTML'inden aday çıkarma (`og:video`, JSON-LD, `<video>/<source>`, script), HLS varyant çözümü, `probe` | Saf ayrıştırıcılar testli |
| `src-tauri/src/ytdlp.rs` | yt-dlp bulma/doğrulama, format listesi, süreç yönetimi, ilerleme ayrıştırma, iptal | `__HYDRA__|` ilerleme protokolü |
| `src-tauri/src/torrent.rs` | BitTorrent motoru (librqbit): magnet ve `.torrent`, peer/seeder istatistiği, kapatma | Torrent'ler normal listeye `kind: torrent` olarak düşer |
| `src-tauri/src/capture.rs` | Yerel yakalama API'si (axum): `/health`, `/add`, CORS, port yönetimi | Yalnız `127.0.0.1`, token yok |
| `src-tauri/src/web.rs` | LAN sunucusu (`0.0.0.0:6790`): `/probe`, `/downloads`, `/events` (SSE), `/settings`, `/torrents`; `webapp/` dosyalarını sunar, QR üretir | `Settings.webToken` ile isteğe bağlı koruma |
| `src-tauri/src/commands.rs` | Arayüzün çağırdığı Tauri komutları; argümanlar JS'ten camelCase | İş mantığı içermez |
| `src/` | Vanilla TS arayüz (Vite); keyed satır render'ı, filtreler, ayarlar çekmecesi | Durum yalnız olaydan |
| `extension/` | Chrome/Edge MV3 eklentisi: sağ tık, panel, avcı modu | `http://127.0.0.1/*` |
| `webapp/` | Telefon arayüzü (bağımsız statik dosyalar + PWA manifest + service worker) | Masaüstü sunucusundan servis edilir |
| `tools/` | `e2e.mjs` duman testi, ikon üretici | Geliştirme yardımcıları |

## Veri akışı

### 1. Doğrudan HTTP indirmesi

1. Arayüz `add_download` komutunu çağırır (veya eklenti `GET /add`).
2. `manager::add` URL şemasını doğrular, hedef klasörü belirler, `util::sanitize_filename`
   ve `unique_path` ile çakışmasız hedef üretir.
3. `engine::http::probe` HEAD, gerekirse 1 baytlık `Range` isteğiyle boyut, MIME ve
   `Accept-Ranges` bilgisini toplar.
4. `model` segment planını üretir: 1–16 parça, en az 1 MiB, boşluksuz ve çakışmasız.
5. Kayıt `Queued` olarak `downloads.json`'a yazılır, `downloads:changed` yayınlanır.
6. Ticker, eşzamanlılık limitine göre `schedule` eder; her parça `parts/<id>/<index>.part`
   dosyasına sıralı eklenir. Duraklatma `CancelToken`'ı tetikler, yazılan baytlar saklanır.
7. Tüm parçalar tamamlanınca `concat_parts` hedefi `.hydra-tmp` olarak yazar, ardından
   `rename` ile atomik olarak taşır ve `parts/<id>/` silinir.

### 2. Medya (yt-dlp)

1. `extract::probe` sayfayı inceler; bilinen medya sitesi veya aday yoksa yt-dlp'den format
   listesi istenir (`yt-dlp -J`).
2. Kayıt `Kind::Media` olarak eklenir; seçilen format `Download.format` alanında saklanır.
3. `ytdlp::run` süreci başlatır, `__HYDRA__|` önekli satırlardan yüzde/hız okur ve
   `downloads:changed` yayınlar. İptal, süreci sonlandırır.

### 3. Torrent

1. `extract::is_torrent_source` magnet veya `.torrent` kaynağını tanır.
2. `torrent.rs` librqbit oturumuna ekler; ilerleme ve `Swarm { peers, seeders }` alanları
   ticker ile güncellenir.
3. Torrent de aynı `Download` listesinde görünür; arayüz `kind` alanına göre rozet çizer.

### 4. Eklenti ve telefon

- **Eklenti:** sağ tık / panel → `GET http://127.0.0.1:<capture-port>/add` → `capture.rs`
  → `manager::add`. Avcı modu seçilen öğenin adresini sayfa `Referer`'ı ile gönderir.
- **Telefon:** `webapp/` arayüzü LAN sunucusundan yüklenir; `/probe`, `/downloads`,
  `/events` (SSE), `/add` ve kontrol uç noktalarını kullanır. `Settings.webToken` doluysa
  `?token=` veya `X-Hydra-Token` doğrulanır.

## Eşzamanlılık modeli

- Tokio çok iş parçacıklı çalışma zamanı; Tauri komutları async.
- `AppState.store` bir `std::sync::Mutex<Store>`'dur ve yalnız kısa kritik bölümlerde
  tutulur. **Mutex guard'ı `.await` üzerinden taşınmaz.**
- İndirme başına bir `CancelToken` (`tokio::sync::watch`); duraklatma/iptal sinyali tüm
  parça görevlerine yayılır.
- Eşzamanlılık iki seviyededir: aynı anda çalışan indirme sayısı (`max_concurrent`, 1–10)
  ve dosya başına bağlantı sayısı (`connections`, 1–16).
- `RateLimiter` süreç genelinde tektir; tüm bağlantılar aynı bütçeden tüketir.
- Ticker 400 ms'de bir hız/ilerleme hesaplar ve tam listeyi yayınlar.

## Kalıcılık

Konum: `%APPDATA%\com.hydradl.app\` (Tauri `app_data_dir`, identifier
`com.hydradl.app`).

| Yol | İçerik |
|---|---|
| `downloads.json` | `{ version, settings, items }`; `version` şu an `1` |
| `downloads.json.tmp` | Yazma sırasında geçici dosya; rename ile değiştirilir |
| `parts\<id>\<index>.part` | Yarım parçalar; uygulama kapansa da kalır |
| `parts\<id>\` | yt-dlp ara dosyaları da burada tutulur |
| `bin\yt-dlp.exe` | İsteğe bağlı, elle yerleştirilen yt-dlp |

Kurallar:

- `Settings` ve `Download` alanları serde `default` ile geriye uyumludur. Alan
  silinirken/yeniden adlandırılırken `STATE_VERSION` artırılır ve migrasyon düşünülür.
- Açılışta `Downloading` durumundaki kayıtlar `Paused`a çekilir; `auto_resume` açıksa
  devam ettirilir.
- Kayıtlı `done` ile diskteki `.part` boyu uyuşmazsa **diskteki gerçek boy** esas alınır.
- Duraklatma parçaları korur; iptal `parts/<id>/` dizinini siler. Bu fark kullanıcıya
  görünen bir sözleşmedir.

## Olay sözleşmesi

| Olay | Yük | Kaynak |
|---|---|---|
| `downloads:changed` | Tam `Download[]`, camelCase | Her durum değişikliği + ticker |
| `hydra:notice` | `{ message, level }` (`success` / `error` / `info`) | Bildirimler |

Arayüz başka bir kaynaktan ilerleme okumaz; olay her zaman **tam listeyi** taşır. Bu,
istemcilerin (masaüstü, telefon, eklenti) aynı görünümü üretmesini sağlar.

## Hata ve iptal semantiği

- Sunucu `Range`'i yok sayarsa (`RangeIgnored`) tek bağlantıyla baştan indirilir.
- Ağ hatalarında parça yeniden denenir; kalıcı hatada kayıt `Failed` olur ve hata mesajı
  `error` alanına yazılır.
- İptal, `Cancelled` durumuna geçirir ve parçaları siler; yeniden denemek yeni bir plan
  üretir.
- Tek örnek kilidi (`tauri-plugin-single-instance`) ikinci başlatmayı engeller, pencereyi
  öne getirir.

## Güvenlik sınırları

- Yakalama sunucusu yalnız geri döngüde dinler, token istemez, CORS `*`. Yerel bir sayfa
  indirme ekleyebilir; ayarlardan kapatılabilir.
- LAN sunucusu token ile korunabilir; varsayılan olarak internete açılmaz.
- Dosya adları temizlenir ve hedef klasör dışına çıkacak yollar reddedilir.
- Dış araçlar (yt-dlp, librqbit) kabuk üzerinden değil, argüman listesiyle çalıştırılır.

Ayrıntılı tehdit modeli: [`../SECURITY.md`](../SECURITY.md).

## Test stratejisi

| Katman | Yöntem |
|---|---|
| Saf mantık (`model`, `util`, `extract`, `store`) | `cargo test` birim testleri; ağ yok |
| HTTP motoru | Yerel test sunucusuyla Range/yarım parça/birleştirme senaryoları |
| yt-dlp | Örnek JSON ve ilerleme satırlarının ayrıştırılması |
| Uçtan uca | `tools/e2e.mjs`: gerçek Range sunucusu → yakalama API'si → SHA-256 doğrulaması |
| Arayüz | `npm run build` (tsc) + elle duman testi |

Yeni özellikte önce saf mantık test edilir, sonra ağ katmanına bağlanır. CI bu adımları
Windows üzerinde çalıştırır: [`.github/workflows/ci.yml`](../.github/workflows/ci.yml).

## İlgili belgeler

- [`API.md`](API.md) — yerel HTTP sözleşmesi (yakalama + LAN)
- [`FAQ.md`](FAQ.md) — kullanım soruları
- [`../PLAN.md`](../PLAN.md) — yol haritası
