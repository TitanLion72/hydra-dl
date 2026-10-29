//! Yerel HTTP API'si ve mobil web arayuzu.
//!
//! Ayni yonlendirici iki yerde kullanilir:
//! - yakalama sunucusu (`127.0.0.1:6712`, tarayici eklentisi, token yok)
//! - web sunucusu (`0.0.0.0:6790`, telefon/tablet, istege bagli token)

use crate::manager::{self, AddOptions, AppState};
use crate::model::{Download, Kind};
use crate::store::Settings;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;
use tokio::sync::oneshot;

pub struct WebHandle {
    pub port: u16,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for WebHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        self.task.abort();
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub bind: String,
    pub url: Option<String>,
    pub lan_urls: Vec<String>,
    pub token_required: bool,
}

// ---------------------------------------------------------------------------
// İstek gövdeleri
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProbeBody {
    pub url: String,
    #[serde(default)]
    pub media: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct AddBody {
    pub url: String,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub referer: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub media: Option<bool>,
    /// Torrent'te indirilecek dosya indeksleri.
    #[serde(default)]
    pub files: Option<Vec<usize>>,
}

#[derive(Debug, Deserialize)]
pub struct AddQuery {
    pub url: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub referer: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub media: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct TorrentBody {
    pub source: String,
    #[serde(default)]
    pub dir: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TorrentFilesBody {
    /// Dosya listesini isteyecegimiz torrent kaynagi.
    #[serde(default)]
    pub source: Option<String>,
    /// Indirme kimligi (eklenmis bir torrent'in secimini degistirmek icin).
    #[serde(default)]
    pub id: Option<String>,
    /// Yeni secim; `source` verildiginde yok sayilir.
    #[serde(default)]
    pub files: Option<Vec<usize>>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    #[serde(default, rename = "deleteFile")]
    pub delete_file: Option<bool>,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(serde_json::json!({ "ok": false, "error": message.into() })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// İşleyiciler
// ---------------------------------------------------------------------------

async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let items = state.snapshot();
    Json(serde_json::json!({
        "app": "hydra-dl",
        "version": env!("CARGO_PKG_VERSION"),
        "uptimeSecs": state.uptime_secs(),
        "downloads": items.len(),
        "active": items.iter().filter(|d| d.status.is_running()).count(),
    }))
}

async fn list_downloads(State(state): State<Arc<AppState>>) -> Json<Vec<Download>> {
    Json(state.snapshot())
}

/// SSE: her degisiklikte tam listeyi yayinlar.
async fn events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut last = String::new();
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        loop {
            tick.tick().await;
            let items = state.snapshot();
            let payload = serde_json::to_string(&items).unwrap_or_else(|_| "[]".into());
            if payload != last {
                last = payload.clone();
                yield Ok(Event::default().event("downloads").data(payload));
            } else {
                yield Ok(Event::default().comment("ping"));
            }
            if state.uptime_secs() > 60 * 60 * 24 * 30 {
                break;
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

async fn probe(State(state): State<Arc<AppState>>, Json(body): Json<ProbeBody>) -> Response {
    probe_impl(state, body.url, body.media).await
}

async fn probe_get(State(state): State<Arc<AppState>>, Query(query): Query<ProbeBody>) -> Response {
    probe_impl(state, query.url, query.media).await
}

async fn probe_impl(state: Arc<AppState>, url: String, media: Option<bool>) -> Response {
    if url.trim().is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "url gerekli");
    }
    let settings = state.settings();
    let ytdlp = crate::ytdlp::resolve(&state.app, &settings.ytdlp_path);
    match crate::extract::probe(&state.http(), ytdlp.as_deref(), &url, media).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(err) => error_response(StatusCode::UNPROCESSABLE_ENTITY, err),
    }
}

async fn add(State(state): State<Arc<AppState>>, Json(body): Json<AddBody>) -> Response {
    add_impl(
        state,
        AddBody {
            url: body.url,
            dir: body.dir,
            filename: body.filename,
            referer: body.referer,
            format: body.format,
            media: body.media,
            files: body.files,
        },
    )
    .await
}

async fn add_get(State(state): State<Arc<AppState>>, Query(query): Query<AddQuery>) -> Response {
    add_impl(
        state,
        AddBody {
            url: query.url,
            dir: query.dir,
            filename: query.filename,
            referer: query.referer,
            format: query.format,
            media: query.media,
            files: None,
        },
    )
    .await
}

async fn add_impl(state: Arc<AppState>, body: AddBody) -> Response {
    if body.url.trim().is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "url gerekli");
    }
    let options = AddOptions {
        dir: body.dir.filter(|d| !d.trim().is_empty()).map(PathBuf::from),
        filename: body.filename.filter(|f| !f.trim().is_empty()),
        referer: body.referer,
        media: body.media,
        format: body.format.filter(|f| !f.trim().is_empty()),
        files: body.files,
    };
    match manager::add(&state, &body.url, options).await {
        Ok(item) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "id": item.id,
                "filename": item.filename,
                "totalBytes": item.total_bytes,
                "media": matches!(item.kind, Kind::Media),
                "torrent": matches!(item.kind, Kind::Torrent),
            })),
        )
            .into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

