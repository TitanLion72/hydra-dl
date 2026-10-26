//! Diskte kalici durum: indirme listesi + ayarlar (`downloads.json`).

use crate::model::{Download, Status};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Bos ise isletim sisteminin Indirilenler klasoru kullanilir.
    pub download_dir: PathBuf,
    /// Ayni anda calisan indirme sayisi.
    pub max_concurrent: usize,
    /// Dosya basina baglanti (segment) sayisi.
    pub connections: usize,
    /// 0 = sinirsiz.
    pub speed_limit_kbps: u64,
    pub capture_enabled: bool,
    pub capture_port: u16,
    /// Bos ise PATH / resources icinden bulunur.
    pub ytdlp_path: String,
    /// Uygulama acilirken yarim kalanlari kaldigi yerden surdur.
    pub auto_resume: bool,
    /// Eklenti her indirmede panel gostersin mi?
    pub ask_on_download: bool,
    /// Sayfadan oge secme (avci modu) varsayilan durumu.
    pub hunter_enabled: bool,
    /// Telefon/tablet icin web arayuzu.
    pub web_enabled: bool,
    pub web_port: u16,
    /// Web arayuzunun dinleyecegi adres (`0.0.0.0` tum arayuzler).
    pub web_bind: String,
    /// Bos degilse web arayuzu token ister.
    pub web_token: String,
    /// BitTorrent motoru.
    pub torrent_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: PathBuf::new(),
            max_concurrent: 3,
            connections: 8,
            speed_limit_kbps: 0,
            capture_enabled: true,
            capture_port: 6712,
            ytdlp_path: String::new(),
            auto_resume: true,
            ask_on_download: true,
            hunter_enabled: false,
            web_enabled: false,
            web_port: 6790,
            web_bind: "0.0.0.0".into(),
            web_token: String::new(),
            torrent_enabled: true,
        }
    }
}

impl Settings {
    /// Degerleri guvenli araliga ceker; kullanici JSON'u elle bozsa bile.
    pub fn normalize(&mut self) {
        self.max_concurrent = self.max_concurrent.clamp(1, 10);
        self.connections = self.connections.clamp(1, crate::model::MAX_CONNECTIONS);
        if self.capture_port < 1024 {
            self.capture_port = 6712;
        }
        if self.web_port < 1024 {
            self.web_port = 6790;
        }
        if self.web_bind.trim().is_empty() {
            self.web_bind = "0.0.0.0".into();
        }
        // Token yalniz guvenli karakterlerden olussun.
        self.web_token.retain(|c| c.is_ascii_alphanumeric());
        if self.web_token.len() > 64 {
            self.web_token.truncate(64);
        }
        // 10 GB/s ustu anlamsiz; yanlis girisi sinirsiz say.
        if self.speed_limit_kbps > 10_000_000 {
            self.speed_limit_kbps = 0;
        }
    }

