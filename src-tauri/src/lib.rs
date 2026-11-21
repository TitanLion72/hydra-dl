mod capture;
mod commands;
mod dash;
mod engine;
mod extract;
mod manager;
mod model;
mod store;
mod tls;
mod torrent;
mod tray;
mod util;
mod web;
mod ytdlp;

use manager::AppState;
use std::sync::Arc;
use store::Store;
use tauri::{Manager, WindowEvent};

/// Kapanış/kurulum öncesi ortak temizlik.
///
/// Pencere kapanışı ve güncelleme kurulumu **aynı** yolu kullanır: çalışan
/// indirmeler duraklatılır (iptal değil — parçalar diskte kalır), durum
/// kaydedilir, torrent oturumu kapatılır. Böylece güncelleme yolu bu adımı
/// atlayamaz ve yeni sürüm açılışta kaldığı yerden devam eder.
pub async fn shutdown_state(state: Arc<AppState>) {
    manager::pause_all_for_shutdown(&state).await;
    // Duraklatma turu kendi kaydını yapar; burada yine kaydediyoruz ki
    // ileride tur kısalırsa "kapanışta kaydet" garantisi aynen kalsın.
    state.persist();
    state.torrent.shutdown().await;
}

pub fn run() {
    // rustls saglayicisini en basta kur: TLS kullanimi herhangi bir noktadan
    // baslarsa (web sunucusu acilista) varsayilan hazir olsun. Kurulmazsa rustls
    // panic atar ve `panic = "abort"` yuzunden surec tamamen olur.
    tls::install_crypto_provider();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            app.manage(tray::TrayState::default());

            // Tray simgesi: pencere kapansa bile indirmeler surer.
            if let Err(err) = tray::setup(app.handle()) {
                eprintln!("[hydra] tray kurulamadi: {err}");
            }

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
            // Pencere kapatildi: varsayilan olarak tepsiye kucult (indirmeler surer).
            // "Cikis" tray menusunden yapilir; o zaman gercekten kapanir.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let quitting = window
                    .try_state::<tray::TrayState>()
                    .map(|t| t.is_quitting())
                    .unwrap_or(false);
                if !quitting {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            // Pencere kapanirken indirmeleri duraklat, durumu kaydet ve torrent
            // oturumunu duzgun kapat (DHT/peer'lar). Guncelleme yolu da ayni
            // `shutdown_state` fonksiyonunu kullanir.
            if matches!(event, WindowEvent::Destroyed) {
                if let Some(state) = window.try_state::<Arc<AppState>>() {
                    tauri::async_runtime::spawn(shutdown_state(state.inner().clone()));
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
            commands::pause_all_for_shutdown,
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
            commands::check_ytdlp_update,
            commands::export_settings_to,
            commands::import_settings_from,
        ])
        .run(tauri::generate_context!())
        .expect("Hydra DL baslatilamadi");
}
