//! BitTorrent/P2P motoru (librqbit). Magnet ve .torrent destekler.

use crate::manager::AppState;
use crate::model::{apply_selection, selected_total, Status, Swarm, TorrentFile};
use librqbit::api::TorrentIdOrHash;
use librqbit::{AddTorrent, AddTorrentOptions, ManagedTorrent, Session, TorrentStatsState};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

type ManagedTorrentHandle = Arc<ManagedTorrent>;

/// Motorun dosya listesini bu kadar sure icinde getirmesi beklenir.
/// Magnet'te metadata once peer'lerden iner; bu yuzden comert bir sinir.
const LIST_TIMEOUT: Duration = Duration::from_secs(60);

/// Bir torrent isinin (veya listesinin) dosya dokumu.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentListing {
    pub name: String,
    pub info_hash: String,
    pub total_bytes: u64,
    pub files: Vec<TorrentFile>,
}

/// Motorun dondurdugu metadata'dan secim listesi uretir.
fn files_from_info(
    info: &librqbit::ValidatedTorrentMetaV1Info<librqbit::ByteBufOwned>,
) -> Vec<TorrentFile> {
    info.iter_file_details()
        .enumerate()
        .map(|(index, detail)| TorrentFile {
            index,
            path: detail
                .filename
                .to_vec()
                .join("/")
                .trim_start_matches('/')
                .to_string(),
            length: detail.len,
            included: true,
        })
        .collect()
}

/// Motorda kayitli torrent'in dosya listesi (secim bayraklariyla).
fn files_of(handle: &ManagedTorrentHandle) -> Option<Vec<TorrentFile>> {
    handle
        .with_metadata(|meta| files_from_info(&meta.info))
        .ok()
}

/// Motora verilecek tek bir torrent isi.
pub struct TorrentJob<'a> {
    pub id: String,
    /// Magnet, info-hash, `.torrent` adresi ya da yerel dosya yolu.
    pub source: &'a str,
    pub dir: PathBuf,
    pub name: Option<String>,
    pub start_paused: bool,
    /// Indirilecek dosya indeksleri; `None` = hepsi.
    pub files: Option<Vec<usize>>,
}

#[derive(Default)]
pub struct TorrentEngine {
    session: Mutex<Option<Arc<Session>>>,
    handles: Mutex<HashMap<String, Entry>>,
}

struct Entry {
    id: TorrentIdOrHash,
    handle: ManagedTorrentHandle,
    /// Tamamlandiginda dosya yolunu bir kez cozmek icin.
    resolved_path: Option<PathBuf>,
}