    pub fn speed_limit_bytes(&self) -> u64 {
        self.speed_limit_kbps.saturating_mul(1024)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Persisted {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    items: Vec<Download>,
}

fn default_version() -> u32 {
    STATE_VERSION
}

#[derive(Debug)]
pub struct Store {
    pub items: Vec<Download>,
    pub settings: Settings,
    pub base_dir: PathBuf,
    state_path: PathBuf,
}

impl Store {
    pub fn load(base_dir: &Path) -> Self {
        let state_path = base_dir.join("downloads.json");
        let mut loaded: Persisted = std::fs::read_to_string(&state_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_else(|| Persisted {
                version: STATE_VERSION,
                settings: Settings::default(),
                items: Vec::new(),
            });

        loaded.settings.normalize();
        // Uygulama kapanirken calisanlar "downloading" kalmis olabilir.
        for item in &mut loaded.items {
            if item.status == Status::Downloading {
                item.status = Status::Queued;
                item.speed_bps = 0;
            }
        }

        let store = Self {
            items: loaded.items,
            settings: loaded.settings,
            base_dir: base_dir.to_path_buf(),
            state_path,
        };
        let _ = std::fs::create_dir_all(store.parts_root());
        store
    }

    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    pub fn parts_root(&self) -> PathBuf {
        self.base_dir.join("parts")
    }

    pub fn parts_dir(&self, id: &str) -> PathBuf {
        self.parts_root().join(id)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let payload = Persisted {
            version: STATE_VERSION,
            settings: self.settings.clone(),
            items: self.items.clone(),
        };
        let json = serde_json::to_string_pretty(&payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::create_dir_all(&self.base_dir)?;
        let tmp = self.state_path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        // Windows'ta std::fs::rename hedefi degistirir (MOVEFILE_REPLACE_EXISTING).
        std::fs::rename(&tmp, &self.state_path)
    }

    pub fn find(&self, id: &str) -> Option<&Download> {
        self.items.iter().find(|d| d.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut Download> {
        self.items.iter_mut().find(|d| d.id == id)
    }

    pub fn position(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|d| d.id == id)
    }

    /// Kuyruktan baslatilacaklari sec: oncelik yok, en eski once.
    /// Torrent'ler kendi motorunda calisir, kuyruga girmez.
    pub fn take_queued(&mut self, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }
        let mut ids: Vec<(i64, String)> = self
            .items
            .iter()
            .filter(|d| d.status == Status::Queued && d.kind != crate::model::Kind::Torrent)
            .map(|d| (d.created_at, d.id.clone()))
            .collect();
        ids.sort_by_key(|(created, _)| *created);
        let chosen: Vec<String> = ids.into_iter().take(limit).map(|(_, id)| id).collect();
        for id in &chosen {
            if let Some(item) = self.find_mut(id) {
                item.status = Status::Downloading;
                item.speed_bps = 0;
                item.error = None;
                item.finished_at = None;
            }
        }
        chosen
    }

    pub fn running_count(&self) -> usize {
        self.items.iter().filter(|d| d.status.is_running()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;

    fn sample(id: &str) -> Download {
        Download {
            id: id.into(),
            url: "https://example.com/f.bin".into(),
            filename: "f.bin".into(),
            dir: PathBuf::from("C:/tmp"),
            kind: Kind::Http,
            status: Status::Queued,
            total_bytes: Some(10),
            downloaded_bytes: 0,
            segments: vec![],
            resumable: true,
            speed_bps: 0,
            progress: None,
            error: None,
            created_at: 0,
            finished_at: None,
            connections: 4,
            referer: None,
            mime: None,
            file_path: None,
            format: None,
            swarm: None,
            source: None,
        }
    }

    #[test]
    fn ayarlar_kaydedilip_geri_yuklenir() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::load(dir.path());
        store.settings.max_concurrent = 5;
        store.settings.download_dir = dir.path().join("indirilenler");
        store.items.push(sample("a"));
        store.save().unwrap();

        let reloaded = Store::load(dir.path());
        assert_eq!(reloaded.settings.max_concurrent, 5);
        assert_eq!(reloaded.items.len(), 1);
        assert_eq!(reloaded.items[0].filename, "f.bin");
    }

    #[test]
    fn yarim_kalan_downloading_kuyruga_doner() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::load(dir.path());
        let mut item = sample("a");
        item.status = Status::Downloading;
        store.items.push(item);
        store.save().unwrap();

        let reloaded = Store::load(dir.path());
        assert_eq!(reloaded.items[0].status, Status::Queued);
    }

    #[test]
    fn bozuk_json_cokmez() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("downloads.json"), "{bozuk").unwrap();
        let store = Store::load(dir.path());
        assert!(store.items.is_empty());
        assert_eq!(store.settings.max_concurrent, 3);
    }

    #[test]
    fn ayarlar_sinirlanir() {
        let mut s = Settings {
            max_concurrent: 99,
            connections: 0,
            speed_limit_kbps: 999_999_999,
            capture_port: 80,
            ..Default::default()
        };
        s.normalize();
        assert_eq!(s.max_concurrent, 10);
        assert_eq!(s.connections, 1);
        assert_eq!(s.speed_limit_kbps, 0);
        assert_eq!(s.capture_port, 6712);
    }

    #[test]
    fn kuyruktan_eski_once_seciliyor() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::load(dir.path());
        let mut a = sample("a");
        a.created_at = 200;
        let mut b = sample("b");
        b.created_at = 100;
        let mut c = sample("c");
        c.created_at = 50;
        c.status = Status::Paused;
        store.items = vec![a, b, c];

        let chosen = store.take_queued(1);
        assert_eq!(chosen, vec!["b".to_string()]);
        assert_eq!(store.find("b").unwrap().status, Status::Downloading);
        assert_eq!(store.running_count(), 1);
        // a hala kuyrukta; bir sonraki secimde o gelir.
        assert_eq!(store.take_queued(5), vec!["a".to_string()]);
        assert_eq!(store.running_count(), 2);
        assert_eq!(store.take_queued(5), Vec::<String>::new());
    }
}
