//! Tauri komutlari: arayuzun cagirdigi tum islemler.

use crate::capture::{self, CaptureStatus};
use crate::extract::ProbeResult;
use crate::manager::{self, AddOptions, AppState};
use crate::model::Download;
use crate::store::Settings;
use crate::web::{self, WebStatus};
use crate::ytdlp::{self, YtdlpStatus};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

type St<'a> = State<'a, Arc<AppState>>;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub download_dir: String,
    pub state_path: String,
    pub capture: CaptureStatus,
    pub web: WebStatus,
    pub web_qr: Option<String>,
    pub ytdlp: YtdlpStatus,
    pub uptime_secs: u64,
}

#[tauri::command]
pub fn list_downloads(state: St<'_>) -> Vec<Download> {
    state.snapshot()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri komut yuzeyi: her alan ayri arguman.
pub async fn add_download(
    state: St<'_>,
    url: String,
    dir: Option<String>,
    filename: Option<String>,
    media: Option<bool>,
    referer: Option<String>,
    format: Option<String>,
    files: Option<Vec<usize>>,
) -> Result<Download, String> {
    let options = AddOptions {
        dir: dir.filter(|d| !d.trim().is_empty()).map(PathBuf::from),
        filename: filename.filter(|f| !f.trim().is_empty()),
        referer,
        media,
        format: format.filter(|f| !f.trim().is_empty()),
        files,
    };
    manager::add(state.inner(), &url, options).await
}

/// Torrent dosya listesini getirir (indirme baslatmaz).
#[tauri::command]
pub async fn list_torrent_files(
    state: St<'_>,
    source: String,
) -> Result<crate::torrent::TorrentListing, String> {
    let settings = state.settings();
    if !settings.torrent_enabled {
        return Err("Torrent motoru kapalı. Ayarlardan açabilirsiniz.".into());
    }
    state
        .torrent
        .list(state.http(), &source, &settings.download_dir)
        .await
}

/// Eklenmis torrent'in indirilecek dosyalarini degistirir.
#[tauri::command]
pub async fn set_torrent_files(
    state: St<'_>,
    id: String,
    files: Vec<usize>,
) -> Result<(), String> {
    state.torrent.set_files(state.inner(), &id, files).await
}

/// Ayarlardaki "Test bildirimi": ayar kapali olsa da gonderir.
#[tauri::command]
pub fn test_notification(state: St<'_>) -> Result<(), String> {
    state.send_notification("Hydra DL", "Bildirimler çalışıyor.")
}

#[tauri::command]
pub async fn probe_url(
    state: St<'_>,
    url: String,
    media: Option<bool>,
) -> Result<ProbeResult, String> {
    let settings = state.settings();
    let ytdlp = ytdlp::resolve(&state.app, &settings.ytdlp_path);
    crate::extract::probe(state.http(), ytdlp.as_deref(), &url, media).await
}

#[tauri::command]
pub async fn pause_download(state: St<'_>, id: String) -> Result<(), String> {
    manager::pause(state.inner(), &id).await
}

#[tauri::command]
pub async fn resume_download(state: St<'_>, id: String) -> Result<(), String> {
    manager::resume(state.inner(), &id).await
}

#[tauri::command]
pub async fn cancel_download(state: St<'_>, id: String) -> Result<(), String> {
    manager::cancel(state.inner(), &id).await
}

#[tauri::command]
pub async fn remove_download(
    state: St<'_>,
    id: String,
    delete_file: Option<bool>,
) -> Result<(), String> {
    manager::remove(state.inner(), &id, delete_file.unwrap_or(false)).await
}

#[tauri::command]
pub fn clear_finished(state: St<'_>, delete_files: Option<bool>) {
    manager::clear_finished(state.inner(), delete_files.unwrap_or(false));
}

#[tauri::command]
pub fn get_settings(state: St<'_>) -> Settings {
    state.settings()
}

#[tauri::command]
pub async fn save_settings(state: St<'_>, settings: Settings) -> Result<Settings, String> {
    let previous = state.settings();
    let saved = manager::save_settings(state.inner(), settings)?;
    if previous.capture_enabled != saved.capture_enabled
        || previous.capture_port != saved.capture_port
    {
        capture::restart(state.inner(), saved.capture_enabled, saved.capture_port).await?;
    }
    if previous.web_enabled != saved.web_enabled
        || previous.web_port != saved.web_port
        || previous.web_bind != saved.web_bind
        || previous.web_token != saved.web_token
    {
        web::restart_web(state.inner(), &saved).await?;
    }
    Ok(saved)
}

#[tauri::command]
pub async fn get_app_info(state: St<'_>) -> Result<AppInfo, String> {
    let settings = state.settings();
    let state_path = {
        let store = state.store.lock().unwrap();
        store.state_path().to_string_lossy().to_string()
    };
    let ytdlp = ytdlp::status(&state.app, &settings.ytdlp_path).await;
    let web_status = web::status(state.inner());
    let web_qr = web_status
        .url
        .as_ref()
        .map(|url| {
            if settings.web_token.is_empty() {
                url.clone()
            } else {
                format!("{url}?token={}", settings.web_token)
            }
        })
        .and_then(|url| web::qr_svg_data_uri(&url).ok());
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        download_dir: settings.download_dir.to_string_lossy().to_string(),
        state_path,
        capture: capture::status(state.inner()),
        web: web_status,
        web_qr,
        ytdlp,
        uptime_secs: state.uptime_secs(),
    })
}

