//! Indirme yoneticisi: kuyruk, eszamanlilik, ilerleme, duraklat/devam.

use crate::engine::http::{self, HttpError};
use crate::engine::{CancelToken, RateLimiter};
use crate::model::{fair_connections, plan_segments, Download, Kind, Status};
use crate::store::{Settings, Store};
use crate::torrent::TorrentJob;
use crate::util;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter};

pub const EVENT_DOWNLOADS: &str = "downloads:changed";
pub const EVENT_NOTICE: &str = "hydra:notice";

#[derive(Debug, Clone, Default)]
pub struct AddOptions {
    pub dir: Option<PathBuf>,
    pub filename: Option<String>,
    pub referer: Option<String>,
    /// Some(true) = yt-dlp, Some(false) = dogrudan HTTP, None = otomatik.
    pub media: Option<bool>,
    /// yt-dlp format kimligi (`-f`).
    pub format: Option<String>,
}

struct Handle {
    cancel: Arc<CancelToken>,
    join: JoinHandle<()>,
}

pub struct AppState {
    pub app: AppHandle,
    pub store: Mutex<Store>,
    handles: Mutex<HashMap<String, Handle>>,
    progress: Mutex<HashMap<String, Arc<Vec<AtomicU64>>>>,
    limiter: Arc<RateLimiter>,
    http: reqwest::Client,
    started: Instant,
    pub capture: Mutex<Option<crate::capture::CaptureHandle>>,
    pub web: Mutex<Option<crate::web::WebHandle>>,
    pub torrent: crate::torrent::TorrentEngine,
}

impl AppState {
    pub fn new(app: AppHandle, mut store: Store) -> Arc<Self> {
        if store.settings.download_dir.as_os_str().is_empty() {
            store.settings.download_dir = default_download_dir();
        }
        let _ = std::fs::create_dir_all(&store.settings.download_dir);
        let limiter = RateLimiter::new(store.settings.speed_limit_bytes());
        let _ = store.save();

        Arc::new(Self {
            app,
            store: Mutex::new(store),
            handles: Mutex::new(HashMap::new()),
            progress: Mutex::new(HashMap::new()),
            limiter,
            http: http::build_client(),
            started: Instant::now(),
            capture: Mutex::new(None),
            web: Mutex::new(None),
            torrent: crate::torrent::TorrentEngine::new(),
        })
    }

    pub fn snapshot(&self) -> Vec<Download> {
        self.store.lock().unwrap().items.clone()
    }

    pub fn item(&self, id: &str) -> Option<Download> {
        self.store.lock().unwrap().find(id).cloned()
    }

    pub fn parts_dir(&self, id: &str) -> PathBuf {
        self.store.lock().unwrap().parts_dir(id)
    }

