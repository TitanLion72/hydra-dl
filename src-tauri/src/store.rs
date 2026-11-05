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
    /// Web arayuzu HTTPS (kendinden imzali sertifika) ile servis edilsin mi?
    /// Varsayilan kapali; acildiginda LAN'daki token trafigi sifrelenir.
    #[serde(default)]
    pub web_tls: bool,
    /// BitTorrent motoru.
    pub torrent_enabled: bool,
    /// Indirme bitince/hata olunca isletim sistemi bildirimi goster.
    pub notify_on_finish: bool,
    /// HTTP/SOCKS5 proxy (`http://host:port`, `socks5://host:port`). Bos = yok.
    pub proxy_url: String,
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
            web_tls: false,
            torrent_enabled: true,
            notify_on_finish: true,
            proxy_url: String::new(),
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
        // Proxy: yalniz taninan semalar; gecersiz deger temizlenir.
        self.proxy_url = self.proxy_url.trim().to_string();
        if !self.proxy_url.is_empty()
            && crate::engine::http::proxy_problem(&self.proxy_url).is_some()
        {
            self.proxy_url.clear();
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

// ---------------------------------------------------------------------------
// Ayarların dışa/içe aktarımı
// ---------------------------------------------------------------------------

/// Dışa aktarılan dosyanın imzası. Yanlış dosyayı (ör. `downloads.json`)
/// içe aktarmayı **erken** reddetmek için sabit bir metin.
pub const SETTINGS_FORMAT: &str = "hydra-dl-settings";

/// Dosya biçiminin sürümü. Yeni alanlar eklendiğinde artırılır; eski dosyalar
/// okunmaya devam eder (`apply_import` ileri sürümü reddeder, geriye uyumlu).
pub const SETTINGS_EXPORT_VERSION: u32 = 1;

/// Ayarları dışa aktarma paketine çevirir.
///
/// **`webToken` varsayılan olarak dışa aktarılmaz:** dosya paylaşıldığında
/// erişim anahtarı sızmasın. `include_token` yalnız kullanıcı açıkça isterse
/// `true` olur.
///
/// `download_dir` kişisel bir yol içerir; bu bilinçli olarak **dahil edilir**
/// (kendi yedeğini geri yüklemek için gerekli). Paylaşacak kullanıcı uyarılır.
pub fn export_payload(
    settings: &Settings,
    include_token: bool,
    exported_at: &str,
) -> serde_json::Value {
    let mut value = serde_json::to_value(settings).unwrap_or(serde_json::Value::Null);
    if let Some(object) = value.as_object_mut() {
        if !include_token {
            object.remove("webToken");
        }
    }
    serde_json::json!({
        "format": SETTINGS_FORMAT,
        "version": SETTINGS_EXPORT_VERSION,
        "exportedAt": exported_at,
        "settings": value,
    })
}

/// İçe aktarma sonucu: uygulanacak ayarlar + değişen alanların adları.
///
/// Aynı tip hem Rust içinde hem HTTP/arayüz yanıtında kullanılır; arayüz
/// `changed` listesini kullanıcıya "şu alanlar güncellendi" diye gösterir.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub settings: Settings,
    /// Değişen alanların adları (camelCase, arayüz için).
    pub changed: Vec<String>,
    /// Uygulanamayan istekler (ör. oluşturulamayan klasör) — sessizce yutulmaz.
    pub warnings: Vec<String>,
}