async fn pause(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match manager::pause(&state, &id).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

async fn resume(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match manager::resume(&state, &id).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

async fn cancel(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match manager::cancel(&state, &id).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

async fn remove(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<DeleteQuery>,
) -> Response {
    match manager::remove(&state, &id, query.delete_file.unwrap_or(false)).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

async fn get_settings(State(state): State<Arc<AppState>>) -> Json<Settings> {
    Json(state.settings())
}

async fn put_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<Settings>,
) -> Response {
    let previous = state.settings();
    match manager::save_settings(&state, settings) {
        Ok(saved) => {
            if previous.web_enabled != saved.web_enabled
                || previous.web_port != saved.web_port
                || previous.web_bind != saved.web_bind
                || previous.web_token != saved.web_token
            {
                if let Err(err) = restart_web(&state, &saved).await {
                    return error_response(StatusCode::CONFLICT, err);
                }
            }
            if previous.capture_enabled != saved.capture_enabled
                || previous.capture_port != saved.capture_port
            {
                if let Err(err) =
                    crate::capture::restart(&state, saved.capture_enabled, saved.capture_port).await
                {
                    return error_response(StatusCode::CONFLICT, err);
                }
            }
            Json(saved).into_response()
        }
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

/// `GET /torrents` — indirme listesindeki torrent'lerin ozeti (dokumandaki soz).
///
/// Ayrintili veri `/downloads` icinde durur; bu ucnokta hata ayiklarken torrent
/// durumunu tek yerden gormek icin var.
async fn list_torrents(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let torrents: Vec<serde_json::Value> = state
        .snapshot()
        .into_iter()
        .filter(|d| matches!(d.kind, Kind::Torrent))
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "filename": d.filename,
                "status": d.status,
                "downloadedBytes": d.downloaded_bytes,
                "totalBytes": d.total_bytes,
                "speedBps": d.speed_bps,
                "peers": d.swarm.as_ref().map(|s| s.peers),
                "error": d.error,
                "filePath": d.file_path,
            })
        })
        .collect();
    Json(serde_json::json!({ "ok": true, "torrents": torrents }))
}

async fn add_torrent(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TorrentBody>,
) -> Response {
    let options = AddOptions {
        dir: body.dir.filter(|d| !d.trim().is_empty()).map(PathBuf::from),
        ..Default::default()
    };
    match manager::add(&state, &body.source, options).await {
        Ok(item) => (
            StatusCode::OK,
            Json(serde_json::json!({ "ok": true, "id": item.id, "filename": item.filename })),
        )
            .into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

/// `POST /notify/test` — bildirim altyapısını dener (ayar açık olmasa da gönderir).
async fn notify_test(State(state): State<Arc<AppState>>) -> Response {
    match state.send_notification("Hydra DL", "Bildirimler çalışıyor.") {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Err(err) => error_response(StatusCode::BAD_GATEWAY, err),
    }
}

/// `POST /torrent/files`
/// - `{ "source": "magnet:..." }` → torrent dosya listesi (indirme baslamaz).
/// - `{ "id": "...", "files": [0,2] }` → eklenmis torrent'in secimini degistirir.
async fn torrent_files(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TorrentFilesBody>,
) -> Response {
    if let Some(source) = body.source.filter(|s| !s.trim().is_empty()) {
        let settings = state.settings();
        if !settings.torrent_enabled {
            return error_response(
                StatusCode::BAD_REQUEST,
                "Torrent motoru kapalı. Ayarlardan açabilirsiniz.",
            );
        }
        let dir = settings.download_dir.clone();
        match state.torrent.list(&state.http(), &source, &dir).await {
            Ok(listing) => (StatusCode::OK, Json(listing)).into_response(),
            Err(err) => error_response(StatusCode::UNPROCESSABLE_ENTITY, err),
        }
    } else if let Some(id) = body.id {
        match body.files {
            Some(files) => match state.torrent.set_files(&state, &id, files).await {
                Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
                Err(err) => error_response(StatusCode::BAD_REQUEST, err),
            },
            None => error_response(StatusCode::BAD_REQUEST, "files gerekli"),
        }
    } else {
        error_response(StatusCode::BAD_REQUEST, "source veya id gerekli")
    }
}

/// yt-dlp'yi indirip kurar. Uzun sürebilir; istemci makul bir zaman aşımı kullanmalı.
async fn install_ytdlp(State(state): State<Arc<AppState>>) -> Response {
    let app = state.app.clone();
    let client = state.http().clone();
    match crate::ytdlp::install(&app, &client, |_| {}).await {
        Ok(path) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "path": path.to_string_lossy().to_string(),
            })),
        )
            .into_response(),
        Err(err) => error_response(StatusCode::BAD_GATEWAY, err),
    }
}