    pub fn settings(&self) -> Settings {
        self.store.lock().unwrap().settings.clone()
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn limiter(&self) -> &Arc<RateLimiter> {
        &self.limiter
    }

    /// Su anda indiren HTTP is sayisi (baglanti butcesini paylastirmak icin).
    pub fn active_http_downloads(&self) -> usize {
        let store = self.store.lock().unwrap();
        store
            .items
            .iter()
            .filter(|d| d.kind == Kind::Http && d.status == Status::Downloading)
            .count()
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    pub fn persist(&self) {
        let store = self.store.lock().unwrap();
        if let Err(e) = store.save() {
            eprintln!("[hydra] durum kaydedilemedi: {e}");
        }
    }

    pub fn emit_downloads(&self) {
        let items = self.snapshot();
        let _ = self.app.emit(EVENT_DOWNLOADS, items);
    }

    pub fn notice(&self, message: impl Into<String>, level: &str) {
        let _ = self.app.emit(
            EVENT_NOTICE,
            serde_json::json!({ "message": message.into(), "level": level }),
        );
    }

    pub fn update_item<F: FnOnce(&mut Download)>(&self, id: &str, f: F) -> bool {
        let mut store = self.store.lock().unwrap();
        match store.find_mut(id) {
            Some(item) => {
                f(item);
                true
            }
            None => false,
        }
    }

    pub fn cancel_token(&self, id: &str) -> Option<Arc<CancelToken>> {
        self.handles
            .lock()
            .unwrap()
            .get(id)
            .map(|h| h.cancel.clone())
    }

    /// Depodaki segmentleri atomik ilerleme sayaclariyla esitle.
    fn sync_progress(&self, id: &str, progress: &[AtomicU64]) {
        let mut store = self.store.lock().unwrap();
        let Some(item) = store.find_mut(id) else {
            return;
        };
        let mut sum = 0u64;
        for (i, seg) in item.segments.iter_mut().enumerate() {
            if let Some(atom) = progress.get(i) {
                let value = atom.load(Ordering::Relaxed);
                seg.done = seg.total_len().map(|len| value.min(len)).unwrap_or(value);
            }
            sum = sum.saturating_add(seg.done);
        }
        item.downloaded_bytes = sum;
        if let Some(total) = item.total_bytes {
            if total > 0 {
                item.progress = Some((sum as f64 / total as f64).clamp(0.0, 1.0));
            }
        } else {
            item.progress = None;
        }
    }

    fn clear_worker(&self, id: &str) {
        self.handles.lock().unwrap().remove(id);
        self.progress.lock().unwrap().remove(id);
    }
}

pub fn default_download_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    home.map(|h| h.join("Downloads"))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Kuyruktaki isleri bos kapasite kadar baslat.
pub fn schedule(state: &Arc<AppState>) {
    let ids = {
        let mut store = state.store.lock().unwrap();
        let max = store.settings.max_concurrent;
        let running = store.running_count();
        if running >= max {
            return;
        }
        let ids = store.take_queued(max - running);
        if ids.is_empty() {
            return;
        }
        if let Err(e) = store.save() {
            eprintln!("[hydra] durum kaydedilemedi: {e}");
        }
        ids
    };
    for id in ids {
        spawn_worker(state.clone(), id);
    }
    state.emit_downloads();
}

fn spawn_worker(state: Arc<AppState>, id: String) {
    let cancel = CancelToken::new();
    let worker_state = state.clone();
    let worker_cancel = cancel.clone();
    let task_id = id.clone();

    // `tokio::spawn` çağıran thread'de runtime bağlamı ister; `setup()` ana thread'de
    // çalıştığı için yarım kalan indirme varken uygulama açılışta
    // "there is no reactor running" paniğiyle çöküyordu (0xc0000409).
    // Tauri'nin runtime'ı her bağlamdan çağrılabilir.
    let join = tauri::async_runtime::spawn(async move {
        let kind = worker_state.item(&task_id).map(|d| d.kind);
        match kind {
            Some(Kind::Http) => {
                run_http(worker_state.clone(), task_id.clone(), worker_cancel).await
            }
            Some(Kind::Media) => {
                crate::ytdlp::run(worker_state.clone(), task_id.clone(), worker_cancel).await
            }
            // Torrent'ler kendi motorunda calisir; kuyruga girmez.
            Some(Kind::Torrent) | None => {}
        }
        worker_state.clear_worker(&task_id);
        worker_state.persist();
        worker_state.emit_downloads();
        schedule(&worker_state);
    });

    state
        .handles
        .lock()
        .unwrap()
        .insert(id, Handle { cancel, join });
}

/// tek bir HTTP indirmesini yurutur; durum/segment kaydini kendisi gunceller.
async fn run_http(state: Arc<AppState>, id: String, cancel: Arc<CancelToken>) {
    let Some(mut local) = state.item(&id) else {
        return;
    };
    // Kullanici spawn ile ilk adim arasinda duraklatmis olabilir.
    if local.status != Status::Downloading {
        return;
    }
    let parts_dir = state.parts_dir(&id);
    let _ = tokio::fs::create_dir_all(&parts_dir).await;

    if local.segments.is_empty() {
        // Baglanti butcesi tum acik indirmelere paylastirilir: 4 indirme × 16
        // baglanti = 64 soket acmak yerine 32'yi gecmeyiz.
        let active = state.active_http_downloads();
        let connections = fair_connections(local.connections, active);
        local.segments = plan_segments(local.total_bytes, local.resumable, connections);
    }
    if !local.resumable {
        // Range yok: onceki yarim veri guvenilmez, bastan basla.
        for seg in &mut local.segments {
            seg.done = 0;
        }
        http::remove_dir(&parts_dir).await;
        let _ = tokio::fs::create_dir_all(&parts_dir).await;
    }
    if local.total_bytes == Some(0) {
        finish_completed(&state, &id, &parts_dir, &local.segments, true).await;
        return;
    }

    state.update_item(&id, |item| {
        item.status = Status::Downloading;
        item.segments = local.segments.clone();
    });
    state.persist();
    state.emit_downloads();

    let mut progress = Arc::new(new_atomics(&local.segments));
    state
        .progress
        .lock()
        .unwrap()
        .insert(id.clone(), progress.clone());

    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let result = http::download_segments(http::DownloadRequest {
            client: state.http(),
            url: &local.url,
            referer: local.referer.as_deref(),
            segments: &local.segments,
            parts_dir: &parts_dir,
            cancel: &cancel,
            progress: progress.as_slice(),
            limiter: state.limiter(),
        })
        .await;
        state.sync_progress(&id, &progress);

        match result {
            Ok(()) => {
                finish_completed(&state, &id, &parts_dir, &local.segments, false).await;
                return;
            }
            Err(HttpError::Cancelled) => {
                let mut store = state.store.lock().unwrap();
                if let Some(item) = store.find_mut(&id) {
                    if item.status == Status::Downloading {
                        item.status = Status::Paused;
                    }
                    item.speed_bps = 0;
                }
                let _ = store.save();
                drop(store);
                state.emit_downloads();
                return;
            }
            Err(HttpError::RangeIgnored) if attempt < 2 => {
                // Sunucu Range'i yok saydi: tek baglanti ile bastan indir.
                http::remove_dir(&parts_dir).await;
                let _ = tokio::fs::create_dir_all(&parts_dir).await;
                local.resumable = false;
                local.connections = 1;
                local.segments = plan_segments(local.total_bytes, false, 1);
                progress = Arc::new(new_atomics(&local.segments));
                state
                    .progress
                    .lock()
                    .unwrap()
                    .insert(id.clone(), progress.clone());
                state.update_item(&id, |item| {
                    item.resumable = false;
                    item.connections = 1;
                    item.segments = local.segments.clone();
                });
                state.persist();
                continue;
            }
            Err(err) => {
                let retryable = matches!(err, HttpError::Network(_) | HttpError::Incomplete(_));
                if retryable && attempt < 3 && !cancel.is_cancelled() {
                    tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
                    if let Some(item) = state.item(&id) {
                        local.segments = item.segments;
                    }
                    state.update_item(&id, |item| item.speed_bps = 0);
                    continue;
                }
                if cancel.is_cancelled() {
                    return;
                }
                state.update_item(&id, |item| {
                    item.status = Status::Failed;
                    item.speed_bps = 0;
                    item.error = Some(err.to_string());
                    item.finished_at = Some(now_ts());
                });
                state.persist();
                state.emit_downloads();
                return;
            }
        }
    }
}

fn new_atomics(segments: &[crate::model::Segment]) -> Vec<AtomicU64> {
    segments.iter().map(|s| AtomicU64::new(s.done)).collect()
}

async fn finish_completed(
    state: &Arc<AppState>,
    id: &str,
    parts_dir: &Path,
    segments: &[crate::model::Segment],
    empty: bool,
) {
    let Some(mut item) = state.item(id) else {
        return;
    };

    // Segment kayitlari ticker'in son turundan bayat olabilir; gercek boyut
    // diskten okunur. Eksik parca varsa "tamamlandi" demek veri kaybidir.
    let mut total_written = 0u64;
    if !empty {
        let mut problem: Option<String> = None;
        for seg in segments {
            let part = parts_dir.join(format!("{}.part", seg.index));
            let size = tokio::fs::metadata(&part)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            total_written = total_written.saturating_add(size);
            if let Some(expected) = seg.total_len() {
                if size != expected && problem.is_none() {
                    problem = Some(format!(
                        "{}. parca eksik: {size}/{expected} bayt",
                        seg.index
                    ));
                }
            }
        }
        let expected_total = crate::model::segments_len(segments);
        if problem.is_none() {
            if let Some(expected) = expected_total {
                if total_written != expected {
                    problem = Some(format!(
                        "toplam boyut tutmuyor: {total_written}/{expected} bayt"
                    ));
                }
            }
        }
        if let Some(problem) = problem {
            // Parcalar silinmez: kullanici "yeniden dene" ile devam edebilir.
            state.update_item(id, |d| {
                d.status = Status::Failed;
                d.speed_bps = 0;
                d.error = Some(problem);
                d.finished_at = Some(now_ts());
            });
            state.persist();
            state.emit_downloads();
            return;
        }
    }

    let target = if empty {
        let target = util::unique_path(&item.dir, &item.filename);
        let _ = tokio::fs::File::create(&target).await;
        target
    } else {
        let candidate = util::unique_path(&item.dir, &item.filename);
        match http::concat_parts(parts_dir, segments, &candidate).await {
            Ok(()) => candidate,
            Err(err) => {
                state.update_item(id, |d| {
                    d.status = Status::Failed;
                    d.error = Some(format!("birlestirme basarisiz: {err}"));
                    d.finished_at = Some(now_ts());
                });
                state.persist();
                state.emit_downloads();
                return;
            }
        }
    };

    http::remove_dir(parts_dir).await;
    let filename = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&item.filename)
        .to_string();
    item.filename = filename;
    item.status = Status::Completed;
    item.downloaded_bytes = if empty { 0 } else { total_written };
    if item.total_bytes.is_none() && !empty {
        item.total_bytes = Some(total_written);
    }
    item.progress = Some(1.0);
    item.speed_bps = 0;
    item.error = None;
    item.finished_at = Some(now_ts());
    item.file_path = Some(target);

