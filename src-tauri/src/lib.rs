mod capture;
mod commands;
mod engine;
mod extract;
mod manager;
mod model;
mod store;
mod torrent;
mod util;
mod web;
mod ytdlp;

use manager::AppState;
use std::sync::Arc;
use store::Store;
use tauri::{Manager, WindowEvent};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data)?;
            let mut store = Store::load(&app_data);

            if store.settings.download_dir.as_os_str().is_empty() {
                if let Ok(downloads) = app.path().download_dir() {
                    store.settings.download_dir = downloads;
                }
            }

            let state = AppState::new(app.handle().clone(), store);
            app.manage(state.clone());

            manager::start_ticker(&state);
            manager::resume_pending(&state);
            manager::resume_torrents(&state);

            let settings = state.settings();
            if settings.capture_enabled {
                let capture_state = state.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) =
                        capture::restart(&capture_state, true, settings.capture_port).await
                    {
                        eprintln!("[hydra] {err}");
                        capture_state.notice(err, "error");
                    }
                });
            }
            if settings.web_enabled {
                let web_state = state.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = web::restart_from_settings(&web_state).await {
                        eprintln!("[hydra] {err}");
                        web_state.notice(err, "error");
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Pencere kapanirken torrent oturumunu duzgun kapat (DHT/peer'lar).
            if matches!(event, WindowEvent::Destroyed) {
                if let Some(state) = window.try_state::<Arc<AppState>>() {
                    let state = state.inner().clone();
                    state.persist();
                    tauri::async_runtime::spawn(async move {
                        state.torrent.shutdown().await;
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_downloads,
            commands::add_download,
            commands::list_torrent_files,
            commands::set_torrent_files,
            commands::test_notification,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::remove_download,
            commands::clear_finished,
            commands::get_settings,
            commands::save_settings,
            commands::get_app_info,
            commands::get_ytdlp_status,
            commands::get_capture_status,
            commands::open_download,
            commands::reveal_download,
            commands::probe_url,
            commands::get_web_status,
            commands::install_ytdlp,
        ])
        .run(tauri::generate_context!())
        .expect("Hydra DL baslatilamadi");
}