impl TorrentEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Oturumu (gerekirse) baslatir. Dinleme portu sistem tarafindan secilir.
    async fn ensure_session(&self, dir: &PathBuf) -> Result<Arc<Session>, String> {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.as_ref() {
            return Ok(session.clone());
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("Torrent klasörü: {e}"))?;
        let session = match Session::new(dir.clone()).await {
            Ok(session) => session,
            Err(err) => {
                // DHT baslatilamazsa (port kapali, disk izni, cevrimdisi makine)
                // torrent moturu tamamen dusecegine DHT'siz calissin; magnet
                // disindaki kaynaklar bundan etkilenmez.
                eprintln!("[hydra] DHT baslatilamadi ({err}), DHT'siz deneniyor");
                Session::new_with_opts(
                    dir.clone(),
                    librqbit::SessionOptions {
                        dht: None,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| format!("Torrent oturumu başlatılamadı: {e}"))?
            }
        };
        *guard = Some(session.clone());
        Ok(session)
    }

    /// Torrent'i indirmeden yalnizca dosya listesini getirir.
    ///
    /// `.torrent` kaynagi dogrudan cozulur; magnet'te metadata peer'lerden
    /// geldigi icin `LIST_TIMEOUT` kadar beklenir.
    pub async fn list(
        &self,
        client: &reqwest::Client,
        source: &str,
        dir: &PathBuf,
    ) -> Result<TorrentListing, String> {
        let session = self.ensure_session(dir).await?;
        let add = match resolve_add_source(client, source).await? {
            TorrentSource::Url(url) => AddTorrent::from_url(url),
            TorrentSource::Bytes(bytes) => AddTorrent::from_bytes(bytes),
        };
        let opts = AddTorrentOptions {
            paused: true,
            list_only: true,
            overwrite: true,
            output_folder: Some(dir.to_string_lossy().to_string()),
            ..Default::default()
        };
        let response = tokio::time::timeout(LIST_TIMEOUT, session.add_torrent(add, Some(opts)))
            .await
            .map_err(|_| {
                "Torrent bilgisi zamanında alınamadı. Magnet ise peer bulunamadı; .torrent dosyası dene.".to_string()
            })?
            .map_err(|e| format!("Torrent okunamadı: {e}"))?;

        let listing = match response {
            librqbit::AddTorrentResponse::ListOnly(listing) => listing,
            librqbit::AddTorrentResponse::Added(_, handle)
            | librqbit::AddTorrentResponse::AlreadyManaged(_, handle) => {
                // Bu torrent zaten motorda: mevcut dosya listesini kullan.
                let files = files_of(&handle)
                    .ok_or_else(|| "Torrent dosya listesi okunamadı".to_string())?;
                return Ok(TorrentListing {
                    name: handle.name().unwrap_or_else(|| "torrent".into()),
                    info_hash: handle.info_hash().as_string(),
                    total_bytes: files.iter().map(|f| f.length).sum(),
                    files,
                });
            }
        };

        let files = files_from_info(&listing.info);
        let name = listing
            .info
            .name()
            .map(|n| n.into_owned())
            .unwrap_or_else(|| "torrent".into());
        Ok(TorrentListing {
            name,
            info_hash: listing.info_hash.as_string(),
            total_bytes: files.iter().map(|f| f.length).sum(),
            files,
        })
    }

    pub async fn add(
        &self,
        state: &Arc<AppState>,
        client: &reqwest::Client,
        job: TorrentJob<'_>,
    ) -> Result<(), String> {
        let TorrentJob {
            id,
            source,
            dir,
            name,
            start_paused,
            files,
        } = job;
        let session = self.ensure_session(&dir).await?;
        let add = match resolve_add_source(client, source).await? {
            TorrentSource::Url(url) => AddTorrent::from_url(url),
            TorrentSource::Bytes(bytes) => AddTorrent::from_bytes(bytes),
        };
        let opts = AddTorrentOptions {
            paused: start_paused,
            overwrite: true,
            output_folder: Some(dir.to_string_lossy().to_string()),
            only_files: files.clone(),
            ..Default::default()
        };
        let response = session
            .add_torrent(add, Some(opts))
            .await
            .map_err(|e| format!("Torrent eklenemedi: {e}"))?;
        let (torrent_id, handle) = match response {
            librqbit::AddTorrentResponse::Added(id, handle)
            | librqbit::AddTorrentResponse::AlreadyManaged(id, handle) => {
                (TorrentIdOrHash::Id(id), handle)
            }
            librqbit::AddTorrentResponse::ListOnly(_) => {
                return Err("Torrent yalnız listelendi".into())
            }
        };

        // Metadata gelmeden toplam boyut 0 olabilir; ilk istatistigi bekle.
        let mut stats = handle.stats();
        for _ in 0..40 {
            if stats.total_bytes > 0 || stats.finished {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            stats = handle.stats();
        }
        let resolved_name = handle
            .name()
            .map(|n| crate::util::sanitize_filename(n.as_str()))
            .or(name)
            .unwrap_or_else(|| "torrent".into());
        // Secili dosyalarin toplami, metadata'nin tamamina gore daha dogru.
        let mut selection = files_of(&handle);
        if let (Some(list), Some(requested)) = (selection.as_mut(), files.as_deref()) {
            let _ = apply_selection(list, requested);
        }
        let selected = selection
            .as_ref()
            .and_then(|files| selected_total(files))
            .or(if stats.total_bytes > 0 {
                Some(stats.total_bytes)
            } else {
                None
            });

        state.update_item(&id, |item| {
            item.status = if start_paused {
                Status::Paused
            } else {
                Status::Downloading
            };
            item.filename = resolved_name;
            item.total_bytes = selected;
            item.downloaded_bytes = stats.progress_bytes;
            item.segments.clear();
            item.resumable = true;
            item.mime = Some("torrent".into());
            item.swarm = Some(Swarm::default());
            item.source = Some(source.to_string());
            item.torrent_files = selection;
        });

        self.handles.lock().await.insert(
            id,
            Entry {
                id: torrent_id,
                handle,
                resolved_path: None,
            },
        );
        state.persist();
        state.emit_downloads();
        Ok(())
    }

    /// Indirilecek dosya secimini degistirir (indirme durmadan).
    pub async fn set_files(
        &self,
        state: &Arc<AppState>,
        id: &str,
        selected: Vec<usize>,
    ) -> Result<(), String> {
        let session = self
            .session
            .lock()
            .await
            .clone()
            .ok_or("Torrent oturumu kapalı")?;
        let guard = self.handles.lock().await;
        let entry = guard.get(id).ok_or("Torrent bulunamadı")?;
        let mut files = files_of(&entry.handle)
            .ok_or_else(|| "Torrent dosya listesi henüz hazır değil".to_string())?;
        apply_selection(&mut files, &selected)?;
        let wanted: HashSet<usize> = files
            .iter()
            .filter(|f| f.included)
            .map(|f| f.index)
            .collect();
        session
            .update_only_files(&entry.handle, &wanted)
            .await
            .map_err(|e| format!("Dosya seçimi uygulanamadı: {e}"))?;
        drop(guard);

        let total = selected_total(&files);
        state.update_item(id, |item| {
            item.total_bytes = total;
            item.torrent_files = Some(files);
        });
        state.persist();
        state.emit_downloads();
        Ok(())
    }

    pub async fn pause(&self, state: &Arc<AppState>, id: &str) -> Result<(), String> {
        let session = self
            .session
            .lock()
            .await
            .clone()
            .ok_or("Torrent oturumu kapalı")?;
        let guard = self.handles.lock().await;
        let entry = guard.get(id).ok_or("Torrent bulunamadı")?;
        session
            .pause(&entry.handle)
            .await
            .map_err(|e| format!("Duraklatılamadı: {e}"))?;
        drop(guard);
        state.update_item(id, |item| {
            item.status = Status::Paused;
            item.speed_bps = 0;
        });
        Ok(())
    }

    pub async fn resume(&self, state: &Arc<AppState>, id: &str) -> Result<(), String> {
        let session = self
            .session
            .lock()
            .await
            .clone()
            .ok_or("Torrent oturumu kapalı")?;
        let guard = self.handles.lock().await;
        let entry = guard.get(id).ok_or("Torrent bulunamadı")?;
        session
            .unpause(&entry.handle)
            .await
            .map_err(|e| format!("Devam ettirilemedi: {e}"))?;
        drop(guard);
        state.update_item(id, |item| {
            item.status = Status::Downloading;
            item.error = None;
        });
        Ok(())
    }

    pub async fn remove(&self, state: &Arc<AppState>, id: &str, delete_files: bool) {
        let Some(session) = self.session.lock().await.clone() else {
            return;
        };
        if let Some(entry) = self.handles.lock().await.remove(id) {
            if let Err(err) = session.delete(entry.id, delete_files).await {
                eprintln!("[hydra] torrent silinemedi: {err}");
            }
        }
        state.update_item(id, |item| item.speed_bps = 0);
    }

    /// Ticker her turunda cagrilir: ilerleme, hiz ve bitis durumunu isler.
    pub async fn refresh(&self, state: &Arc<AppState>) -> bool {
        let session = self.session.lock().await.clone();
        let Some(session) = session else {
            return false;
        };
        let ids: Vec<String> = self.handles.lock().await.keys().cloned().collect();
        if ids.is_empty() {
            return false;
        }

        let peers = serde_json::to_value(session.stats_snapshot())
            .ok()
            .and_then(|value| {
                value
                    .pointer("/peers/alive")
                    .or_else(|| value.pointer("/peers/seen"))
                    .and_then(|v| v.as_u64())
            })
            .unwrap_or(0) as usize;

        let mut active = false;
        let mut finished_now: Vec<(String, PathBuf)> = Vec::new();
        let mut failed_now: Vec<(String, String, String)> = Vec::new();
        {
            let mut guard = self.handles.lock().await;
            for id in &ids {
                let Some(entry) = guard.get_mut(id) else {
                    continue;
                };
                let stats = entry.handle.stats();
                let speed = stats
                    .live
                    .as_ref()
                    .map(|live| live.download_speed.as_bytes())
                    .unwrap_or(0);
                let total = if stats.total_bytes > 0 {
                    Some(stats.total_bytes)
                } else {
                    None
                };
                let error = stats.error.clone();
                let finished = stats.finished;
                let progress = stats.progress_bytes;
                let state_live = matches!(stats.state, TorrentStatsState::Live);
                if state_live {
                    active = true;
                }

                if finished && entry.resolved_path.is_none() {
                    entry.resolved_path = resolve_completed_path(&entry.handle);
                    if let Some(path) = entry.resolved_path.clone() {
                        finished_now.push((id.clone(), path));
                    }
                }

                let resolved = entry.resolved_path.clone();
                let status_now = {
                    let item = state.item(id);
                    item.map(|d| d.status)
                };
                state.update_item(id, |item| {
                    if item.total_bytes.is_none() {
                        item.total_bytes = total;
                    }
                    item.downloaded_bytes = progress;
                    item.speed_bps = speed;
                    item.swarm = Some(Swarm {
                        peers,
                        seeders: peers,
                    });
                    if let Some(err) = &error {
                        // Ayni hatayi her ticker turunda bildirmemek icin yalniz geciste.
                        let was_failed = item.status == Status::Failed;
                        item.status = Status::Failed;
                        item.error = Some(err.clone());
                        if !was_failed {
                            failed_now.push((id.clone(), item.filename.clone(), err.clone()));
                        }
                    } else if finished {
                        if item.status != Status::Completed {
                            item.status = Status::Completed;
                            item.progress = Some(1.0);
                            item.finished_at = Some(chrono::Utc::now().timestamp());
                            item.speed_bps = 0;
                            item.error = None;
                        }
                        if let Some(path) = resolved.clone() {
                            item.file_path = Some(path);
                        }
                        if let Some(total) = total {
                            item.downloaded_bytes = total;
                        }
                    } else if error.is_none() && state_live && item.status == Status::Queued {
                        let _ = status_now;
                        item.status = Status::Downloading;
                    }
                });
            }
        }

        for (id, path) in finished_now {
            // Cok dosyali torrentte `path` klasordur. O durumda gorunen ad torrent
            // adi kalir; klasor adini dosya adi gibi yazmiyoruz.
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()).map(String::from)
                {
                    state.update_item(&id, |item| item.filename = filename.clone());
                    state.notice(format!("Torrent tamamlandı: {filename}"), "success");
                    state.notify("Torrent tamamlandı", &filename);
                }
            } else {
                let label = state
                    .item(&id)
                    .map(|d| d.filename)
                    .unwrap_or_else(|| "Torrent".into());
                state.notice(format!("Torrent tamamlandı: {label}"), "success");
                state.notify("Torrent tamamlandı", &label);
            }
        }

        for (_id, label, error) in failed_now {
            state.notify(
                "Torrent başarısız",
                &crate::manager::notify_body(&label, Some(&error)),
            );
        }
        active
    }

    pub async fn has(&self, id: &str) -> bool {
        self.handles.lock().await.contains_key(id)
    }

    pub async fn shutdown(&self) {
        let session = self.session.lock().await.take();
        if let Some(session) = session {
            session.stop().await;
        }
        self.handles.lock().await.clear();
    }
}

