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
use std::path::PathBuf;
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
    match crate::extract::probe(state.http(), ytdlp.as_deref(), &url, media).await {
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
fn webapp_dir(state: &AppState) -> PathBuf {
    if let Ok(resource) = state.app.path().resource_dir() {
        let candidate = resource.join("webapp");
        if candidate.join("index.html").is_file() {
            return candidate;
        }
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    for base in [cwd.clone(), cwd.join("..")] {
        let candidate = base.join("webapp");
        if candidate.join("index.html").is_file() {
            return candidate;
        }
    }
    cwd.join("webapp")
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

fn serve_static(state: &AppState, requested: &str) -> Response {
    let trimmed = requested.trim_start_matches('/');
    let root = webapp_dir(state);
    // Dizin disina cikma girisimlerini engelle.
    if trimmed.contains("..") || trimmed.contains('\\') {
        return error_response(StatusCode::BAD_REQUEST, "geçersiz yol");
    }
    let mut candidate = root.join(if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    });
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
        .route("/torrents", post(add_torrent))
        .route("/ytdlp", get(ytdlp_status))
        .route("/ytdlp/install", post(install_ytdlp))
        .route("/assets/{*path}", get(static_file))
        .route("/{*path}", get(static_file))
        .with_state(state)
}
// ---------------------------------------------------------------------------
// Sunucu yasam dongusu
// ---------------------------------------------------------------------------

fn auth_middleware(
    token: String,
) -> impl Fn(
    Request<Body>,
    Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
       + Clone {
    move |request: Request<Body>, next: Next| {
        let token = token.clone();
        Box::pin(async move {
            // Statik dosyalar ve saglik kontrolu serbest; API token ister.
            let path = request.uri().path().to_string();
            let check = path.starts_with("/downloads")
                || path.starts_with("/settings")
                || path.starts_with("/probe")
                || path.starts_with("/add")
                || path.starts_with("/torrents")
                || path.starts_with("/ytdlp")
                || path.starts_with("/events");
            if !check {
                return next.run(request).await;
            }
            if token.is_empty() {
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

    let mut router = api_router(state).layer(middleware::from_fn(cors));
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        router = router.layer(middleware::from_fn(auth_middleware(token)));
    }

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
    fn lan_adresi_bos_olmaz() {
        // Ag yoksa bos olabilir; sadece paniklemedigini dogrula.
        let _ = lan_addresses();
    }
}
