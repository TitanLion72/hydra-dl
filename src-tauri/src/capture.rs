//! Yakalama sunucusu: tarayici eklentisi icin `127.0.0.1:<port>`.
//!
//! Ayni API'yi web arayuzuyle paylasir (`web::api_router`); fark yalnizca
//! dinleme adresi ve token zorunlulugudur.

use crate::manager::AppState;
use crate::web;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct CaptureHandle {
    pub port: u16,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        self.task.abort();
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub url: Option<String>,
}

pub async fn start(state: Arc<AppState>, port: u16) -> Result<CaptureHandle, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Yakalama sunucusu {port} portuna bağlanamadı: {e}"))?;
    let actual_port = listener
        .local_addr()
        .map_err(|e| format!("Adres okunamadı: {e}"))?
        .port();

    let router = web::api_router(state);
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, router).with_graceful_shutdown(async {
            let _ = rx.await;
        });
        if let Err(err) = server.await {
            eprintln!("[hydra] yakalama sunucusu durdu: {err}");
        }
    });

    Ok(CaptureHandle {
        port: actual_port,
        shutdown: Some(tx),
        task,
    })
}

/// Sunucuyu kapatip (gerekirse) yeni portla yeniden baslatir.
pub async fn restart(state: &Arc<AppState>, enabled: bool, port: u16) -> Result<(), String> {
    let previous = state.capture.lock().unwrap().take();
    drop(previous);
    if !enabled {
        return Ok(());
    }
    let handle = start(state.clone(), port).await?;
    *state.capture.lock().unwrap() = Some(handle);
    Ok(())
}

pub fn status(state: &AppState) -> CaptureStatus {
    let guard = state.capture.lock().unwrap();
    let settings = state.settings();
    let running = guard.is_some();
    let port = guard
        .as_ref()
        .map(|h| h.port)
        .unwrap_or(settings.capture_port);
    CaptureStatus {
        enabled: settings.capture_enabled,
        running,
        port,
        url: running.then(|| format!("http://127.0.0.1:{port}")),
    }
}