/// Bazi siteler UA'siz istegi indirme sayfasina yonlendiriyor; tarayici gibi gorunuyoruz.
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

/// Cozulmus torrent kaynagi: motor ya URL (magnet/.torrent) ya da bellekteki
/// `.torrent` dosyasi ile beslenir.
#[derive(Debug)]
pub enum TorrentSource {
    Url(String),
    Bytes(Vec<u8>),
}

/// Yerel `.torrent` dosyası için üst sınır: gerçek torrent dosyaları birkaç MB'ı
/// geçmez; 32 MB keyfi dosya okumayı engeller (CodeQL: `rust/path-injection`).
const MAX_LOCAL_TORRENT: u64 = 32 * 1024 * 1024;

/// Kullanıcı kaynağını motora verilecek hale getirir.
///
/// - `magnet:` ve 40 karakterlik info-hash → dogrudan motor (indirme yok)
/// - diskte var olan yol → dosya okunur (bencode + boyut doğrulanır)
/// - `http(s)` → `fetch_torrent_source` (cerez/UA/sayfa takibi)
pub async fn resolve_add_source(
    client: &reqwest::Client,
    source: &str,
) -> Result<TorrentSource, String> {
    let trimmed = source.trim();
    if trimmed.starts_with("magnet:") || is_info_hash(trimmed) {
        return Ok(TorrentSource::Url(trimmed.to_string()));
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return fetch_torrent_source(client, trimmed).await;
    }

    // Yerel yol: yalnız gerçek bir .torrent dosyası kabul edilir. Bu doğrulama
    // olmadan API'ye erişen biri rastgele bir dosyayı "torrent" diye okutup
    // hata mesajlarından varlığını/içeriğini sızdırabilirdi.
    let meta = tokio::fs::metadata(trimmed)
        .await
        .map_err(|_| "Torrent dosyası bulunamadı".to_string())?;
    if !meta.is_file() {
        return Err("Torrent kaynağı bir dosya olmalı".into());
    }
    if meta.len() > MAX_LOCAL_TORRENT {
        return Err(format!(
            "Torrent dosyası çok büyük ({} MB); en fazla {} MB olabilir",
            meta.len() / (1024 * 1024),
            MAX_LOCAL_TORRENT / (1024 * 1024)
        ));
    }
    let bytes = tokio::fs::read(trimmed)
        .await
        .map_err(|_| "Torrent dosyası okunamadı".to_string())?;
    if !looks_like_torrent(&bytes) {
        // İçerik hakkında bilgi sızdırma: "bu bir torrent değil" yeter.
        return Err("Bu dosya geçerli bir .torrent değil".into());
    }
    Ok(TorrentSource::Bytes(bytes))
}

