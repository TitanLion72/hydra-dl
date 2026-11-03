//! Web sunucusu için **isteğe bağlı** kendinden imzalı TLS (HTTPS).
//!
//! Neden: web sunucusu LAN'a açılıyor ve token HTTP başlığında gidiyor. Aynı ağdaki
//! biri trafiği dinlerse token'ı düz metin görebilir. Self-signed sertifika mükemmel
//! değildir (kullanıcı tarayıcı uyarısını kabul eder) ama **trafik şifrelenir**.
//!
//! Tasarım kararları:
//! - Sertifika `<app_data>/tls/` altında saklanır; her açılışta yeniden üretilmez
//!   (aksi hâlde tarayıcı her seferinde yeni uyarı gösterirdi).
//! - Kapsanan adlar (`tls.json`) yanında tutulur: LAN IP'si değişirse sertifika
//!   yenilenir, hostname uyuşmazlığı olmaz.
//! - Yakalama sunucusu (`127.0.0.1:6712`, eklenti) **HTTP kalır**: eklenti ona
//!   `127.0.0.1` üzerinden erişir, ağa çıkmaz.

use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// Üretilecek sertifikanın kapsadığı adlar.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CertNames {
    pub dns: Vec<String>,
    pub ips: Vec<IpAddr>,
}

impl CertNames {
    /// Yerel makine için makul varsayılan + verilen ek adresler.
    ///
    /// `extras` LAN adresleridir; yinelenenler elenir, sıra korunur.
    pub fn local(extras: &[String]) -> Self {
        let mut dns = vec!["localhost".to_string()];
        let mut ips: Vec<IpAddr> = vec![
            "127.0.0.1".parse().expect("geçerli IPv4"),
            "::1".parse().expect("geçerli IPv6"),
        ];
        for extra in extras {
            let trimmed = extra.trim();
            if trimmed.is_empty() {
                continue;
            }
            match trimmed.parse::<IpAddr>() {
                Ok(ip) => {
                    if !ips.contains(&ip) {
                        ips.push(ip);
                    }
                }
                Err(_) => {
                    if !dns.iter().any(|name| name.eq_ignore_ascii_case(trimmed)) {
                        dns.push(trimmed.to_string());
                    }
                }
            }
        }
        Self { dns, ips }
    }

    /// `other` içindeki tüm adlar bunda var mı? (Sertifika yeniden kullanılsın mı?)
    pub fn covers(&self, other: &CertNames) -> bool {
        other
            .dns
            .iter()
            .all(|name| self.dns.iter().any(|have| have.eq_ignore_ascii_case(name)))
            && other.ips.iter().all(|ip| self.ips.contains(ip))
    }
}

/// Diskteki sertifika + anahtar çifti ve kapsanan adlar.
pub struct StoredTls {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub names_path: PathBuf,
}

impl StoredTls {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            cert_path: dir.join("cert.pem"),
            key_path: dir.join("key.pem"),
            names_path: dir.join("tls.json"),
        }
    }

    /// Var olan sertifika istenen adları kapsıyorsa onu kullanır; aksi hâlde üretir.
    ///
    /// Dönen: (sertifika PEM, anahtar PEM, yeniden üretildi mi?).
    pub fn ensure(&self, names: &CertNames) -> Result<(Vec<u8>, Vec<u8>, bool), String> {
        if let (Ok(cert), Ok(key), Ok(names_json)) = (
            std::fs::read(&self.cert_path),
            std::fs::read(&self.key_path),
            std::fs::read_to_string(&self.names_path),
        ) {
            if let Ok(existing) = serde_json::from_str::<CertNames>(&names_json) {
                if existing.covers(names) {
                    return Ok((cert, key, false));
                }
            }
        }
        let (cert_pem, key_pem) = generate_pem(names)?;
        if let Some(parent) = self.cert_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("TLS klasörü oluşturulamadı: {e}"))?;
        }
        std::fs::write(&self.cert_path, &cert_pem)
            .map_err(|e| format!("Sertifika yazılamadı: {e}"))?;
        std::fs::write(&self.key_path, &key_pem).map_err(|e| format!("Anahtar yazılamadı: {e}"))?;
        let json = serde_json::to_string_pretty(names)
            .map_err(|e| format!("Ad listesi yazılamadı: {e}"))?;
        std::fs::write(&self.names_path, json)
            .map_err(|e| format!("Ad listesi yazılamadı: {e}"))?;
        Ok((cert_pem, key_pem, true))
    }
}