    state.update_item(id, |d| *d = item.clone());
    state.persist();
    state.emit_downloads();
    state.notice(format!("Tamamlandi: {}", item.filename), "success");
}

// ---------------------------------------------------------------------------
// Komutlarin kullandigi islemler
// ---------------------------------------------------------------------------

pub async fn add(
    state: &Arc<AppState>,
    url: &str,
    options: AddOptions,
) -> Result<Download, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Bos URL".into());
    }
    let settings = state.settings();
    if crate::extract::is_torrent_source(url) {
        return add_torrent(state, url, &options, &settings).await;
    }
    let parsed = url::Url::parse(url).map_err(|_| "Gecersiz URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Yalnizca http/https ve torrent baglantilari destekleniyor".into());
    }

    let dir = options
        .dir
        .clone()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| {
            if settings.download_dir.as_os_str().is_empty() {
                default_download_dir()
            } else {
                settings.download_dir.clone()
            }
        });
    std::fs::create_dir_all(&dir).map_err(|e| format!("Klasor olusturulamadi: {e}"))?;

    // Format secildiyse medya yolu zorunlu (yt-dlp format kimligi).
    // Torrent ve bilinen medya siteleri extract::kind_for_source'dan gelir.
    let hinted = crate::extract::kind_for_source(url, options.media);
    let use_media = hinted == Kind::Media || options.format.is_some();
    if use_media
        && crate::ytdlp::resolve(&state.app, &settings.ytdlp_path).is_none()
        && options.format.is_some()
    {
        return Err("Bu format yt-dlp gerektiriyor. Ayarlardan yt-dlp.exe yolunu gösterin.".into());
    }
    let use_media = use_media && crate::ytdlp::resolve(&state.app, &settings.ytdlp_path).is_some();

    let (filename, total_bytes, resumable, mime, kind) = if use_media {
        let guessed = options
            .filename
            .clone()
            .or_else(|| util::filename_from_url(url))
            .unwrap_or_else(|| "medya".into());
        (guessed, None, true, Some("media".to_string()), Kind::Media)
    } else {
        let probe = http::probe(state.http(), url).await?;
        let filename = options
            .filename
            .clone()
            .or(probe.filename)
            .or_else(|| util::filename_from_url(url))
            .unwrap_or_else(|| "download".into());
        let mime = probe
            .mime
            .clone()
            .unwrap_or_else(|| util::mime_from_filename(&filename).to_string());
        (
            filename,
            probe.total,
            probe.resumable,
            Some(mime),
            Kind::Http,
        )
    };

    let filename = util::sanitize_filename(&filename);
    let target = util::unique_path(&dir, &filename);
    let filename = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&filename)
        .to_string();

    let connections = if kind == Kind::Http {
        settings.connections
    } else {
        1
    };
    let item = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.to_string(),
        filename,
        dir,
        kind,
        status: Status::Queued,
        total_bytes,
        downloaded_bytes: 0,
        segments: Vec::new(),
        resumable,
        speed_bps: 0,
        progress: Some(0.0),
        error: None,
        created_at: now_ts(),
        finished_at: None,
        connections,
        referer: options.referer.or_else(|| Some(url.to_string())),
        mime,
        file_path: None,
        format: options.format.clone(),
        swarm: None,
        source: None,
    };

    {
        let mut store = state.store.lock().unwrap();
        store.items.insert(0, item.clone());
        let _ = store.save();
    }
    state.emit_downloads();
    schedule(state);
    Ok(item)
}

