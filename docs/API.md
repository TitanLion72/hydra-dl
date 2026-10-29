# Hydra DL — yerel API sözleşmesi (v1)

Aynı API iki sunucuda da çalışır:

| Sunucu | Varsayılan adres | Kullanıcı |
|---|---|---|
| Yakalama sunucusu | `http://127.0.0.1:6712` | Tarayıcı eklentisi |
| Web sunucusu | `http://<lan-ip>:6790` | Telefon / tablet tarayıcısı |

Tüm yanıtlar JSON'dur (SSE hariç). CORS `*`. Hatalar `{ "ok": false, "error": "..." }` +
uygun HTTP kodu ile döner. Alanlar camelCase'dir.

## Durum

### `GET /health`

```json
{ "app": "hydra-dl", "version": "0.2.0", "uptimeSecs": 412, "downloads": 3, "active": 1 }
```

### `GET /downloads`

`Download[]` — arayüzün kullandığı modelin aynısı (`src/api.ts`).

### `GET /events`

`text/event-stream`; her olayda `data:` satırı tam `Download[]` yayınlar. İlk bağlantıda
anlık liste gönderilir, sonra değişiklikte. 15 sn'de bir `: ping` yorumu.

## Keşif

### `POST /probe` · `GET /probe?url=...`

Gövde: `{ "url": "...", "media": true|false|null }`

```json
{
  "ok": true,
  "url": "https://site/sayfa",
  "finalUrl": "https://cdn.site/video.mp4",
  "kind": "media",
  "title": "Başlık",
  "filename": "video.mp4",
  "size": 15234567,
  "mime": "video/mp4",
  "resumable": true,
  "needsYtdlp": true,
  "formats": [
    { "id": "137", "label": "1080p · mp4", "ext": "mp4", "height": 1080,
      "filesize": 15234567, "vcodec": "avc1", "acodec": "none", "note": "video" }
  ],
  "candidates": [
    { "url": "https://cdn.site/video.mp4", "kind": "file", "label": "Doğrudan dosya", "size": 15234567 }
  ],
  "warnings": []
}
```

- `kind`: `file` | `media` | `torrent` | `hls` | `page` | `unknown`
- `formats` boş olabilir (düz dosya). `needsYtdlp` true ise kalite listesi yt-dlp'den gelir.
- `candidates`: sayfadan çıkarılan gerçek indirme kaynakları (reklam/ara sayfa atlanmış).

## Ekleme

### `POST /add` · `GET /add?url=&filename=&dir=&referer=&format=&media=`

```json
{ "ok": true, "id": "uuid", "filename": "video.mp4", "totalBytes": 15234567, "media": true }
```

`format` verilirse yt-dlp `-f <format>` ile indirilir. `media` verilmezse otomatik algılanır.

### Çerez / oturum (giriş isteyen siteler)

`/probe` ve `/add` gövdelerine `cookie` alanı eklenebilir:

```json
{ "url": "https://site/dosya.zip", "referer": "https://site/panel", "cookie": "sid=abc; tema=koyu" }
```

- Çerez **hem probe** (HEAD/Range) **hem segment indirmelerinde** `Cookie` başlığı olarak
  gönderilir; medya indirmelerinde yt-dlp'ye `--add-header "Cookie: …"` + `--referer` gider.
- Değer tarayıcı eklentisinden gelir (`cookies` izni). Alan yoksa indirme çerezsiz sürer.
- **Güvenlik:** CR/LF/NUL içeren veya 8 KB'tan uzun değer reddedilir (başlık enjeksiyonu).
  Çerez `downloads.json` içinde saklanır (duraklat/devam için gerekli) ve `/downloads`
  yanıtında döner — API token korumalıdır, paylaşmayın.

## Kontrol

| Metot | Yol | Not |
|---|---|---|
| POST | `/downloads/{id}/pause` | |
| POST | `/downloads/{id}/resume` | |
| POST | `/downloads/{id}/cancel` | |
| DELETE | `/downloads/{id}?deleteFile=true` | |

### Geçici hatalar ve yeniden deneme

Segment indirmesi `408`, `425`, `429` ve `5xx` (`501` hariç) durumlarında ya da ağ hatasında
otomatik yeniden denenir:

- En fazla **5 deneme** (ilk deneme dahil), denemeler arası üstel geri çekilme
  (0,5 → 1 → 2 → 4 sn, ±jitter), üst sınır 30 sn.
- Sunucu `Retry-After` gönderdiyse (saniye veya HTTP tarihi) ona uyulur; 30 sn ile kırpılır.
- `4xx` (401/403/404/410/416…) tekrar denenmez; hata `failed` olarak raporlanır.
- Deneme sürerken `Download.retry` dolar:

```json
{ "retry": { "attempt": 3, "max": 5, "waitMs": 4000 } }
```

`attempt` çalışmakta olan denemedir (2 = ikinci deneme), `waitMs` o denemeden önce beklenen
süre. Bitişte/başarıda `retry` alanı düşer. Segment bazlı denemeler tükendiyse tüm dosya için
bir kez daha baştan denenir (parçalar diskte kalır).

## Ayarlar

`GET /settings`, `PUT /settings` — `Settings` modeli (`src/api.ts`). `PUT` tüm nesneyi ister.