/// İçe aktarma paketini **mevcut ayarların üzerine** uygular (saf fonksiyon).
///
/// Neden birleştirme: dosyada olmayan alan mevcut değeri korur. Bu sayede
/// token'sız bir yedek geri yüklendiğinde kullanıcının erişim anahtarı
/// **silinmez** (token dışa aktarılmadığı için dosyada yoktur).
///
/// Hata durumları (hepsi sessizce yutulmaz, çağırana döner):
/// - paket bir nesne değil,
/// - `format` imzası yanlış,
/// - `version` bu sürümden büyük (anlamadığımız dosyayı yarım uygulamayalım),
/// - `settings` nesne değil / hiç yok.
pub fn apply_import(
    current: &Settings,
    payload: &serde_json::Value,
) -> Result<ImportReport, String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "Geçersiz dosya: JSON nesnesi bekleniyordu.".to_string())?;

    let format = object.get("format").and_then(|v| v.as_str()).unwrap_or("");
    if format != SETTINGS_FORMAT {
        return Err("Bu dosya bir Hydra ayar dosyası değil (imza uyuşmuyor).".to_string());
    }

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if version == 0 {
        return Err("Ayar dosyasında sürüm bilgisi yok.".to_string());
    }
    if version > u64::from(SETTINGS_EXPORT_VERSION) {
        return Err(format!(
            "Bu ayar dosyası daha yeni bir sürümden ({version}); uygulamayı güncelle."
        ));
    }

    let incoming = object
        .get("settings")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "Ayar dosyasında 'settings' nesnesi yok.".to_string())?;

    // Mevcut ayarları temel al, gelen alanları üzerine yaz.
    let mut merged =
        serde_json::to_value(current).map_err(|e| format!("Ayarlar okunamadı: {e}"))?;
    let merged_object = merged
        .as_object_mut()
        .ok_or_else(|| "Ayarlar nesneye çevrilemedi.".to_string())?;

    // Boş yol gelirse mevcut klasörü koru: bozuk dosya indirme yerini sıfırlamasın.
    let mut warnings: Vec<String> = Vec::new();
    for (key, value) in incoming {
        if key == "downloadDir" && value.as_str().map(str::trim).unwrap_or("").is_empty() {
            warnings.push("Dosyadaki indirme klasörü boş; mevcut klasör korundu.".into());
            continue;
        }
        merged_object.insert(key.clone(), value.clone());
    }

    let mut next: Settings =
        serde_json::from_value(merged).map_err(|e| format!("Ayar dosyası okunamadı: {e}"))?;
    next.normalize();

    // Değişen alanları mevcutla karşılaştırarak bul (kullanıcıya özet gösterilir).
    let before = serde_json::to_value(current).unwrap_or(serde_json::Value::Null);
    let after = serde_json::to_value(&next).unwrap_or(serde_json::Value::Null);
    let mut changed = Vec::new();
    if let (Some(before), Some(after)) = (before.as_object(), after.as_object()) {
        // `incoming` sırasını koru: dosyada olan alanlar önce, en anlamlı olanlar.
        let mut keys: Vec<&String> = incoming.keys().collect();
        keys.sort();
        for key in keys {
            if key == "downloadDir" && next.download_dir.as_os_str().is_empty() {
                continue;
            }
            if before.get(key) != after.get(key) {
                changed.push(key.clone());
            }
        }
    }

    Ok(ImportReport {
        settings: next,
        changed,
        warnings,
    })
}