/// Torrent ekleme yolu: motor varsa dogrudan baslatir.
async fn add_torrent(
    state: &Arc<AppState>,
    source: &str,
    options: &AddOptions,
    settings: &Settings,
) -> Result<Download, String> {
    if !settings.torrent_enabled {
        return Err("Torrent motoru kapalı. Ayarlardan açabilirsiniz.".into());
    }
    let dir = options
        .dir
        .clone()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| settings.download_dir.clone());
    std::fs::create_dir_all(&dir).map_err(|e| format!("Klasor olusturulamadi: {e}"))?;

    let filename = options
        .filename
        .clone()
        .or_else(|| crate::extract::torrent_name_hint(source))
        .unwrap_or_else(|| "torrent".into());
    let item = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: source.to_string(),
        filename: util::sanitize_filename(&filename),
        dir,
        kind: Kind::Torrent,
        status: Status::Downloading,
        total_bytes: None,
        downloaded_bytes: 0,
        segments: Vec::new(),
        resumable: true,
        speed_bps: 0,
        progress: Some(0.0),
        error: None,
        created_at: now_ts(),
        finished_at: None,
        connections: 0,
        referer: None,
        mime: Some("torrent".into()),
        file_path: None,
        format: None,
        swarm: Some(crate::model::Swarm::default()),
        source: Some(source.to_string()),
    };
    {
        let mut store = state.store.lock().unwrap();
        store.items.insert(0, item.clone());
        let _ = store.save();
    }

    if let Err(err) = state
        .torrent
        .add(
            state,
            state.http(),
            TorrentJob {
                id: item.id.clone(),
                source,
                dir: item.dir.clone(),
                name: Some(item.filename.clone()),
                start_paused: false,
            },
        )
        .await
    {
        state.update_item(&item.id, |d| {
            d.status = Status::Failed;
            d.error = Some(err.clone());
            d.finished_at = Some(now_ts());
        });
        state.persist();
        state.emit_downloads();
        return Err(err);
    }
    state.emit_downloads();
    let updated = state.item(&item.id).unwrap_or(item);
    Ok(updated)
}