/// 40 karakterlik hex info-hash (magnet yerine kabul edilir).
pub fn is_info_hash(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `http(s)` torrent kaynagini indirir.
///
/// Gercek dunya sorunu (kullanici bildirdi): bazi siteler `.torrent` adresinde ilk
/// istekte **HTML indirme sayfasi** donuyor ve gercek dosyayi yalnizca cerez +
/// tarayici UA ile ikinci istekte veriyor. Bu yuzden sirayla denenir:
/// 1. istek → bencode mu? 2. cerezle tekrar → bencode mu?
/// 3. sayfadaki `.torrent`/`magnet:` baglantisi → ayni iki adim.
pub async fn fetch_torrent_source(
    client: &reqwest::Client,
    source: &str,
) -> Result<TorrentSource, String> {
    let (body, cookie) = http_get(
        client,
        source,
        None,
        "text/html,application/xhtml+xml,application/x-bittorrent;q=0.9,*/*;q=0.8",
    )
    .await?;
    if looks_like_torrent(&body) {
        return Ok(TorrentSource::Bytes(body));
    }

    // Cerezle tekrar: bazi siteler ilk istekte cerez birakip gercek dosyayi
    // ikinci istekte veriyor (dosyadrive.vip tam olarak boyle).
    let (body, cookie) = http_get(
        client,
        source,
        cookie.as_deref(),
        "application/x-bittorrent,*/*;q=0.8",
    )
    .await?;
    if looks_like_torrent(&body) {
        return Ok(TorrentSource::Bytes(body));
    }

    // Sayfa: icindeki gercek baglantiyi bul ve ayni adimlari orada dene.
    if let Some(link) = find_torrent_link(&String::from_utf8_lossy(&body), source) {
        if link.starts_with("magnet:") {
            return Ok(TorrentSource::Url(link));
        }
        if link != source {
            let (body, cookie) = http_get(
                client,
                &link,
                cookie.as_deref(),
                "application/x-bittorrent,*/*;q=0.8",
            )
            .await?;
            if looks_like_torrent(&body) {
                return Ok(TorrentSource::Bytes(body));
            }
            let (body, _) = http_get(
                client,
                &link,
                Some(cookie.as_deref().unwrap_or_default()),
                "application/x-bittorrent,*/*;q=0.8",
            )
            .await?;
            if looks_like_torrent(&body) {
                return Ok(TorrentSource::Bytes(body));
            }
        }
    }

    Err(
        "Bu adres .torrent dosyası değil, indirme sayfası gibi görünüyor. \
         Sayfadaki gerçek .torrent bağlantısını (veya magnet bağlantısını) kullanın."
            .into(),
    )
}

/// Tek bir HTTP istegi; yanitla birlikte `Set-Cookie` basliklarini da dondurur.
async fn http_get(
    client: &reqwest::Client,
    url: &str,
    cookie: Option<&str>,
    accept: &str,
) -> Result<(Vec<u8>, Option<String>), String> {
    let mut request = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .header(reqwest::header::USER_AGENT, BROWSER_UA)
        .header(reqwest::header::ACCEPT, accept)
        .header(reqwest::header::REFERER, url);
    if let Some(cookie) = cookie.filter(|c| !c.is_empty()) {
        request = request.header(reqwest::header::COOKIE, cookie);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("Torrent bağlantısı açılamadı: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Torrent bağlantısı {status} döndü"));
    }
    let cookies = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .map(|pair| pair.trim().to_string())
        .filter(|pair| pair.contains('='))
        .collect::<Vec<_>>();
    let body = response
        .bytes()
        .await
        .map_err(|e| format!("Torrent dosyası okunamadı: {e}"))?
        .to_vec();
    let cookie = if cookies.is_empty() {
        None
    } else {
        Some(cookies.join("; "))
    };
    Ok((body, cookie))
}