/// Verilen adlar için kendinden imzalı sertifika + anahtar üretir (PEM).
pub fn generate_pem(names: &CertNames) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut params =
        rcgen::CertificateParams::new(names.dns.clone()).map_err(|e| format!("Sertifika: {e}"))?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Hydra DL (yerel)");
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "Hydra DL");
    for ip in &names.ips {
        params
            .subject_alt_names
            .push(rcgen::SanType::IpAddress(*ip));
    }
    // Sabit tarihler: yeniden üretimde tarayıcı "yeni sertifika" uyarısı vermesin.
    params.not_before = rcgen::date_time_ymd(2024, 1, 1);
    params.not_after = rcgen::date_time_ymd(2036, 1, 1);
    let key_pair = rcgen::KeyPair::generate().map_err(|e| format!("Anahtar: {e}"))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| format!("İmzalama: {e}"))?;
    Ok((
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    ))
}

/// PEM içerikten axum-server TLS yapılandırması kurar.
///
/// **CryptoProvider:** rustls 0.23, süreç düzeyindeki sağlayıcıyı features'tan
/// otomatik seçemiyor (bu projede `reqwest` → `aws-lc-rs`, `rcgen` → `ring`).
/// Kurulmazsa `ServerConfig` oluşturulurken rustls **panic** atıyor; release
/// profili `panic = "abort"` olduğu için uygulama tamamen kapanıyordu. Bu yüzden
/// sağlayıcıyı burada açıkça kuruyoruz. `install_default` birden fazla çağrıda
/// zararsızdır (ikinci çağrı hata döner, yok sayılır).
pub async fn rustls_config(
    cert_pem: &[u8],
    key_pem: &[u8],
) -> Result<axum_server::tls_rustls::RustlsConfig, String> {
    install_crypto_provider();
    axum_server::tls_rustls::RustlsConfig::from_pem(cert_pem.to_vec(), key_pem.to_vec())
        .await
        .map_err(|e| format!("TLS yapılandırması: {e}"))
}