pub async fn pause(state: &Arc<AppState>, id: &str) -> Result<(), String> {
    let kind = state.item(id).map(|d| d.kind).ok_or("Indirme bulunamadi")?;
    if kind == Kind::Torrent {
        let current = state.item(id).map(|d| d.status);
        if !matches!(current, Some(Status::Downloading) | Some(Status::Queued)) {
            return Err("Bu durumda duraklatılamaz".into());
        }
        state.torrent.pause(state, id).await?;
        state.persist();
        state.emit_downloads();
        return Ok(());
    }
    let token = {
        let mut store = state.store.lock().unwrap();
        let item = store.find_mut(id).ok_or("Indirme bulunamadi")?;
        match item.status {
            Status::Downloading | Status::Queued => {
                item.status = Status::Paused;
                item.speed_bps = 0;
            }
            other => return Err(format!("Bu durumda duraklatilamaz: {other:?}")),
        }
        let _ = store.save();
        drop(store);
        state.cancel_token(id)
    };
    if let Some(token) = token {
        token.cancel();
    }
    state.emit_downloads();
    Ok(())
}

pub async fn resume(state: &Arc<AppState>, id: &str) -> Result<(), String> {
    let kind = state.item(id).map(|d| d.kind).ok_or("Indirme bulunamadi")?;
    if kind == Kind::Torrent {
        let current = state.item(id).map(|d| d.status);
        if current == Some(Status::Completed) {
            return Err("Zaten tamamlanmış".into());
        }
        if current == Some(Status::Downloading) {
            return Ok(());
        }
        state.torrent.resume(state, id).await?;
        state.persist();
        state.emit_downloads();
        return Ok(());
    }
    {
        let mut store = state.store.lock().unwrap();
        let item = store.find_mut(id).ok_or("Indirme bulunamadi")?;
        match item.status {
            Status::Paused | Status::Failed | Status::Cancelled | Status::Queued => {
                item.status = Status::Queued;
                item.error = None;
                item.finished_at = None;
                item.speed_bps = 0;
            }
            Status::Completed => return Err("Zaten tamamlanmis".into()),
            Status::Downloading => return Ok(()),
        }
        let _ = store.save();
    }
    state.emit_downloads();
    schedule(state);
    Ok(())
}