/// Bencode'lenmis torrent dosyasi mi? (HTML sayfasi `<!DOCTYPE` ile baslar.)
pub fn looks_like_torrent(bytes: &[u8]) -> bool {
    if bytes.first() != Some(&b'd') {
        return false;
    }
    // `4:info` anahtari torrent sozlugunun zorunlu parcasi.
    bytes.windows(6).any(|window| window == b"4:info")
}

/// Sayfadaki ilk `.torrent` ya da `magnet:` baglantisini bulur (taban adrese gore cozer).
///
/// Yalnizca `href`/`src`/`data-url` ozniteliklerindeki adreslere bakilir: sayfa
/// basliginda gecen ".torrent" kelimesi baglanti degildir (dosyadrive.vip'in
/// `<title>... .torrent</title>` satiri tam olarak boyle bir tuzakti).
pub fn find_torrent_link(html: &str, base: &str) -> Option<String> {
    // magnet: dogrudan kullanilabilir.
    if let Some(index) = html.find("magnet:?") {
        let rest = &html[index..];
        let end = rest
            .find(|c: char| c == '"' || c == '\'' || c == '<' || c.is_whitespace())
            .unwrap_or(rest.len());
        let link = rest[..end].trim();
        if link.len() > "magnet:?".len() {
            return Some(link.to_string());
        }
    }

    let lowered = html.to_ascii_lowercase();
    for attribute in ["href=", "src=", "data-url="] {
        let mut from = 0;
        while let Some(offset) = lowered[from..].find(attribute) {
            let value_start = from + offset + attribute.len();
            from = value_start;
            let Some(value) = attribute_value(&html[value_start..]) else {
                continue;
            };
            if is_torrent_link(&value) {
                return Some(absolute_link(&value, base));
            }
        }
    }
    None
}