async fn ytdlp_status(State(state): State<Arc<AppState>>) -> Response {
    let settings = state.settings();
    let status = crate::ytdlp::status(&state.app, &settings.ytdlp_path).await;
    (StatusCode::OK, Json(status)).into_response()
}

async fn index(State(state): State<Arc<AppState>>) -> Response {
    serve_static(&state, "index.html")
}

/// Statik dosyalar calisma aninda diskten okunur: `webapp/` gelistirme
/// dizininde ya da paket kaynaklarinda (`<kaynak>/webapp`) bulunur.
/// Aday köklerden `index.html` içeren ilk `webapp/` klasörünü seçer.
/// Saf fonksiyon: paket yolları değişince testle yakalanabilsin.
pub fn pick_webapp_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|base| base.join("index.html").is_file())
        .cloned()
}

/// Statik dosyalar çalışma anında diskten okunur. Aranan yerler, sırayla:
/// 1. Paket kaynakları: `resource/webapp` (tauri `resources` hedefi)
/// 2. Paket kaynakları: `resource/_up_/webapp` (kaynak `src-tauri` dışındaysa
///    Tauri dosyaları `_up_` altına koyar — eski paketlerle uyumluluk)
/// 3. Geliştirme: çalışma dizini ve bir üstü (repo kökü)
fn webapp_dir(state: &AppState) -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(resource) = state.app.path().resource_dir() {
        candidates.push(resource.join("webapp"));
        candidates.push(resource.join("_up_").join("webapp"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("webapp"));
            candidates.push(dir.join("_up_").join("webapp"));
        }
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    candidates.push(cwd.join("webapp"));
    candidates.push(cwd.join("..").join("webapp"));

    pick_webapp_dir(&candidates).unwrap_or_else(|| cwd.join("webapp"))
}

fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "webmanifest" => "application/manifest+json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// İstenen yolu kök klasöre göre çözer. Kök dışına çıkabilecek isteklerde `None`.
///
/// `Path::join` Windows'ta mutlak yolu **ezer**: `root.join("C:/Windows/win.ini")`
/// kökü atlayıp o dosyayı verir. Bu yüzden '..' / '\' reddedilir **ve** her
/// bileşenin `Normal`/`CurDir` olduğu doğrulanır (sürücü öneki, UNC, kök dizin
/// bileşeni kabul edilmez).
fn resolve_static_path(root: &FsPath, requested: &str) -> Option<PathBuf> {
    let trimmed = requested.trim_start_matches('/');
    if trimmed.contains("..") || trimmed.contains('\\') {
        return None;
    }
    for component in FsPath::new(trimmed).components() {
        if !matches!(component, Component::Normal(_) | Component::CurDir) {
            return None;
        }
    }
    let candidate = if trimmed.is_empty() {
        root.join("index.html")
    } else {
        root.join(trimmed)
    };
    // Son savunma: gerçek yol (symlink'ler çözülmüş) kökün altında olmalı.
    if let (Ok(real_root), Ok(real)) = (
        std::fs::canonicalize(root),
        std::fs::canonicalize(&candidate),
    ) {
        if !real.starts_with(&real_root) {
            return None;
        }
    }
    Some(candidate)
}