pub async fn cancel(state: &Arc<AppState>, id: &str) -> Result<(), String> {
    let kind = state.item(id).map(|d| d.kind).ok_or("Indirme bulunamadi")?;
    if kind == Kind::Torrent {
        {
            let mut store = state.store.lock().unwrap();
            let item = store.find_mut(id).ok_or("Indirme bulunamadi")?;
            if item.status == Status::Completed {
                return Err("Tamamlanmis indirme iptal edilemez".into());
            }
            item.status = Status::Cancelled;
            item.speed_bps = 0;
            item.finished_at = Some(now_ts());
            let _ = store.save();
        }
        state.torrent.remove(state, id, false).await;
        state.emit_downloads();
        return Ok(());
    }
    let token = {
        let mut store = state.store.lock().unwrap();
        let item = store.find_mut(id).ok_or("Indirme bulunamadi")?;
        if item.status == Status::Completed {
            return Err("Tamamlanmis indirme iptal edilemez".into());
        }
        item.status = Status::Cancelled;
        item.speed_bps = 0;
        item.finished_at = Some(now_ts());
        let _ = store.save();
        state.cancel_token(id)
    };
    if let Some(token) = token {
        token.cancel();
    }
    let parts = state.parts_dir(id);
    http::remove_dir(&parts).await;
    state.emit_downloads();
    Ok(())
}

pub async fn remove(state: &Arc<AppState>, id: &str, delete_file: bool) -> Result<(), String> {
    if state.torrent.has(id).await {
        state.torrent.remove(state, id, delete_file).await;
    }
    let handle = state.handles.lock().unwrap().remove(id);
    if let Some(handle) = handle {
        handle.cancel.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(5), handle.join).await;
    }
    let item = {
        let mut store = state.store.lock().unwrap();
        let position = store.position(id).ok_or("Indirme bulunamadi")?;
        let item = store.items.remove(position);
        let _ = store.save();
        item
    };
    state.progress.lock().unwrap().remove(id);
    http::remove_dir(&state.parts_dir(id)).await;
    if delete_file {
        let path = item
            .file_path
            .clone()
            .unwrap_or_else(|| item.dir.join(&item.filename));
        let _ = tokio::fs::remove_file(path).await;
    }
    state.emit_downloads();
    Ok(())
}

pub fn clear_finished(state: &Arc<AppState>, delete_file: bool) {
    let removed: Vec<Download> = {
        let mut store = state.store.lock().unwrap();
        let (finished, keep): (Vec<Download>, Vec<Download>) =
            store.items.drain(..).partition(|d| {
                matches!(
                    d.status,
                    Status::Completed | Status::Failed | Status::Cancelled
                )
            });
        store.items = keep;
        let _ = store.save();
        finished
    };
    if delete_file {
        for item in removed {
            let path = item
                .file_path
                .clone()
                .unwrap_or_else(|| item.dir.join(&item.filename));
            let _ = std::fs::remove_file(path);
        }
    }
    state.emit_downloads();
}

pub fn save_settings(state: &Arc<AppState>, mut settings: Settings) -> Result<Settings, String> {
    settings.normalize();
    if settings.download_dir.as_os_str().is_empty() {
        settings.download_dir = default_download_dir();
    }
    std::fs::create_dir_all(&settings.download_dir)
        .map_err(|e| format!("Klasor olusturulamadi: {e}"))?;
    {
        let mut store = state.store.lock().unwrap();
        store.settings = settings.clone();
        let _ = store.save();
    }
    state.limiter.set_limit(settings.speed_limit_bytes());
    state.emit_downloads();
    schedule(state);
    Ok(settings)
}