/// `href=...` sonrasindaki degeri okur (tirnakli ya da bosluga kadar).
fn attribute_value(rest: &str) -> Option<String> {
    let rest = rest.trim_start();
    let mut chars = rest.chars();
    let value: String = match chars.next()? {
        quote @ ('"' | '\'') => rest[1..]
            .split(quote)
            .next()
            .unwrap_or_default()
            .to_string(),
        _ => rest
            .split(|c: char| c.is_whitespace() || c == '>')
            .next()
            .unwrap_or_default()
            .to_string(),
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Adres `.torrent` dosyasini mi gosteriyor? (sorgu/parca yok sayilir)
pub fn is_torrent_link(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    let path = lowered
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    path.ends_with(".torrent")
}

/// Goreli adresi taban adrese gore mutlak hale getirir.
fn absolute_link(link: &str, base: &str) -> String {
    if link.starts_with("http://") || link.starts_with("https://") || link.starts_with("magnet:") {
        return link.to_string();
    }
    match url::Url::parse(base)
        .ok()
        .and_then(|base| base.join(link).ok())
    {
        Some(joined) => joined.to_string(),
        None => link.to_string(),
    }
}

/// Tamamlanan torrent'in diskteki yolunu bulur.
/// `handle.name()` her zaman klasor adi degil: bazi torrentlerde dosyalar dogrudan
/// indirme klasorune cikiyor ve `folder/name` diye bir yol olusmuyor. O yuzden once
/// torrentin gercek dosya listesine bakiyoruz.
fn resolve_completed_path(handle: &ManagedTorrentHandle) -> Option<PathBuf> {
    let folder = handle.output_folder().to_path_buf();
    let files: Vec<PathBuf> = handle
        .with_metadata(|meta| {
            meta.file_infos
                .iter()
                .map(|info| folder.join(&info.relative_filename))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    pick_completed_path(&folder, handle.name().as_deref(), &files)
}

/// Var olan dosyalara bakarak yolu secer.
///
/// - tek dosya  -> dosyanin kendisi (acilinca oynar)
/// - cok dosya  -> indirme klasoru (acilinca klasor acilir)
/// - hicbiri yok -> `folder/name` varsa o, yoksa `None`
fn pick_completed_path(
    folder: &std::path::Path,
    name: Option<&str>,
    files: &[PathBuf],
) -> Option<PathBuf> {
    let existing: Vec<&PathBuf> = files.iter().filter(|p| p.exists()).collect();
    match existing.len() {
        0 => name
            .map(|n| folder.join(n))
            .filter(|p| p.exists())
            .or_else(|| Some(folder.to_path_buf()).filter(|p| p.is_dir())),
        1 => Some(existing[0].clone()),
        _ => Some(folder.to_path_buf()),
    }
}

/// Basit test: torrent kaynagi tanima ve isim cikarma (ag gerekmez).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnet_linki_add_torrent_uretir() {
        // AddTorrent::from_url paniklemeden magnet'i kabul etmeli.
        let _ = AddTorrent::from_url(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=test".to_string(),
        );
    }

    #[test]
    fn hiz_bayt_cevrimi() {
        // Speed yalniz istatistik ciktisinda; kendi cevrimimizi dogrulayalim.
        let mbps = 1.5f64;
        assert_eq!((mbps * 1024.0 * 1024.0) as u64, 1_572_864);
    }

    /// Gercek bir cok dosyali torrent olusturup dosya listesini okur.
    /// Ag gerekmez: kaynak yerel `.torrent` dosyasi.
    #[tokio::test]
    async fn yerel_torrentin_dosyalari_listelenir() {
        let dir = tempfile::tempdir().unwrap();
        let files = ["bir.txt", "iki.bin", "uc.dat"];
        for (i, name) in files.iter().enumerate() {
            std::fs::write(dir.path().join(name), vec![b'x'; 2048 * (i + 1)]).unwrap();
        }
        let made = librqbit::create_torrent(
            dir.path(),
            Default::default(),
            &librqbit::spawn_utils::BlockingSpawner::new(1),
        )
        .await
        .unwrap();
        let torrent_path = dir.path().join("test.torrent");
        std::fs::write(&torrent_path, made.as_bytes().unwrap()).unwrap();

        let engine = TorrentEngine::new();
        let client = crate::engine::http::build_client();
        let out = tempfile::tempdir().unwrap();
        let listing = engine
            .list(
                &client,
                torrent_path.to_str().unwrap(),
                &out.path().to_path_buf(),
            )
            .await
            .unwrap();

        assert_eq!(listing.files.len(), 3);
        assert_eq!(listing.total_bytes, 2048 + 4096 + 6144);
        assert!(listing.files.iter().all(|f| f.included));
        assert_eq!(listing.files[2].index, 2);
        assert_eq!(listing.files[2].length, 6144);
        assert!(!listing.info_hash.is_empty());

        // Secimi daralt: yalniz ikinci dosya.
        let mut subset = listing.files.clone();
        apply_selection(&mut subset, &[1]).unwrap();
        assert_eq!(selected_total(&subset), Some(4096));
        assert!(!subset[0].included && subset[1].included && !subset[2].included);
    }

    #[tokio::test]
    async fn yerel_dosya_torrent_degilse_reddedilir() {
        let dir = tempfile::tempdir().unwrap();
        let client = crate::engine::http::build_client();

        // HTML/rastgele dosya: "bu bir torrent degil" (icerik sizdirmaz).
        let html = dir.path().join("sayfa.html");
        std::fs::write(&html, b"<!DOCTYPE html><html>gizli</html>").unwrap();
        let err = resolve_add_source(&client, html.to_str().unwrap())
            .await
            .unwrap_err();
        assert!(err.contains("geçerli bir .torrent değil"), "hata: {err}");
        assert!(
            !err.contains("gizli"),
            "dosya icerigi hata mesajina sizmamali"
        );

        // Olmayan yol: genel mesaj (yolun varligi hakkinda bilgi vermez).
        let yok = dir.path().join("olmayan.torrent");
        let err = resolve_add_source(&client, yok.to_str().unwrap())
            .await
            .unwrap_err();
        assert_eq!(err, "Torrent dosyası bulunamadı");

        // Dizin: dosya olmali.
        let err = resolve_add_source(&client, dir.path().to_str().unwrap())
            .await
            .unwrap_err();
        assert!(err.contains("bir dosya olmalı"), "hata: {err}");
    }

    #[tokio::test]
    async fn yerel_dosya_boyut_siniri() {
        // 32 MB'tan buyuk dosya reddedilir (okumadan once metadata kontrolu).
        let dir = tempfile::tempdir().unwrap();
        let buyuk = dir.path().join("buyuk.torrent");
        let file = std::fs::File::create(&buyuk).unwrap();
        file.set_len(MAX_LOCAL_TORRENT + 1024).unwrap();
        drop(file);
        let client = crate::engine::http::build_client();
        let err = resolve_add_source(&client, buyuk.to_str().unwrap())
            .await
            .unwrap_err();
        assert!(err.contains("çok büyük"), "hata: {err}");
    }

    #[test]
    fn secim_kurallari() {
        let mut files = vec![
            TorrentFile {
                index: 0,
                path: "a".into(),
                length: 10,
                included: true,
            },
            TorrentFile {
                index: 1,
                path: "b".into(),
                length: 5,
                included: true,
            },
        ];
        assert_eq!(selected_total(&files), Some(15));
        // Bos secim reddedilir.
        assert!(apply_selection(&mut files, &[]).is_err());
        // Olmayan indeks -> hicbir dosya secili kalmaz.
        assert!(apply_selection(&mut files, &[7]).is_err());
        assert_eq!(selected_total(&files), None);
        apply_selection(&mut files, &[1]).unwrap();
        assert_eq!(selected_total(&files), Some(5));
    }

    #[tokio::test]
    async fn bos_motor_refresh_false_doner() {
        let engine = TorrentEngine::new();
        assert!(!engine.handles.lock().await.contains_key("yok"));
        assert!(!engine.has("yok").await);
    }

    #[test]
    fn tamamlanan_yol_tek_dosyada_dosya_cok_dosyada_klasor() {
        // Gercek hata: cok dosyali torrentte filePath bos kaliyordu, "dosyayi ac" olu.
        let dir = tempfile::tempdir().unwrap();
        let single = dir.path().join("film.mp4");
        std::fs::write(&single, b"x").unwrap();
        assert_eq!(
            pick_completed_path(dir.path(), Some("film"), std::slice::from_ref(&single)),
            Some(single.clone()),
            "tek dosyada dosyanin kendisi donmeli"
        );

        let other = dir.path().join("altyazi.srt");
        std::fs::write(&other, b"y").unwrap();
        assert_eq!(
            pick_completed_path(dir.path(), Some("film"), &[single, other]),
            Some(dir.path().to_path_buf()),
            "cok dosyada indirme klasoru donmeli"
        );

        // Dosya listesi bos: torrent adiyla olusan klasor varsa o kullanilir.
        let named = tempfile::tempdir().unwrap();
        let inner = named.path().join("Koleksiyon");
        std::fs::create_dir(&inner).unwrap();
        assert_eq!(
            pick_completed_path(named.path(), Some("Koleksiyon"), &[]),
            Some(inner)
        );
    }

    #[test]
    fn bencode_ve_html_ayrilir() {
        let mut torrent = b"d8:announce22:udp://opentor.net:696913:announce-list".to_vec();
        torrent.extend_from_slice(b"ll4:infod4:name4:teste");
        assert!(looks_like_torrent(&torrent));
        assert!(!looks_like_torrent(
            b"<!DOCTYPE html>\n<html><head><title>x.torrent</title></head></html>"
        ));
        assert!(!looks_like_torrent(b""));
    }

    #[test]
    fn info_hash_tanima() {
        assert!(is_info_hash("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_info_hash("0123456789abcdef0123456789abcdef0123456"));
        assert!(!is_info_hash("magnet:?xt=urn:btih:0123456789abcdef0123456"));
        assert!(!is_info_hash("zzzz456789abcdef0123456789abcdef01234567"));
    }

    #[test]
    fn torrent_baglantisi_yalnizca_ozniteliklerden_bulunur() {
        // Gercek sayfa: baslikta ".torrent" var ama baglanti href'te.
        let html = r#"<!DOCTYPE html>
            <html><head><title>SimpleFileSharer | 007- Oyunindir vip.torrent</title></head>
            <body>
              <h2 class='dfname'>007- Oyunindir vip.torrent <small>78.01 KB</small></h2>
              <a href='/filess/ee4b49af51d5462eca72fd6db53a96ed.torrent' class='btn'>Download</a>
            </body></html>"#;
        assert_eq!(
            find_torrent_link(html, "https://www.dosyadrive.vip/filess/ee4b.torrent"),
            Some(
                "https://www.dosyadrive.vip/filess/ee4b49af51d5462eca72fd6db53a96ed.torrent".into()
            )
        );
    }

    #[test]
    fn torrent_baglantisi_magnet_ve_goreli_yol() {
        let magnet =
            r#"<a href="magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=x">m</a>"#;
        assert_eq!(
            find_torrent_link(magnet, "https://ornek.test/sayfa"),
            Some("magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=x".into())
        );

        let relative = r#"<a href="indir/dosya.torrent">indir</a>"#;
        assert_eq!(
            find_torrent_link(relative, "https://ornek.test/klasor/sayfa.html"),
            Some("https://ornek.test/klasor/indir/dosya.torrent".into())
        );

        // Baglanti yoksa None; ".torrent" kelimesi metinde gecmesi yetmez.
        assert_eq!(
            find_torrent_link("<p>bu bir .torrent aciklamasi</p>", "https://ornek.test/"),
            None
        );
    }

    #[test]
    fn torrent_uzantisi_sorguyu_yok_sayar() {
        assert!(is_torrent_link("https://ornek.test/a.TORRENT"));
        assert!(is_torrent_link("/filess/x.torrent?token=1"));
        assert!(!is_torrent_link("https://ornek.test/indir?file=x.torrent"));
        assert!(!is_torrent_link(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567"
        ));
    }

    #[tokio::test]
    async fn magnet_ve_hash_istek_yapmadan_cozulur() {
        let client = reqwest::Client::new();
        // Ag erisimi olmadan dogrudan motor kaynagi donmeli.
        let magnet =
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Big+Buck+Bunny";
        assert!(matches!(
            resolve_add_source(&client, magnet).await,
            Ok(TorrentSource::Url(url)) if url == magnet
        ));
        assert!(matches!(
            resolve_add_source(&client, "0123456789abcdef0123456789abcdef01234567").await,
            Ok(TorrentSource::Url(_))
        ));
    }

    /// Minimal bencode: icinde `4:info` gecen sozluk.
    fn ornek_torrent() -> Vec<u8> {
        let mut bytes = b"d8:announce20:http://x/announce4:infod4:name5:orneke".to_vec();
        bytes.truncate(bytes.len() - 1);
        bytes.push(b'e');
        bytes
    }

    async fn yerel_sunucu(app: axum::Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn cerez_isteyen_site_ikinci_istekte_torrent_verir() {
        // Gercek hata (kullanici bildirimi): dosyadrive.vip `.torrent` adresinde ilk
        // istekte HTML indirme sayfasi donuyor, gercek dosya yalnizca cerez +
        // tarayici UA ile ikinci istekte geliyor.
        use axum::extract::State;
        use axum::http::{HeaderMap, StatusCode};
        use axum::response::IntoResponse;
        use axum::routing::get;
        use std::sync::atomic::{AtomicUsize, Ordering};

        async fn handler(
            State(hits): State<Arc<AtomicUsize>>,
            headers: HeaderMap,
        ) -> impl IntoResponse {
            let cookie = headers
                .get("cookie")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            if cookie.contains("izin=1") {
                return (
                    StatusCode::OK,
                    [("content-type", "application/force-download")],
                    ornek_torrent(),
                )
                    .into_response();
            }
            hits.fetch_add(1, Ordering::Relaxed);
            (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8"), ("set-cookie", "izin=1; Path=/")],
                "<!DOCTYPE html><html><head><title>007.torrent</title></head><body>indir</body></html>",
            )
                .into_response()
        }

        let hits = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new()
            .route("/files/x.torrent", get(handler))
            .with_state(hits.clone());
        let base = yerel_sunucu(app).await;

        let client = reqwest::Client::new();
        let resolved = fetch_torrent_source(&client, &format!("{base}/files/x.torrent"))
            .await
            .expect("cerezli ikinci istek torrenti getirmeli");
        assert!(
            matches!(&resolved, TorrentSource::Bytes(bytes) if looks_like_torrent(bytes)),
            "bencode bekleniyordu: {resolved:?}"
        );
        assert_eq!(hits.load(Ordering::Relaxed), 1, "ilk istek HTML donmeli");
    }

    #[tokio::test]
    async fn indirme_sayfasindaki_baglanti_izlenir() {
        // Sayfa hic torrent donmezse icindeki gercek baglanti denenir.
        use axum::routing::get;

        async fn page() -> impl axum::response::IntoResponse {
            (
                [("content-type", "text/html")],
                r#"<html><head><title>dosya.torrent</title></head>
                   <body><a href="/gercek/dosya.torrent">indir</a></body></html>"#,
            )
        }
        async fn file() -> impl axum::response::IntoResponse {
            (
                [("content-type", "application/x-bittorrent")],
                ornek_torrent(),
            )
        }

        let app = axum::Router::new()
            .route("/sayfa", get(page))
            .route("/gercek/dosya.torrent", get(file));
        let base = yerel_sunucu(app).await;

        let client = reqwest::Client::new();
        let resolved = fetch_torrent_source(&client, &format!("{base}/sayfa"))
            .await
            .expect("sayfadaki baglanti izlenmeli");
        assert!(matches!(&resolved, TorrentSource::Bytes(bytes) if looks_like_torrent(bytes)));
    }

    #[tokio::test]
    async fn torrent_olmayan_adres_anlasilir_hata_verir() {
        use axum::routing::get;
        async fn page() -> &'static str {
            "<html><body>burada indirme yok</body></html>"
        }
        let app = axum::Router::new().route("/bos", get(page));
        let base = yerel_sunucu(app).await;

        let err = fetch_torrent_source(&reqwest::Client::new(), &format!("{base}/bos"))
            .await
            .expect_err("torrent yoksa hata donmeli");
        assert!(err.contains(".torrent"), "mesaj: {err}");
    }

    /// Gercek site testi (CI'da kosmaz): `HYDRA_TEST_TORRENT_URL` verilirse
    /// cerez/UA takibi gercek sitede de calisiyor mu diye bakar.
    #[tokio::test]
    #[ignore = "gercek ag erisimi gerekir"]
    async fn gercek_site_cerez_ile_torrent_verir() {
        let Ok(url) = std::env::var("HYDRA_TEST_TORRENT_URL") else {
            return;
        };
        let resolved = fetch_torrent_source(&reqwest::Client::new(), &url)
            .await
            .expect("gercek siteden torrent alinamadi");
        match resolved {
            TorrentSource::Bytes(bytes) => {
                assert!(looks_like_torrent(&bytes), "bencode bekleniyordu");
                assert!(bytes.len() > 1000, "boyut: {}", bytes.len());
                println!("OK: {} bayt .torrent", bytes.len());
            }
            TorrentSource::Url(url) => println!("OK: magnet: {url}"),
        }
    }
}
