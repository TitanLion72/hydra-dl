//! Tray (sistem tepsisi) simgesi ve menusu.
//!
//! Davranis:
//! - Sol tik: pencereyi goster/one getir.
//! - Menu: Goster/Gizle, Tumunu duraklat/devam ettir, Cikis.
//! - Pencere kapatilirsa uygulama tepside yasamaya devam eder; **Cikis** menusu
//!   gercekten kapatir (durum kaydedilir).
//!
//! Tooltip her ticker turunda guncellenir: "Hydra DL — 2 indirme · 5,4 MB/s".

use crate::manager::AppState;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// Tray'in hala yasadigini ve "cikis" istendigini tutan isaret.
pub struct TrayState {
    /// Kullanici "Cikis" dedi mi? (pencere kapatma = tepside kal)
    pub quitting: std::sync::atomic::AtomicBool,
    pub handle: std::sync::Mutex<Option<tauri::tray::TrayIcon>>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            quitting: std::sync::atomic::AtomicBool::new(false),
            handle: std::sync::Mutex::new(None),
        }
    }
}

impl TrayState {
    pub fn is_quitting(&self) -> bool {
        self.quitting.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Tray simgesi kuruldu mu? (`/health` bunu raporlar.)
    pub fn is_ready(&self) -> bool {
        self.handle.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    fn begin_quit(&self) {
        self.quitting
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Tooltip metni: aktif indirme sayisi + toplam hiz.
pub fn tooltip(app_name: &str, active: usize, speed_bps: u64) -> String {
    if active == 0 {
        return format!("{app_name} — boşta");
    }
    let per_sec = speed_bps as f64;
    let (value, unit) = if per_sec >= 1024.0 * 1024.0 {
        (per_sec / (1024.0 * 1024.0), "MB/s")
    } else if per_sec >= 1024.0 {
        (per_sec / 1024.0, "KB/s")
    } else {
        (per_sec, "B/s")
    };
    format!("{app_name} — {active} indirme · {value:.1} {unit}")
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Tum indirmeleri duraklat/devam ettir (menu eylemi).
fn toggle_all(app: &AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let state = state.inner().clone();
    let items = state.snapshot();
    let any_running = items.iter().any(|d| d.status.is_running());
    tauri::async_runtime::spawn(async move {
        for item in items {
            if any_running {
                if item.status.is_running() {
                    let _ = crate::manager::pause(&state, &item.id).await;
                }
            } else if matches!(item.status, crate::model::Status::Paused) {
                let _ = crate::manager::resume(&state, &item.id).await;
            }
        }
    });
}

/// Tray simgesini ve menusunu kurar.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Göster", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Gizle", true, None::<&str>)?;
    let toggle_all_item = MenuItem::with_id(
        app,
        "pause_all",
        "Tümünü duraklat / devam ettir",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &hide,
            &PredefinedMenuItem::separator(app)?,
            &toggle_all_item,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("tray ikonu yok".into()))?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Hydra DL")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "pause_all" => toggle_all(app),
            "quit" => {
                if let Some(tray_state) = app.try_state::<TrayState>() {
                    tray_state.begin_quit();
                }
                if let Some(state) = app.try_state::<Arc<AppState>>() {
                    state.inner().persist();
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Sol tik: pencereyi goster.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    if let Some(tray_state) = app.try_state::<TrayState>() {
        *tray_state.handle.lock().unwrap() = Some(tray);
    }
    Ok(())
}

/// Tooltip'i gunceller (ticker turunda cagrilir).
pub fn update_tooltip(app: &AppHandle, tooltip_text: &str) {
    if let Some(tray_state) = app.try_state::<TrayState>() {
        let guard = tray_state.handle.lock().unwrap();
        if let Some(tray) = guard.as_ref() {
            let _ = tray.set_tooltip(Some(tooltip_text));
        }
    }
}

/// Durumdan tooltip metni uretir (saf; test edilebilir).
pub fn tooltip_from_state(app_name: &str, items: &[crate::model::Download]) -> String {
    let active = items.iter().filter(|d| d.status.is_running()).count();
    let speed: u64 = items.iter().map(|d| d.speed_bps).sum();
    tooltip(app_name, active, speed)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Download, Kind, Status};
    use std::path::PathBuf;

    fn item(status: Status, speed: u64) -> Download {
        Download {
            id: "x".into(),
            url: "https://x/y".into(),
            filename: "f.bin".into(),
            dir: PathBuf::from("C:/tmp"),
            kind: Kind::Http,
            status,
            total_bytes: Some(100),
            downloaded_bytes: 0,
            segments: vec![],
            resumable: true,
            speed_bps: speed,
            progress: None,
            error: None,
            created_at: 0,
            finished_at: None,
            connections: 1,
            referer: None,
            cookie: None,
            mime: None,
            file_path: None,
            format: None,
            swarm: None,
            source: None,
            torrent_files: None,
            retry: None,
        }
    }

    #[test]
    fn tooltip_bosta_ve_aktifken_dogru() {
        assert_eq!(tooltip("Hydra DL", 0, 0), "Hydra DL — boşta");
        assert_eq!(
            tooltip("Hydra DL", 2, 5 * 1024 * 1024),
            "Hydra DL — 2 indirme · 5.0 MB/s"
        );
        assert_eq!(
            tooltip("Hydra DL", 1, 1536 * 1024),
            "Hydra DL — 1 indirme · 1.5 MB/s"
        );
        assert_eq!(
            tooltip("Hydra DL", 1, 2048),
            "Hydra DL — 1 indirme · 2.0 KB/s"
        );
    }

    #[test]
    fn tooltip_durumdan_uretilir() {
        let items = vec![
            item(Status::Downloading, 1024 * 1024),
            item(Status::Downloading, 1024 * 1024),
            item(Status::Paused, 0),
            item(Status::Completed, 0),
        ];
        assert_eq!(
            tooltip_from_state("Hydra DL", &items),
            "Hydra DL — 2 indirme · 2.0 MB/s"
        );
        let idle = vec![item(Status::Paused, 0), item(Status::Completed, 0)];
        assert_eq!(tooltip_from_state("Hydra DL", &idle), "Hydra DL — boşta");
    }
}