fn serve_static(state: &AppState, requested: &str) -> Response {
    let root = webapp_dir(state);
    let Some(mut candidate) = resolve_static_path(&root, requested) else {
        return error_response(StatusCode::BAD_REQUEST, "geçersiz yol");
    };
    let trimmed = requested.trim_start_matches('/');
    if !candidate.is_file() {
        if trimmed.contains('.') {
            return error_response(StatusCode::NOT_FOUND, "bulunamadı");
        }
        candidate = root.join("index.html");
        if !candidate.is_file() {
            return error_response(
                StatusCode::NOT_FOUND,
                "web arayüzü bulunamadı (webapp/index.html)",
            );
        }
    }
    match std::fs::read(&candidate) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                content_type_for(candidate.to_string_lossy().as_ref()),
            )
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(err) => error_response(StatusCode::NOT_FOUND, format!("okunamadı: {err}")),
    }
}

async fn static_file(State(state): State<Arc<AppState>>, Path(path): Path<String>) -> Response {
    serve_static(&state, &path)
}

// ---------------------------------------------------------------------------
// Yönlendirici
// ---------------------------------------------------------------------------

pub fn api_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/downloads", get(list_downloads))
        .route("/events", get(events))
        .route("/probe", get(probe_get).post(probe))
        .route("/add", get(add_get).post(add))
        .route("/downloads/{id}/pause", post(pause))
        .route("/downloads/{id}/resume", post(resume))
        .route("/downloads/{id}/cancel", post(cancel))
        .route("/downloads/{id}", delete(remove))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/torrents", post(add_torrent).get(list_torrents))
        .route("/torrent/files", post(torrent_files))
        .route("/notify/test", post(notify_test))
        .route("/ytdlp", get(ytdlp_status))
        .route("/ytdlp/install", post(install_ytdlp))
        .route("/assets/{*path}", get(static_file))
        .route("/{*path}", get(static_file))
        .with_state(state)
}
// ---------------------------------------------------------------------------
// Sunucu yasam dongusu
// ---------------------------------------------------------------------------

/// Yol token istiyor mu? **Varsayılan: evet.**
///
/// Yalnızca sağlık kontrolü ve gerçekten var olan statik dosyalar serbesttir.
/// Böylece yeni eklenen her API yolu otomatik olarak korunur; "prefix listesine
/// eklemeyi unuttum" sınıfı hatalar (ör. `/torrent/files`) oluşamaz.
fn needs_token(path: &str, webapp_root: &FsPath) -> bool {
    if path == "/health" {
        return false;
    }
    match resolve_static_path(webapp_root, path) {
        Some(file) => !file.is_file(),
        None => true,
    }
}

/// `Origin` başlığı bu sunucu için kabul edilebilir mi?
///
/// Tarayıcı `Origin`'i sayfa tarafından değiştirilemez; bu yüzden rastgele bir
/// web sitesinin `fetch("http://127.0.0.1:6712/add", ...)` çağrısı (localhost
/// CSRF) burada durur. Eklenti (`chrome-extension://`, `moz-extension://`) ve
/// tarayıcı dışı istemciler (Origin yok) serbesttir.
pub fn origin_allowed(origin: Option<&str>, host: Option<&str>) -> bool {
    let Some(origin) = origin.map(str::trim).filter(|o| !o.is_empty()) else {
        // Origin yok: curl, eklenti arka planı, Tauri arayüzü.
        return true;
    };
    let lower = origin.to_ascii_lowercase();
    if lower == "null" {
        // Sandbox iframe / data: URL — kaynağı belli değil, güvenme.
        return false;
    }
    for scheme in [
        "chrome-extension://",
        "moz-extension://",
        "safari-web-extension://",
        "safari-extension://",
    ] {
        if lower.starts_with(scheme) {
            return true;
        }
    }
    // http(s): yalnız aynı köken (Host ile aynı authority) kabul edilir.
    let Some(rest) = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))
    else {
        return false;
    };
    let origin_authority = rest.split('/').next().unwrap_or("");
    let Some(host) = host.map(str::trim).filter(|h| !h.is_empty()) else {
        return false;
    };
    origin_authority.eq_ignore_ascii_case(host)
}

fn origin_guard(
    req: Request<Body>,
    next: Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> {
    Box::pin(async move {
        let origin = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok());
        if !origin_allowed(origin, host) {
            return error_response(
                StatusCode::FORBIDDEN,
                "bu kaynaktan gelen istek kabul edilmiyor",
            );
        }
        next.run(req).await
    })
}