| Alan | Anlamı |
|---|---|
| `notifyOnFinish` | İndirme bitince/hata olunca işletim sistemi bildirimi (varsayılan `true`) |
| `proxyUrl` | HTTP/SOCKS5 proxy: `http://host:port`, `https://…`, `socks5://…`, `socks5h://…`. Boş = proxy yok. Geçersiz değer kaydedilirken temizlenir |

Windows'ta paketsiz uygulama bildirimi için AUMID kaydı gerekir; Hydra bunu ilk
bildirimde HKCU\\Software\\Classes\\AppUserModelId\\com.hydradl.app altına yazar.
Bildirim yalnız masaüstü uygulamasında görünür; telefon arayüzünde anahtar yalnızca ayarı
değiştirir. Bildirim gösterilemezse indirme akışı bozulmaz.

**Proxy:** `proxyUrl` doluysa tüm HTTP indirmeleri, sayfa taraması (`/probe`), `.torrent`
çözümleme ve **medya indirmeleri (yt-dlp `--proxy`)** aynı proxy'yi kullanır. Ayar
kaydedildiğinde HTTP istemcisi yeniden kurulur; çalışan indirmeler eski bağlantılarıyla
devam eder, yeni indirmeler proxy'yi kullanır. Torrent (P2P) trafiği proxy'den geçmez
— librqbit'in proxy desteği yok.

`POST /notify/test` — ayar kapalı olsa da bir deneme bildirimi gönderir; altyapı çalışmıyorsa
`502` + hata metni döner (masaüstü ayarlarında "Test bildirimi" düğmesi bunu kullanır).

## Torrent

| Metot | Yol | Gövde |
|---|---|---|
| POST | `/torrents` | `{ "source": "magnet:..." \| "base64 .torrent", "dir": "...", "name": "..." }` |
| GET | `/torrents` | `{ "ok": true, "torrents": [{ "id", "filename", "status", "downloadedBytes", "totalBytes", "speedBps", "peers", "error", "filePath" }] }` |
| POST | `/torrent/files` | `{ "source": "magnet:..." }` → dosya listesi (indirme **başlamaz**) |
| POST | `/torrent/files` | `{ "id": "uuid", "files": [0, 2] }` → eklenmiş torrent'in seçimini değiştirir |

```json
{ "name": "arsiv", "infoHash": "0123...", "totalBytes": 3145728000,
  "files": [{ "index": 0, "path": "bolum-01.mkv", "length": 1572864000, "included": true }] }
```

Dosya seçimi:

- `POST /add` gövdesine `"files": [0, 2]` eklersen yalnız o dosyalar iner; alan yoksa hepsi.
- `POST /torrent/files` listesi `.torrent` kaynağında hemen, magnet'te metadata peer'lerden
  gelene kadar (en fazla 60 sn) sürer; süre aşılırsa `422`.
- Seçim değişince indirme durmaz: motor kalan parçaları yeniden planlar, bitmiş dosyalar
  diskte kalır. `totalBytes` seçili dosyaların toplamıdır; `/downloads` kaydında
  `torrentFiles` alanı seçim durumunu taşır (tek dosyalı torrentte boş olabilir).
- Boş seçim reddedilir (`400`, "En az bir dosya seçmelisin").

Torrent'ler `/downloads` listesinde `kind: "torrent"` olarak da görünür.
`filePath` tek dosyalı torrentte dosyanın kendisi, çok dosyalıda **indirme klasörüdür**.

`/add` ve `/torrents` kaynağı şu biçimlerde kabul eder:

- `magnet:?xt=...`
- 40 karakterlik hex info-hash
- `http(s)` **.torrent dosyası** veya `.torrent` indirme sayfası
- diskteki `.torrent` dosyası yolu

İndirme siteleri `.torrent` adresinde ilk istekte HTML sayfası dönebilir. Sunucu bu durumda
tarayıcı UA'sı + çerez ile tekrar dener, ardından sayfadaki gerçek `.torrent`/`magnet:`
bağlantısını izler. Hepsi başarısız olursa `400` ve "indirme sayfası gibi görünüyor" hatası döner.

## Medya araçları (yt-dlp)

| Metot | Yol | Not |
|---|---|---|
| GET | `/ytdlp` | `{available, path, version}` |
| POST | `/ytdlp/install` | Resmi son sürümü indirir, `<app_data>/bin/yt-dlp.exe` altına kurar, `--version` ile doğrular. Yanıt `{ok, path}`; hata 502 `{ok:false,error}`. Yavaş olabilir (≈7 sn / 20 MB). |

Masaüstü ayrıca `ytdlp:install` olayıyla ilerleme yayınlar:
`{stage: "downloading"|"verifying"|"done"|"failed", downloaded, total, error?}`.

## Eklenti ↔ uygulama

- Yakalama açık/kapalı: `Settings.captureEnabled`
- Her indirmede onay iste: `Settings.askOnDownload` (kapalıysa eklenti doğrudan indirir)
- Avcı modu: `Settings.hunterEnabled`; içerik betiği seçilen öğenin `src`/`currentSrc`/
  `href` adresini `POST /add` ile `referer` olarak sayfa adresiyle gönderir.

## Kimlik doğrulama

Yakalama sunucusu yalnız `127.0.0.1` dinler, token istemez. Web sunucusu LAN'a açıldığında
`Settings.webToken` boş değilse `?token=` ya da `X-Hydra-Token` başlığı gerekir.