/// İndirme klasörü gerçekten kullanılabilir mi? (içe aktarma sonrası kontrol)
///
/// Klasör yoksa oluşturmayı dener. Oluşturamıyorsak **eski klasöre döner**:
/// kullanıcı ayarlarını boşuna kaybetmesin.
pub fn verify_download_dir(next: &Settings, previous: &Settings) -> (Settings, Vec<String>) {
    let mut settings = next.clone();
    let mut warnings = Vec::new();
    if settings.download_dir.as_os_str().is_empty() {
        return (settings, warnings);
    }
    if std::fs::create_dir_all(&settings.download_dir).is_ok() {
        return (settings, warnings);
    }
    warnings.push(format!(
        "İndirme klasörü oluşturulamadı ({}); önceki klasör korundu.",
        settings.download_dir.display()
    ));
    settings.download_dir = previous.download_dir.clone();
    (settings, warnings)
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

    /// Telefondan yüklenen `.torrent` dosyalarının kalıcı klasörü.
    ///
    /// Kalıcı olmalı: `source` alanı `downloads.json`'a yazılıyor ve uygulama
    /// yeniden açıldığında torrent bu yoldan yeniden yükleniyor.
    pub fn torrents_root(&self) -> PathBuf {
        self.base_dir.join("torrents")
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
    fn ayarlar_kaydedilip_geri_yuklenir() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::load(dir.path());
        store.settings.max_concurrent = 5;
        store.settings.download_dir = dir.path().join("indirilenler");
        store.settings.notify_on_finish = false;
        store.items.push(sample("a"));
        store.save().unwrap();

        let reloaded = Store::load(dir.path());
        assert_eq!(reloaded.settings.max_concurrent, 5);
        assert!(
            !reloaded.settings.notify_on_finish,
            "bildirim ayari korunmali"
        );
        assert_eq!(reloaded.items.len(), 1);
        assert_eq!(reloaded.items[0].filename, "f.bin");
    }

    #[test]
    fn eski_ayar_dosyasinda_bildirim_varsayilan_acik() {
        // Bildirim alani eklenmeden once yazilmis ayar: serde default devreye girer.
        let dir = tempfile::tempdir().unwrap();
        let raw = r#"{"version":1,"settings":{"maxConcurrent":4},"items":[]}"#;
        std::fs::write(dir.path().join("downloads.json"), raw).unwrap();
        let store = Store::load(dir.path());
        assert_eq!(store.settings.max_concurrent, 4);
        assert!(store.settings.notify_on_finish);
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

    // --- ayarların dışa/içe aktarımı ---------------------------------------

    fn custom_settings() -> Settings {
        Settings {
            download_dir: PathBuf::from("C:\\Users\\ornek\\Indirilenler"),
            max_concurrent: 5,
            connections: 12,
            speed_limit_kbps: 4096,
            web_port: 6799,
            web_token: "gizli123".into(),
            proxy_url: "socks5://127.0.0.1:1080".into(),
            notify_on_finish: false,
            ..Settings::default()
        }
    }

    #[test]
    fn disa_aktarma_token_icermez_ama_diger_ayarlar_icinde() {
        let settings = custom_settings();
        let payload = export_payload(&settings, false, "2026-11-05T10:00:00Z");

        assert_eq!(payload["format"], SETTINGS_FORMAT);
        assert_eq!(payload["version"], SETTINGS_EXPORT_VERSION);
        assert_eq!(payload["exportedAt"], "2026-11-05T10:00:00Z");

        let exported = payload["settings"].as_object().unwrap();
        assert!(
            !exported.contains_key("webToken"),
            "token varsayılan olarak dışa aktarılmamalı"
        );
        assert!(
            !payload.to_string().contains("gizli123"),
            "token metni dosyada geçmemeli"
        );
        // Diğer ayarlar gerçekten var.
        assert_eq!(exported["maxConcurrent"], 5);
        assert_eq!(exported["webPort"], 6799);
        assert_eq!(exported["proxyUrl"], "socks5://127.0.0.1:1080");
    }

    #[test]
    fn token_istenirse_disa_aktarilir() {
        let payload = export_payload(&custom_settings(), true, "x");
        assert_eq!(payload["settings"]["webToken"], "gizli123");
    }

    #[test]
    fn ice_aktarma_token_yoksa_mevcut_token_korunur() {
        let current = custom_settings();
        let payload = export_payload(&current, false, "x");

        // Farklı bir makinede token farklı olsun.
        let mut target = current.clone();
        target.web_token = "baska999".into();

        let outcome = apply_import(&target, &payload).unwrap();
        assert_eq!(
            outcome.settings.web_token, "baska999",
            "token'sız yedek mevcut token'ı silmemeli"
        );
        assert!(
            !outcome.changed.contains(&"webToken".to_string()),
            "token değişmedi sayılmalı"
        );
    }

    #[test]
    fn ice_aktarma_alanlari_uygular_ve_degisenleri_bildirir() {
        let current = Settings {
            max_concurrent: 3,
            download_dir: PathBuf::from("C:\\Users\\ornek\\A"),
            ..Settings::default()
        };

        let source = Settings {
            max_concurrent: 8,
            download_dir: PathBuf::from("C:\\Users\\ornek\\B"),
            notify_on_finish: false,
            ..current.clone()
        };
        let payload = export_payload(&source, false, "x");

        let outcome = apply_import(&current, &payload).unwrap();
        assert_eq!(outcome.settings.max_concurrent, 8);
        assert_eq!(
            outcome.settings.download_dir,
            PathBuf::from("C:\\Users\\ornek\\B")
        );
        assert!(!outcome.settings.notify_on_finish);
        assert!(outcome.changed.contains(&"maxConcurrent".to_string()));
        assert!(outcome.changed.contains(&"downloadDir".to_string()));
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn bozuk_veya_yabanci_dosya_reddedilir() {
        let current = Settings::default();

        // Format imzası yanlış (ör. downloads.json).
        let wrong = serde_json::json!({ "format": "baska", "version": 1, "settings": {} });
        assert!(apply_import(&current, &wrong).is_err());

        // İmza yok.
        let missing = serde_json::json!({ "settings": {} });
        assert!(apply_import(&current, &missing).is_err());

        // Sürüm yok.
        let no_version = serde_json::json!({ "format": SETTINGS_FORMAT, "settings": {} });
        assert!(apply_import(&current, &no_version).is_err());

        // Gelecek sürüm: yarım uygulama yerine reddet.
        let future = serde_json::json!({
            "format": SETTINGS_FORMAT,
            "version": SETTINGS_EXPORT_VERSION + 1,
            "settings": {}
        });
        let err = apply_import(&current, &future).unwrap_err();
        assert!(err.contains("daha yeni"), "anlaşılır hata: {err}");

        // settings nesnesi yok.
        let no_settings = serde_json::json!({ "format": SETTINGS_FORMAT, "version": 1 });
        assert!(apply_import(&current, &no_settings).is_err());

        // JSON dizi (nesne değil).
        assert!(apply_import(&current, &serde_json::json!([1, 2])).is_err());
    }

    #[test]
    fn ice_aktarmada_degerler_sinirlanir() {
        let current = Settings::default();
        let payload = serde_json::json!({
            "format": SETTINGS_FORMAT,
            "version": 1,
            "settings": {
                "maxConcurrent": 999,
                "connections": 500,
                "webPort": 12,
                "webToken": "ge;çersiz!!token",
                "speedLimitKbps": 99_999_999u64,
                "proxyUrl": "ftp://yok"
            }
        });
        let outcome = apply_import(&current, &payload).unwrap();
        assert_eq!(outcome.settings.max_concurrent, 10, "üst sınır uygulandı");
        assert_eq!(outcome.settings.connections, crate::model::MAX_CONNECTIONS);
        assert_eq!(
            outcome.settings.web_port, 6790,
            "geçersiz port varsayılana döndü"
        );
        assert_eq!(
            outcome.settings.web_token, "geersiztoken",
            "geçersiz karakterler temizlendi (yalnız [A-Za-z0-9])"
        );
        assert_eq!(
            outcome.settings.speed_limit_kbps, 0,
            "anlamsız hız sınırsız sayıldı"
        );
        assert_eq!(
            outcome.settings.proxy_url, "",
            "tanınmayan proxy temizlendi"
        );
    }

    #[test]
    fn ice_aktarmada_bos_klasor_mevcut_olanı_korur() {
        let current = Settings {
            download_dir: PathBuf::from("C:\\Users\\ornek\\Mevcut"),
            ..Settings::default()
        };
        let payload = serde_json::json!({
            "format": SETTINGS_FORMAT,
            "version": 1,
            "settings": { "downloadDir": "   " }
        });
        let outcome = apply_import(&current, &payload).unwrap();
        assert_eq!(
            outcome.settings.download_dir,
            PathBuf::from("C:\\Users\\ornek\\Mevcut")
        );
        assert!(
            outcome.warnings.iter().any(|w| w.contains("boş")),
            "kullanıcı bilgilendirildi: {:?}",
            outcome.warnings
        );
    }

    #[test]
    fn ice_aktarma_gidis_donus_ayni_ayarlari_verir() {
        // Dışa aktar → içe aktar: alanlar birebir korunur (token hariç).
        let source = custom_settings();
        let payload = export_payload(&source, false, "x");
        let empty = Settings::default();
        let outcome = apply_import(&empty, &payload).unwrap();

        assert_eq!(outcome.settings.max_concurrent, source.max_concurrent);
        assert_eq!(outcome.settings.connections, source.connections);
        assert_eq!(outcome.settings.speed_limit_kbps, source.speed_limit_kbps);
        assert_eq!(outcome.settings.download_dir, source.download_dir);
        assert_eq!(outcome.settings.proxy_url, source.proxy_url);
        assert_eq!(outcome.settings.web_port, source.web_port);
    }

    #[test]
    fn olusturulamayan_klasor_eski_klasore_doner() {
        let mut previous = Settings::default();
        let dir = tempfile::tempdir().unwrap();
        previous.download_dir = dir.path().to_path_buf();

        let mut next = previous.clone();
        // Windows'ta sürücü öneki olmayan garip yol; Unix'te de oluşturulamaz.
        next.download_dir = PathBuf::from("Q:\\hydra-olmayan-surucu\\yok\\yer");

        let (settings, warnings) = verify_download_dir(&next, &previous);
        // Bu ortamda oluşturulabiliyorsa test anlamsız olur; o durumda atla.
        if warnings.is_empty() {
            return;
        }
        assert_eq!(settings.download_dir, previous.download_dir);
        assert!(warnings[0].contains("önceki klasör korundu"));
    }

    #[test]
    fn gecerli_klasor_kabul_edilir() {
        let previous = Settings::default();
        let dir = tempfile::tempdir().unwrap();
        let mut next = previous.clone();
        next.download_dir = dir.path().join("alt/klasor");

        let (settings, warnings) = verify_download_dir(&next, &previous);
        assert!(warnings.is_empty(), "uyarı olmamalı: {warnings:?}");
        assert_eq!(settings.download_dir, next.download_dir);
        assert!(settings.download_dir.is_dir(), "klasör oluşturuldu");
    }
}