#[tauri::command]
pub fn get_ytdlp_status(state: St<'_>) -> Result<YtdlpStatus, String> {
    let settings = state.settings();
    let path = ytdlp::resolve(&state.app, &settings.ytdlp_path);
    Ok(YtdlpStatus {
        available: path.is_some(),
        path: path.map(|p| p.to_string_lossy().to_string()),
        version: None,
    })
}

/// yt-dlp'yi indirip uygulama klasörüne kurar. İlerleme `ytdlp:install` olayıyla yayınlanır.
#[tauri::command]
pub async fn install_ytdlp(state: St<'_>) -> Result<YtdlpStatus, String> {
    use tauri::Emitter;

    let app = state.app.clone();
    let client = state.http().clone();
    let emitted_app = app.clone();
    let result = ytdlp::install(&app, &client, move |progress| {
        let _ = emitted_app.emit("ytdlp:install", progress);
    })
    .await;

    match result {
        Ok(path) => {
            let version = ytdlp::status(&app, "").await.version;
            Ok(YtdlpStatus {
                available: true,
                path: Some(path.to_string_lossy().to_string()),
                version,
            })
        }
        Err(err) => {
            let _ = app.emit(
                "ytdlp:install",
                ytdlp::InstallProgress {
                    stage: "failed".into(),
                    downloaded: 0,
                    total: None,
                    error: Some(err.clone()),
                },
            );
            Err(err)
        }
    }
}

#[tauri::command]
pub fn get_capture_status(state: St<'_>) -> CaptureStatus {
    capture::status(state.inner())
}

#[tauri::command]
pub fn get_web_status(state: St<'_>) -> WebStatus {
    web::status(state.inner())
}

fn download_path(state: &AppState, id: &str) -> Result<PathBuf, String> {
    let item = state.item(id).ok_or("Indirme bulunamadi")?;
    Ok(item
        .file_path
        .clone()
        .unwrap_or_else(|| item.dir.join(&item.filename)))
}

#[tauri::command]
pub fn open_download(state: St<'_>, id: String) -> Result<(), String> {
    let path = download_path(state.inner(), &id)?;
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_download(state: St<'_>, id: String) -> Result<(), String> {
    let item = state.item(&id).ok_or("Indirme bulunamadi")?;
    let path = item
        .file_path
        .clone()
        .filter(|p| p.exists())
        .unwrap_or_else(|| item.dir.clone());
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}