/// Süreç düzeyinde rustls sağlayıcısını kurar (bir kez etkili olur).
pub fn install_crypto_provider() {
    // Zaten kuruluysa `Err` döner; bu bir hata değil.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yerel_adlar_localhost_ve_loopback_icerir() {
        let names = CertNames::local(&[]);
        assert!(names.dns.iter().any(|n| n == "localhost"));
        assert!(names.ips.iter().any(|ip| ip.is_loopback()));
    }

    #[test]
    fn lan_adresleri_eklenir_ve_yinelenmez() {
        let names = CertNames::local(&[
            "192.168.1.42".into(),
            "192.168.1.42".into(),
            "127.0.0.1".into(),
            "hydra.local".into(),
        ]);
        assert_eq!(
            names
                .ips
                .iter()
                .filter(|ip| ip.to_string() == "192.168.1.42")
                .count(),
            1,
            "yinelenen IP bir kez"
        );
        assert_eq!(
            names
                .ips
                .iter()
                .filter(|ip| ip.to_string() == "127.0.0.1")
                .count(),
            1,
            "127.0.0.1 ikinci kez eklenmez"
        );
        assert_eq!(
            names.ips.iter().filter(|ip| ip.is_loopback()).count(),
            2,
            "127.0.0.1 + ::1 (ikisi de loopback)"
        );
        assert!(names.dns.iter().any(|n| n == "hydra.local"));
    }

    #[test]
    fn covers_eksik_adresi_yakalar() {
        let have = CertNames::local(&["192.168.1.42".into()]);
        assert!(have.covers(&have), "kendini kapsar");
        let need_more = CertNames::local(&["192.168.1.42".into(), "10.0.0.7".into()]);
        assert!(!have.covers(&need_more), "yeni LAN IP'si kapsanmıyor");
    }

    #[test]
    fn sertifika_uretilir_ve_pem_bicimindedir() {
        let names = CertNames::local(&["192.168.1.42".into()]);
        let (cert, key) = generate_pem(&names).unwrap();
        let cert_text = String::from_utf8_lossy(&cert);
        let key_text = String::from_utf8_lossy(&key);
        assert!(cert_text.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(cert_text.trim_end().ends_with("-----END CERTIFICATE-----"));
        assert!(key_text.contains("PRIVATE KEY"));
        // SAN'lar sertifikanın içinde (kabaca): DER base64'ünde IP metni geçmeyebilir,
        // bu yüzden yalnız boyut ve biçim doğrulanır.
        assert!(cert.len() > 400, "sertifika makul boyutta");
    }

    #[test]
    fn diskteki_sertifika_yeniden_kullanilir_ve_lan_degisince_yenilenir() {
        let dir = tempfile::tempdir().unwrap();
        let store = StoredTls::in_dir(dir.path());
        let names = CertNames::local(&["192.168.1.42".into()]);
        let (cert1, _key1, regenerated1) = store.ensure(&names).unwrap();
        assert!(regenerated1, "ilk seferde üretilir");
        assert!(store.cert_path.exists() && store.key_path.exists());
        assert!(store.names_path.exists());

        let (cert2, _key2, regenerated2) = store.ensure(&names).unwrap();
        assert!(!regenerated2, "aynı adlar için yeniden üretilmez");
        assert_eq!(
            cert1, cert2,
            "sertifika aynı kalır (tarayıcı uyarısı tekrar etmesin)"
        );

        // Yeni LAN IP'si → sertifika yenilenir.
        let wider = CertNames::local(&["192.168.1.42".into(), "10.0.0.7".into()]);
        let (cert3, _key3, regenerated3) = store.ensure(&wider).unwrap();
        assert!(regenerated3, "kapsam büyüyünce yenilenir");
        assert_ne!(cert1, cert3);
    }

    #[test]
    fn bozuk_sertifika_dosyasi_yeniden_uretilir() {
        let dir = tempfile::tempdir().unwrap();
        let store = StoredTls::in_dir(dir.path());
        std::fs::write(&store.cert_path, b"bozuk").unwrap();
        std::fs::write(&store.key_path, b"bozuk").unwrap();
        std::fs::write(&store.names_path, b"{gecersiz json").unwrap();
        let names = CertNames::local(&[]);
        let (cert, _key, regenerated) = store.ensure(&names).unwrap();
        assert!(regenerated);
        assert!(String::from_utf8_lossy(&cert).contains("BEGIN CERTIFICATE"));
    }

    /// Regresyon (canlı çökme): rustls sağlayıcısı kurulmadan `ServerConfig`
    /// oluşturulunca **panic** atıyordu; release'te `panic = "abort"` olduğu için
    /// "HTTPS kullan" açıldığında uygulama tamamen kapanıyordu. Bu test panik'lerse
    /// kırmızı olur ve hatayı sürüm çıkmadan yakalar.
    #[test]
    fn tls_yapilandirmasi_saglayici_kurulunca_olusturulur() {
        install_crypto_provider();
        let names = CertNames::local(&["127.0.0.1".into()]);
        let (cert, key) = generate_pem(&names).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let config = runtime.block_on(rustls_config(&cert, &key));
        assert!(config.is_ok(), "TLS yapılandırması kurulmalı: {config:?}");
    }
}