/// Uygulama acilisinda yarim kalanlari kuyruga al.
pub fn resume_pending(state: &Arc<AppState>) {
    if !state.settings().auto_resume {
        let mut store = state.store.lock().unwrap();
        for item in &mut store.items {
            if item.status == Status::Queued {
                item.status = Status::Paused;
            }
        }
        let _ = store.save();
        drop(store);
        state.emit_downloads();
        return;
    }
    schedule(state);
}

/// Acilista yarim kalan torrent'leri motors uz yeniden baglar.
pub fn resume_torrents(state: &Arc<AppState>) {
    let pending: Vec<(String, String, std::path::PathBuf, String)> = {
        let store = state.store.lock().unwrap();
        store
            .items
            .iter()
            .filter(|d| d.kind == Kind::Torrent && d.status != Status::Completed)
            .filter_map(|d| {
                d.source
                    .clone()
                    .map(|source| (d.id.clone(), source, d.dir.clone(), d.filename.clone()))
            })
            .collect()
    };
    if pending.is_empty() {
        return;
    }
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        for (id, source, dir, filename) in pending {
            let paused = state
                .item(&id)
                .map(|d| d.status == Status::Paused)
                .unwrap_or(false);
            if let Err(err) = state
                .torrent
                .add(
                    &state,
                    state.http(),
                    TorrentJob {
                        id: id.clone(),
                        source: &source,
                        dir,
                        name: Some(filename),
                        start_paused: paused,
                    },
                )
                .await
            {
                state.update_item(&id, |d| {
                    d.status = Status::Failed;
                    d.error = Some(format!("Torrent yeniden yüklenemedi: {err}"));
                });
            }
        }
        state.persist();
        state.emit_downloads();
    });
}

pub fn start_ticker(state: &Arc<AppState>) {
    let weak = Arc::downgrade(state);
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(400));
        let mut last: HashMap<String, (u64, Instant)> = HashMap::new();
        let mut last_save = Instant::now();
        loop {
            tick.tick().await;
            let Some(state) = weak.upgrade() else { break };
            let atoms = state.progress.lock().unwrap().clone();
            let mut active = false;
            {
                let mut store = state.store.lock().unwrap();
                let now = Instant::now();
                for item in &mut store.items {
                    if let Some(seg_atoms) = atoms.get(&item.id) {
                        let mut sum = 0u64;
                        for (i, seg) in item.segments.iter_mut().enumerate() {
                            if let Some(atom) = seg_atoms.get(i) {
                                let value = atom.load(Ordering::Relaxed);
                                seg.done =
                                    seg.total_len().map(|len| value.min(len)).unwrap_or(value);
                            }
                            sum = sum.saturating_add(seg.done);
                        }
                        item.downloaded_bytes = sum;
                    }
                    if item.status == Status::Downloading {
                        active = true;
                        match last.get(&item.id).copied() {
                            Some((prev_bytes, prev_at)) if now > prev_at => {
                                let dt = now.duration_since(prev_at).as_secs_f64();
                                let delta = item.downloaded_bytes.saturating_sub(prev_bytes);
                                let instant = (delta as f64 / dt) as u64;
                                item.speed_bps =
                                    (item.speed_bps as f64 * 0.55 + instant as f64 * 0.45) as u64;
                                last.insert(item.id.clone(), (item.downloaded_bytes, now));
                            }
                            Some(_) => {}
                            None => {
                                last.insert(item.id.clone(), (item.downloaded_bytes, now));
                                item.speed_bps = 0;
                            }
                        }
                    } else {
                        last.remove(&item.id);
                        item.speed_bps = 0;
                    }
                }
                if active && last_save.elapsed() >= Duration::from_secs(2) {
                    let _ = store.save();
                    last_save = Instant::now();
                }
            }
            // Torrent motoru kendi hiz/ilerleme verisini yazar (hiz hesabi orada).
            let torrent_active = state.torrent.refresh(&state).await;
            if active || torrent_active {
                state.emit_downloads();
            }
            schedule(&state);
        }
    });
}
