//! Dosya adi, Content-Disposition ve yol yardimcilari.

use std::path::{Path, PathBuf};
use url::Url;

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Windows'ta yasak karakterleri temizler, uzunlugu sinirlar, bos sonuca
/// "download" yazar. Uzanti korunur.
pub fn sanitize_filename(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();

    let mut name = cleaned.trim().trim_matches('.').trim().to_string();
    if name.is_empty() {
        return "download".into();
    }

    // "CON.txt" de Windows'ta rezerve; kok ada bak.
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        name.insert(0, '_');
    }

    // Uzantiyi koruyarak 180 karaktere kirp (char siniri guvenli).
    if name.chars().count() > 180 {
        let path = Path::new(&name);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("download");
        let keep = 180usize.saturating_sub(ext.chars().count() + 1);
        let short: String = stem.chars().take(keep).collect();
        name = if ext.is_empty() {
            short
        } else {
            format!("{short}.{ext}")
        };
    }
    name
}

/// Ayni klasorde cakisma varsa "ad (1).ext" uretir.
pub fn unique_path(dir: &Path, filename: &str) -> PathBuf {
    let first = dir.join(filename);
    if !first.exists() {
        return first;
    }
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = path.extension().and_then(|e| e.to_str());
    for n in 1..10_000u32 {
        let candidate = match ext {
            Some(ext) => dir.join(format!("{stem} ({n}).{ext}")),
            None => dir.join(format!("{stem} ({n})")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}", chrono::Utc::now().timestamp_millis()))
}

/// Content-Disposition'dan dosya adi cikarir. RFC 5987 (`filename*=UTF-8''..`)
/// oncegelir, sonra `filename="..."`, en son ciplak `filename=...`.
pub fn filename_from_disposition(value: &str) -> Option<String> {
    let mut plain: Option<String> = None;
    for part in value.split(';') {
        let part = part.trim();
        let lower = part.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("filename*=") {
            let original = &part[part.len() - rest.len()..];
            if let Some(decoded) = decode_ext_value(original) {
                return Some(decoded);
            }
        } else if let Some((_, rest)) = part.split_once('=') {
            if lower.starts_with("filename=") {
                let val = rest.trim().trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    plain = Some(val.to_string());
                }
            }
        }
    }
    plain
}

/// `UTF-8''na%C3%AFve.txt` veya `iso-8859-1'fr'...` bicimini cozer.
fn decode_ext_value(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('"');
    let mut pieces = value.splitn(3, '\'');
    let charset = pieces.next()?.to_ascii_lowercase();
    let _lang = pieces.next()?;
    let encoded = pieces.next()?;
    if !matches!(charset.as_str(), "utf-8" | "utf8" | "") {
        // Desteklenmeyen karakter setinde ham degeri kullan.
        return Some(encoded.to_string());
    }
    percent_encoding::percent_decode_str(encoded)
        .decode_utf8()
        .ok()
        .map(|s| s.to_string())
}

/// URL yolundan dosya adi tahmini (query yok sayilir).
pub fn filename_from_url(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    let last = url.path_segments()?.rfind(|s| !s.is_empty())?;
    let decoded = percent_encoding::percent_decode_str(last)
        .decode_utf8()
        .ok()?;
    let name = decoded.trim();
    if name.is_empty() {
        None
    } else {
        Some(sanitize_filename(name))
    }
}

/// Uzantidan kaba MIME tahmini (UI ikonlari icin).
pub fn mime_from_filename(name: &str) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "mkv" | "webm" | "mov" | "avi" => "video",
        "mp3" | "m4a" | "flac" | "wav" | "ogg" | "opus" => "audio",
        "pdf" => "pdf",
        "zip" | "rar" | "7z" | "tar" | "gz" | "xz" => "archive",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" => "image",
        "exe" | "msi" | "dmg" | "pkg" => "app",
        "iso" | "img" => "disk",
        "txt" | "md" | "json" | "csv" | "log" => "text",
        _ => "file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yasak_karakterler_temizlenir() {
        assert_eq!(
            sanitize_filename(r#"a<b>c:d"e/f\g|h?i*j"#),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(sanitize_filename("  rapor.pdf  "), "rapor.pdf");
        assert_eq!(sanitize_filename("...."), "download");
        assert_eq!(sanitize_filename(""), "download");
        assert_eq!(sanitize_filename("a\u{0}b"), "a_b");
    }

    #[test]
    fn rezerve_adlar_korunur() {
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("nul"), "_nul");
        assert_eq!(sanitize_filename("console.txt"), "console.txt");
    }

    #[test]
    fn uzun_ad_uzantiyi_korur() {
        let long = format!("{}.pdf", "a".repeat(400));
        let out = sanitize_filename(&long);
        assert!(out.ends_with(".pdf"));
        assert_eq!(out.chars().count(), 180);
    }

    #[test]
    fn disposition_cozumlenir() {
        assert_eq!(
            filename_from_disposition("attachment; filename=\"rapor 2024.pdf\""),
            Some("rapor 2024.pdf".into())
        );
        assert_eq!(
            filename_from_disposition("attachment; filename*=UTF-8''na%C3%AFve%20x.txt"),
            Some("naïve x.txt".into())
        );
        assert_eq!(
            filename_from_disposition("inline; filename=simple.bin"),
            Some("simple.bin".into())
        );
        assert_eq!(filename_from_disposition("attachment"), None);
    }

    #[test]
    fn url_den_ad_uretir() {
        assert_eq!(
            filename_from_url("https://x.com/a/b/rapor%20x.pdf?q=1").as_deref(),
            Some("rapor x.pdf")
        );
        assert_eq!(filename_from_url("https://x.com/"), None);
        assert_eq!(filename_from_url("not a url"), None);
    }

    #[test]
    fn mime_tahmini() {
        assert_eq!(mime_from_filename("a.MP4"), "video");
        assert_eq!(mime_from_filename("a.pdf"), "pdf");
        assert_eq!(mime_from_filename("a.unknown"), "file");
    }

    #[test]
    fn cakisan_ad_numaralanir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_path(dir.path(), "a.txt"), dir.path().join("a.txt"));
        std::fs::write(dir.path().join("a.txt"), b"x").unwrap();
        assert_eq!(
            unique_path(dir.path(), "a.txt"),
            dir.path().join("a (1).txt")
        );
        std::fs::write(dir.path().join("a (1).txt"), b"x").unwrap();
        assert_eq!(
            unique_path(dir.path(), "a.txt"),
            dir.path().join("a (2).txt")
        );
    }
}