/// Yonlendiriciye Origin korumasini uygular (yakalama + web sunucusu ortak).
///
/// Yakalama sunucusu `127.0.0.1`'de ve **token'siz** calisir; korunmazsa
/// kullanicinin ziyaret ettigi herhangi bir web sayfasi `fetch` ile indirme
/// ekleyebilir/ayarlari degistirebilir (localhost CSRF). Bu yuzden iki sunucu da
/// ayni korumayi kullanir.
pub fn with_origin_guard(router: Router) -> Router {
    router.layer(middleware::from_fn(origin_guard))
}

fn auth_middleware(
    token: String,
    webapp_root: PathBuf,
) -> impl Fn(
    Request<Body>,
    Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
       + Clone {
    move |request: Request<Body>, next: Next| {
        let token = token.clone();
        let root = webapp_root.clone();
        Box::pin(async move {
            if token.is_empty() {
                // Token yok: kurulum kolaylığı için açık, ama arayüz uyarı gösterir.
                return next.run(request).await;
            }
            let path = request.uri().path().to_string();
            if !needs_token(&path, &root) {
                return next.run(request).await;
            }
            let header_ok = request
                .headers()
                .get("x-hydra-token")
                .and_then(|v| v.to_str().ok())
                .map(|v| v == token)
                .unwrap_or(false);
            let query_ok = request.uri().query().map(|q| {
                q.split('&').any(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    parts.next() == Some("token") && parts.next() == Some(token.as_str())
                })
            });
            if header_ok || query_ok == Some(true) {
                next.run(request).await
            } else {
                error_response(StatusCode::UNAUTHORIZED, "geçersiz token")
            }
        })
    }
}

fn cors(
    req: Request<Body>,
    next: Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> {
    Box::pin(async move {
        if req.method() == axum::http::Method::OPTIONS {
            let mut preflight = StatusCode::NO_CONTENT.into_response();
            add_cors_headers(preflight.headers_mut());
            return preflight;
        }
        let mut response = next.run(req).await;
        add_cors_headers(response.headers_mut());
        response
    })
}

fn add_cors_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, x-hydra-token"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
}

async fn serve(
    state: Arc<AppState>,
    bind: String,
    port: u16,
    token: Option<String>,
) -> Result<WebHandle, String> {
    let ip: IpAddr = bind
        .parse()
        .map_err(|_| format!("Geçersiz dinleme adresi: {bind}"))?;
    let addr = SocketAddr::new(ip, port);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Web sunucusu {addr} üzerinde başlatılamadı: {e}"))?;
    let actual_port = listener
        .local_addr()
        .map_err(|e| format!("Adres okunamadı: {e}"))?
        .port();

    // Statik kok: token korumasinin "hangi yol serbest" karari icin gerekli.
    let webapp_root = webapp_dir(&state);
    // Katman sirasi (dis -> ic): origin_guard, cors, auth, router.
    let mut router = api_router(state).layer(middleware::from_fn(auth_middleware(
        token.clone().unwrap_or_default(),
        webapp_root,
    )));
    router = router.layer(middleware::from_fn(cors));
    router = with_origin_guard(router);

    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, router).with_graceful_shutdown(async {
            let _ = rx.await;
        });
        if let Err(err) = server.await {
            eprintln!("[hydra] web sunucusu durdu: {err}");
        }
    });
    Ok(WebHandle {
        port: actual_port,
        shutdown: Some(tx),
        task,
    })
}

/// Web sunucusunu (gerekirse) yeniden baslatir.
pub async fn restart_web(state: &Arc<AppState>, settings: &Settings) -> Result<(), String> {
    let previous = state.web.lock().unwrap().take();
    drop(previous);
    if !settings.web_enabled {
        return Ok(());
    }
    let handle = serve(
        state.clone(),
        settings.web_bind.clone(),
        settings.web_port,
        Some(settings.web_token.clone()),
    )
    .await?;
    *state.web.lock().unwrap() = Some(handle);
    Ok(())
}

pub async fn restart_from_settings(state: &Arc<AppState>) -> Result<(), String> {
    let settings = state.settings();
    restart_web(state, &settings).await
}

pub fn lan_addresses() -> Vec<String> {
    // UDP soketi ile varsayilan arayuz IP'sini ogren (paket gondermez).
    let mut out = Vec::new();
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local) = socket.local_addr() {
                out.push(local.ip().to_string());
            }
        }
    }
    out
}

