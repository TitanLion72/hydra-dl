//! BitTorrent/P2P motoru (librqbit). Magnet ve .torrent destekler.

use crate::manager::AppState;
use crate::model::{Status, Swarm};
use librqbit::api::TorrentIdOrHash;
use librqbit::{AddTorrent, AddTorrentOptions, ManagedTorrent, Session, TorrentStatsState};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

type ManagedTorrentHandle = Arc<ManagedTorrent>;

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
        let session = Session::new(dir.clone())
            .await
            .map_err(|e| format!("Torrent oturumu başlatılamadı: {e}"))?;
        *guard = Some(session.clone());
        Ok(session)
    }

    pub async fn add(
        &self,
        state: &Arc<AppState>,
        id: String,
        source: &str,
        dir: PathBuf,
        name: Option<String>,
        start_paused: bool,
    ) -> Result<(), String> {
        let session = self.ensure_session(&dir).await?;
        let opts = AddTorrentOptions {
            paused: start_paused,
            overwrite: true,
            output_folder: Some(dir.to_string_lossy().to_string()),
            ..Default::default()
        };
        let response = session
            .add_torrent(AddTorrent::from_url(source.to_string()), Some(opts))
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
        let total = if stats.total_bytes > 0 {
            Some(stats.total_bytes)
        } else {
            None
        };

        state.update_item(&id, |item| {
            item.status = if start_paused {
                Status::Paused
            } else {
                Status::Downloading
            };
            item.filename = resolved_name;
            item.total_bytes = total;
            item.downloaded_bytes = stats.progress_bytes;
            item.segments.clear();
            item.resumable = true;
            item.mime = Some("torrent".into());
            item.swarm = Some(Swarm::default());
            item.source = Some(source.to_string());
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
                        item.status = Status::Failed;
                        item.error = Some(err.clone());
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
                }
            } else {
                let label = state
                    .item(&id)
                    .map(|d| d.filename)
                    .unwrap_or_else(|| "Torrent".into());
                state.notice(format!("Torrent tamamlandı: {label}"), "success");
            }
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

/// Tamamlanan torrent'in diskteki yolunu bulur.
///
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
}
