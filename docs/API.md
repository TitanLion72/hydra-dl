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

## Kontrol

| Metot | Yol | Not |
|---|---|---|
| POST | `/downloads/{id}/pause` | |
| POST | `/downloads/{id}/resume` | |
| POST | `/downloads/{id}/cancel` | |
| DELETE | `/downloads/{id}?deleteFile=true` | |

## Ayarlar

`GET /settings`, `PUT /settings` — `Settings` modeli (`src/api.ts`). `PUT` tüm nesneyi ister.

## Torrent

| Metot | Yol | Gövde |
|---|---|---|
| POST | `/torrents` | `{ "source": "magnet:..." \| "base64 .torrent", "dir": "...", "name": "..." }` |
| GET | `/torrents` | `{ "ok": true, "torrents": [{ "id", "filename", "status", "downloadedBytes", "totalBytes", "speedBps", "peers", "error", "filePath" }] }` |

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