pub fn status(state: &AppState) -> WebStatus {
    let guard = state.web.lock().unwrap();
    let settings = state.settings();
    let running = guard.is_some();
    let port = guard.as_ref().map(|h| h.port).unwrap_or(settings.web_port);
    let host = if settings.web_bind == "0.0.0.0" || settings.web_bind.is_empty() {
        lan_addresses()
            .into_iter()
            .next()
            .unwrap_or_else(|| "127.0.0.1".into())
    } else {
        settings.web_bind.clone()
    };
    let url = running.then(|| format!("http://{host}:{port}"));
    let lan_urls = if running {
        lan_addresses()
            .into_iter()
            .map(|ip| format!("http://{ip}:{port}"))
            .collect()
    } else {
        Vec::new()
    };
    WebStatus {
        enabled: settings.web_enabled,
        running,
        port,
        bind: settings.web_bind,
        url,
        lan_urls,
        token_required: !settings.web_token.is_empty(),
    }
}

/// Web adresi icin QR kod (SVG data URI). Telefonla eslesme icin.
pub fn qr_svg_data_uri(text: &str) -> Result<String, String> {
    use base64::Engine;
    let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::M)
        .map_err(|e| format!("QR üretilemedi: {e}"))?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(220, 220)
        .dark_color(qrcode::render::svg::Color("#0b1320"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    let encoded = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_uretilir() {
        let uri = qr_svg_data_uri("http://192.168.1.20:6790?token=abc").unwrap();
        assert!(uri.starts_with("data:image/svg+xml;base64,"));
        assert!(uri.len() > 500);
    }

    #[test]
    fn webapp_dizini_adaylardan_seciliir() {
        let temp = tempfile::tempdir().unwrap();
        let bos = temp.path().join("bos");
        std::fs::create_dir_all(&bos).unwrap();
        // Hicbir adayda index.html yoksa None.
        assert!(pick_webapp_dir(std::slice::from_ref(&bos)).is_none());

        let dolu = temp.path().join("webapp");
        std::fs::create_dir_all(&dolu).unwrap();
        std::fs::write(dolu.join("index.html"), b"<html></html>").unwrap();
        // Ikinci aday dolu; ilk bos oldugu icin ikincisi secilmeli.
        assert_eq!(pick_webapp_dir(&[bos, dolu.clone()]), Some(dolu));
    }

    /// Guvenlik: token korumasi **varsayilan olarak her yolu** kapsar.
    /// Yalniz `/health` ve gercekten var olan statik dosyalar serbesttir.
    /// Eski "prefix listesi" yaklasiminda `/torrent/files` ve `/notify/test`
    /// korumasiz kaliyordu (gercek acik, kapatildi).
    #[test]
    fn token_varsayilan_olarak_her_yolu_korur() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("webapp");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("index.html"), b"<html></html>").unwrap();
        std::fs::write(root.join("app.js"), b"x").unwrap();
        std::fs::write(root.join("styles.css"), b"x").unwrap();
        std::fs::write(root.join("assets/i.js"), b"x").unwrap();

        // Serbest: saglik + gercek statik dosyalar.
        for serbest in [
            "/health",
            "/",
            "/index.html",
            "/app.js",
            "/assets/i.js",
            "/styles.css",
        ] {
            assert!(!needs_token(serbest, &root), "serbest olmali: {serbest}");
        }

        // Korumali: tum API yollari (eskiden korumasiz kalanlar dahil).
        for korumali in [
            "/downloads",
            "/settings",
            "/probe",
            "/add",
            "/torrents",
            "/torrent/files",
            "/notify/test",
            "/ytdlp",
            "/ytdlp/install",
            "/events",
            "/yeni/gelecek/uc",
            "/olmayan-dosya.txt",
        ] {
            assert!(needs_token(korumali, &root), "token istemeli: {korumali}");
        }
    }

    #[test]
    fn origin_kontrolu_web_sayfasini_engeller() {
        // Tarayici disi istemciler ve eklentiler serbest.
        assert!(origin_allowed(None, Some("127.0.0.1:6712")));
        assert!(origin_allowed(
            Some("chrome-extension://abcdefghijklmnop"),
            Some("127.0.0.1:6712")
        ));
        assert!(origin_allowed(
            Some("moz-extension://1111-2222"),
            Some("127.0.0.1:6712")
        ));
        // Ayni kokun (web arayuzu kendisi).
        assert!(origin_allowed(
            Some("http://192.168.1.10:6790"),
            Some("192.168.1.10:6790")
        ));
        // Rastgele web sitesi -> localhost CSRF denemesi engellenir.
        for kotu in [
            "https://evil.example",
            "http://localhost:3000",
            "http://127.0.0.1:6790", // farkli port = farkli kokun
            "http://192.168.1.10:6791",
            "null",
            "file://",
            "data:text/html,x",
        ] {
            assert!(
                !origin_allowed(Some(kotu), Some("127.0.0.1:6712")),
                "engellenmeli: {kotu}"
            );
        }
        // Kendi kokunu (Host ile ayni) kabul edilir: web arayuzu kendi API'sini cagirir.
        assert!(origin_allowed(
            Some("http://127.0.0.1:6712"),
            Some("127.0.0.1:6712")
        ));
        // Host yoksa http(s) kaynagi guvenilmez.
        assert!(!origin_allowed(Some("http://127.0.0.1:6712"), None));
    }

    #[test]
    fn yakalama_sunucusu_da_origin_korumali() {
        // Yakalama sunucusu 127.0.0.1'de ve token'siz; korunmazsa ziyaret edilen
        // herhangi bir sayfa fetch ile indirme ekleyebilirdi. capture.rs
        // `with_origin_guard` kullanmali.
        let src = include_str!("capture.rs");
        assert!(
            src.contains("with_origin_guard"),
            "capture.rs Origin korumasini uygulamali"
        );
    }

    #[test]
    fn lan_adresi_bos_olmaz() {
        // Ag yoksa bos olabilir; sadece paniklemedigini dogrula.
        let _ = lan_addresses();
    }

    /// Guvenlik: web sunucusu LAN'a acik; kok klasor disina cikan her istek
    /// reddedilmeli. `Path::join` Windows'ta "C:/..." ile koku eziyordu ve
    /// `GET /C:/Windows/win.ini` 200 donuyordu (gercek acik, kapatildi).
    #[test]
    fn statik_yol_kok_disina_cikamaz() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("webapp");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("index.html"), b"<html></html>").unwrap();
        std::fs::write(root.join("assets/app.js"), b"x").unwrap();
        // Kok disinda bir dosya: sizarsa test yakalar.
        std::fs::write(temp.path().join("gizli.txt"), b"gizli").unwrap();

        assert_eq!(
            resolve_static_path(&root, "index.html"),
            Some(root.join("index.html"))
        );
        assert_eq!(
            resolve_static_path(&root, ""),
            Some(root.join("index.html"))
        );
        assert_eq!(
            resolve_static_path(&root, "/assets/app.js"),
            Some(root.join("assets/app.js"))
        );
        assert_eq!(
            resolve_static_path(&root, "assets/./app.js"),
            Some(root.join("assets/app.js"))
        );

        for saldiri in [
            "C:/Windows/win.ini",
            "c:/windows/win.ini",
            "C:\\Windows\\win.ini",
            "\\\\sunucu\\paylasim\\x.txt",
            "../gizli.txt",
            "assets/../../gizli.txt",
            "..%2fgizli.txt",
            "/../gizli.txt",
        ] {
            assert_eq!(
                resolve_static_path(&root, saldiri),
                None,
                "saldiri engellenmeli: {saldiri}"
            );
        }

        // Bas yegane slash'ler kirpilir; sonuc kokun ALTINDA kalir (kacis yok).
        let mutlak_gorunum = resolve_static_path(&root, "//etc/passwd").unwrap();
        assert!(
            mutlak_gorunum.starts_with(&root),
            "kok altinda kalmali: {}",
            mutlak_gorunum.display()
        );
        assert!(!mutlak_gorunum.exists());
    }

    /// Symlink kokun disini gosteriyorsa da reddedilmeli (canonicalize kontrolu).
    #[cfg(windows)]
    #[test]
    fn statik_yol_symlink_ile_de_kacilamaz() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("webapp");
        let disari = temp.path().join("disari");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&disari).unwrap();
        std::fs::write(disari.join("gizli.txt"), b"gizli").unwrap();
        // Windows'ta symlink olusturmak icin ayricalik gerekebilir; olmazsa atla.
        if std::os::windows::fs::symlink_file(disari.join("gizli.txt"), root.join("link.txt"))
            .is_err()
        {
            return;
        }
        assert_eq!(resolve_static_path(&root, "link.txt"), None);
    }
}
