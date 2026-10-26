//! Akilli kaynak bulucu: bir baglanti verildiginde gercek indirme kaynagini
//! cikarir (dogrudan dosya, HLS/DASH manifesti, sayfa icindeki medya).
//!
//! Yontem sirasi:
//! 1. magnet / .torrent  -> `torrent`
//! 2. HEAD / 1 baytlik Range -> dosya mi, sayfa mi?
//! 3. Sayfa ise HTML'i indir, medya adaylarini cikar (meta, video, json-ld, script)
//! 4. Aday yok ya da bilinen medya sitesi ise yt-dlp'den format listesi al
//! 5. HLS master playlist ise varyantlari coz

use crate::engine::http;
use crate::model::Kind;
use crate::ytdlp::{self, YtdlpFormat};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use url::Url;

/// Sayfadan okunacak en fazla HTML boyutu.
const MAX_HTML: usize = 4 * 1024 * 1024;
/// yt-dlp cagrisi icin ust sinir.
const YTDLP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
/// Bu hostlarda dogrudan HTML cekmek yerine yt-dlp daha iyi sonuc verir.
const MEDIA_HOSTS: [&str; 26] = [
    "youtube.com",
    "youtu.be",
    "music.youtube.com",
    "instagram.com",
    "tiktok.com",
    "twitter.com",
    "x.com",
    "facebook.com",
    "fb.watch",
    "vimeo.com",
    "twitch.tv",
    "dailymotion.com",
    "soundcloud.com",
    "bilibili.com",
    "reddit.com",
    "redd.it",
    "pinterest.com",
    "snapchat.com",
    "linkedin.com",
    "rumble.com",
    "odysee.com",
    "kick.com",
    "streamable.com",
    "vk.com",
    "ok.ru",
    "beyazperde.com",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Format {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filesize: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcodec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acodec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub url: String,
    pub kind: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub ok: bool,
    pub url: String,
    pub final_url: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    /// Medya süresi (saniye), yt-dlp'den gelirse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    pub resumable: bool,
    pub needs_ytdlp: bool,
    pub formats: Vec<Format>,
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<String>,
}

impl ProbeResult {
    fn base(url: &str, kind: &str) -> Self {
        Self {
            ok: true,
            url: url.to_string(),
            final_url: url.to_string(),
            kind: kind.to_string(),
            title: None,
            filename: None,
            size: None,
            mime: None,
            duration_secs: None,
            resumable: false,
            needs_ytdlp: false,
            formats: Vec::new(),
            candidates: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

pub fn is_torrent_source(raw: &str) -> bool {
    let trimmed = raw.trim();
    trimmed.starts_with("magnet:")
        || trimmed.to_ascii_lowercase().ends_with(".torrent")
        || trimmed.starts_with("data:application/x-bittorrent")
}

pub fn is_known_media_host(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let host = parsed
        .host_str()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    MEDIA_HOSTS
        .iter()
        .any(|known| host == *known || host.ends_with(&format!(".{known}")))
}

fn is_manifest(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains(".m3u8") || lower.contains(".mpd")
}

fn is_dash(url: &str) -> bool {
    url.to_ascii_lowercase().contains(".mpd")
}

/// Bir URL'nin medya olup olmadigini uzantidan tahmin et.
fn media_kind_from_url(url: &str) -> &'static str {
    let ext = Url::parse(url)
        .ok()
        .and_then(|u| u.path().rsplit('.').next().map(|s| s.to_ascii_lowercase()))
        .unwrap_or_default();
    match ext.as_str() {
        "m3u8" => "hls",
        "mpd" => "dash",
        "mp4" | "webm" | "mkv" | "mov" | "avi" | "flv" | "m4v" => "video",
        "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg" | "opus" => "audio",
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "svg" => "image",
        "pdf" | "epub" | "zip" | "rar" | "7z" | "tar" | "gz" | "iso" | "exe" | "msi" => "file",
        _ => "file",
    }
}

fn looks_like_html(mime: Option<&str>) -> bool {
    mime.map(|m| {
        let m = m.to_ascii_lowercase();
        m.contains("text/html") || m.contains("application/xhtml")
    })
    .unwrap_or(false)
}

/// HTML'den indirme adaylarini cikarir. Saf fonksiyon (test edilebilir).
pub fn extract_candidates(html: &str, base: &str) -> (Vec<Candidate>, Option<String>) {
    // JSON/JS icindeki kacisli slash'lari normalize et (`https:\/\/...`).
    let html = &html.replace("\\/", "/");
    let mut found: Vec<Candidate> = Vec::new();
    let mut title: Option<String> = None;
    let mut seen = std::collections::HashSet::new();

    let mut push = |raw: &str, label: &str| {
        let cleaned = clean_url(raw);
        let Some(resolved) = resolve_url(base, &cleaned) else {
            return;
        };
        if !seen.insert(resolved.clone()) {
            return;
        }
        let kind = media_kind_from_url(&resolved);
        found.push(Candidate {
            label: label.to_string(),
            url: resolved,
            kind: kind.to_string(),
            size: None,
        });
    };

    // <title> ve og:title
    if let Some(caps) = meta_title_regex().captures(html) {
        let text = html_unescape(caps[1].trim());
        if !text.is_empty() {
            title = Some(text);
        }
    }

    for (property, label) in [
        ("og:video:secure_url", "og:video"),
        ("og:video:url", "og:video"),
        ("og:video", "og:video"),
        ("og:audio:secure_url", "og:audio"),
        ("og:audio", "og:audio"),
        ("twitter:player:stream", "twitter:stream"),
        ("twitter:image", "twitter:image"),
        ("og:image", "og:image"),
    ] {
        for content in meta_contents(html, property) {
            push(&content, label);
        }
    }

    // JSON-LD contentUrl / embedUrl
    for caps in json_url_regex().captures_iter(html) {
        if let Some(value) = caps.get(1).or_else(|| caps.get(2)) {
            push(value.as_str(), "json-ld");
        }
    }

    // <video src>, <source src>, <audio src>, <track>
    for caps in media_tag_regex().captures_iter(html) {
        if let Some(src) = caps.get(1) {
            push(src.as_str(), "video etiketi");
        }
    }

    // <iframe src> (gömülü oynaticilar)
    for caps in iframe_regex().captures_iter(html) {
        if let Some(src) = caps.get(1) {
            let cleaned = clean_url(src.as_str());
            if let Some(resolved) = resolve_url(base, &cleaned) {
                if is_known_media_host(&resolved) || is_manifest(&resolved) {
                    push(&resolved, "gömülü oynatıcı");
                }
            }
        }
    }

    // Script/JSON icindeki manifest ve dogrudan medya adresleri
    for caps in loose_media_regex().captures_iter(html) {
        if let Some(url) = caps.get(0) {
            push(url.as_str(), "sayfa içi kaynak");
        }
    }

    // Siralama: manifestler once, sonra video/ses, sonra gorsel
    found.sort_by_key(|c| match c.kind.as_str() {
        "hls" | "dash" => 0,
        "video" => 1,
        "audio" => 2,
        "image" => 4,
        _ => 3,
    });

    (found, title)
}

fn clean_url(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_quotes = trimmed.trim_matches(|c| c == '"' || c == '\'' || c == ',');
    // HTML entity'lerini coz (&amp; gibi) ve ters slash kacislarini temizle.
    html_unescape(without_quotes)
        .replace("\\/", "/")
        .replace("\\u0026", "&")
}

fn html_unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&#38;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn resolve_url(base: &str, raw: &str) -> Option<String> {
    if raw.is_empty() || raw.starts_with("data:") || raw.starts_with("blob:") {
        return None;
    }
    if raw.starts_with("//") {
        let scheme = Url::parse(base)
            .map(|u| u.scheme().to_string())
            .unwrap_or_else(|_| "https".into());
        return Some(format!("{scheme}:{raw}"));
    }
    let base_url = Url::parse(base).ok()?;
    base_url.join(raw).ok().map(|u| u.to_string())
}

fn meta_contents(html: &str, property: &str) -> Vec<String> {
    let mut out = Vec::new();
    // <meta property="og:video" content="..."> her iki sirada da olabilir.
    for caps in meta_tag_regex().captures_iter(html) {
        let tag = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        if !tag.contains(property) {
            continue;
        }
        if let Some(content) = attr_regex().captures(tag).and_then(|c| c.get(1)) {
            out.push(content.as_str().to_string());
        }
    }
    out
}

fn meta_title_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").expect("gecerli regex"))
}

fn meta_tag_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<meta\b[^>]*>").expect("gecerli regex"))
}

fn attr_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)content\s*=\s*(?:"([^"]*)"|'([^']*)')"#).expect("gecerli regex")
    })
}

fn json_url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)"(?:contentUrl|embedUrl)"\s*:\s*"([^"]+)""#).expect("gecerli regex")
    })
}

fn media_tag_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?is)<(?:video|audio|source|track)\b[^>]*\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)')"#,
        )
        .expect("gecerli regex")
    })
}

fn iframe_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<iframe\b[^>]*\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)')"#)
            .expect("gecerli regex")
    })
}

fn loose_media_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)(?:https?:)?(?:\\?/\\?/|//)[^\s"'<>\\]+?\.(?:m3u8|mpd|mp4|webm|mkv|mov|mp3|m4a|flac|wav|ogg|opus|aac|pdf|epub|zip|rar|7z|iso|apk)(?:\?[^\s"'<>\\]*)?"#,
        )
        .expect("gecerli regex")
    })
}

/// HLS master playlist'indeki varyantlari cozer.
pub fn parse_hls_variants(playlist: &str, base: &str) -> Vec<Format> {
    let mut out = Vec::new();
    let mut pending: Option<(Option<u32>, Option<u64>, Option<String>)> = None;
    for line in playlist.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let height = res_regex()
                .captures(rest)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u32>().ok());
            let bandwidth = bandwidth_regex()
                .captures(rest)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u64>().ok());
            let codecs = codecs_regex()
                .captures(rest)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());
            pending = Some((height, bandwidth, codecs));
            continue;
        }
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((height, bandwidth, codecs)) = pending.take() {
            let Some(url) = resolve_url(base, line) else {
                continue;
            };
            let label = match (height, bandwidth) {
                (Some(h), Some(bw)) => format!("{h}p · {}", bitrate_label(bw)),
                (Some(h), None) => format!("{h}p"),
                (None, Some(bw)) => bitrate_label(bw),
                (None, None) => "varyant".to_string(),
            };
            out.push(Format {
                id: url,
                label,
                ext: Some("m3u8".into()),
                height,
                filesize: None,
                vcodec: codecs,
                acodec: None,
                note: Some("HLS varyantı".into()),
            });
        }
    }
    // Varyant yoksa (medya playlist) tek akis olarak sun.
    if out.is_empty() && playlist.contains("#EXTINF") {
        out.push(Format {
            id: base.to_string(),
            label: "HLS akışı".into(),
            ext: Some("m3u8".into()),
            height: None,
            filesize: None,
            vcodec: None,
            acodec: None,
            note: Some("tek akış".into()),
        });
    }
    out
}

fn bitrate_label(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.1} Mbps", bps as f64 / 1_000_000.0)
    } else {
        format!("{} kbps", bps / 1000)
    }
}

fn res_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)RESOLUTION=\d+x(\d+)").expect("gecerli regex"))
}

fn bandwidth_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)BANDWIDTH=(\d+)").expect("gecerli regex"))
}

fn codecs_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)CODECS="([^"]+)""#).expect("gecerli regex"))
}

fn ytdlp_formats_to_formats(list: &[YtdlpFormat]) -> Vec<Format> {
    let mut out: Vec<Format> = list
        .iter()
        .filter(|f| f.vcodec.as_deref() != Some("none") || f.acodec.as_deref() != Some("none"))
        .map(|f| {
            let video_only =
                f.vcodec.as_deref() != Some("none") && f.acodec.as_deref() == Some("none");
            // Video-only formatta ses ayri akista; yt-dlp'nin en iyi sesi eklemesini
            // sagla. Aksi halde kullanici "1080p" secince sessiz video iner.
            let id = if video_only {
                format!("{}+bestaudio/{}", f.format_id, f.format_id)
            } else {
                f.format_id.clone()
            };
            Format {
                id,
                label: f.label(),
                ext: f.ext.clone(),
                height: f.height,
                filesize: f.filesize.or(f.filesize_approx),
                vcodec: f.vcodec.clone(),
                acodec: f.acodec.clone(),
                note: f.format_note.clone(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.height
            .unwrap_or(0)
            .cmp(&a.height.unwrap_or(0))
            .then(b.filesize.unwrap_or(0).cmp(&a.filesize.unwrap_or(0)))
    });
    out
}

/// Manifest/duz metin icerigi indirir (HLS playlist, DASH XML, JSON).
async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("adres açılamadı: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{} döndürdü", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("içerik okunamadı: {e}"))?;
    let slice = &bytes[..bytes.len().min(MAX_HTML)];
    Ok(String::from_utf8_lossy(slice).to_string())
}

async fn fetch_html(client: &reqwest::Client, url: &str) -> Result<(String, String), String> {
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("sayfa açılamadı: {e}"))?;
    let final_url = response.url().to_string();
    if !response.status().is_success() {
        return Err(format!("sayfa {} döndürdü", response.status()));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.contains("html") && !content_type.contains("json") && !content_type.is_empty()
    {
        return Err(format!("sayfa değil ({content_type})"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("sayfa okunamadı: {e}"))?;
    let slice = &bytes[..bytes.len().min(MAX_HTML)];
    Ok((String::from_utf8_lossy(slice).to_string(), final_url))
}

/// Ana giris noktasi: baglantiyi incele ve indirilebilir secenekleri dondur.
pub async fn probe(
    client: &reqwest::Client,
    ytdlp_bin: Option<&std::path::Path>,
    raw_url: &str,
    media: Option<bool>,
) -> Result<ProbeResult, String> {
    let raw_url = raw_url.trim();
    if raw_url.is_empty() {
        return Err("Bağlantı boş".into());
    }
    if is_torrent_source(raw_url) {
        let mut result = ProbeResult::base(raw_url, "torrent");
        result.needs_ytdlp = false;
        result.resumable = true;
        result.filename = torrent_name_hint(raw_url);
        return Ok(result);
    }
    let parsed = Url::parse(raw_url).map_err(|_| "Geçersiz bağlantı".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Yalnızca http/https bağlantıları destekleniyor".into());
    }

    let prefer_media = media == Some(true);
    let avoid_ytdlp = media == Some(false);
    let known_media = is_known_media_host(raw_url);
    let mut warnings: Vec<String> = Vec::new();

    // 1) Dogrudan dosya mi?
    let direct = http::probe(client, raw_url).await;
    if let Ok(probe) = &direct {
        let mime = probe.mime.clone().unwrap_or_default();
        let is_html = looks_like_html(Some(&mime));
        if !is_html && probe.total.is_some() && !is_manifest(raw_url) {
            let mut result = ProbeResult::base(raw_url, media_kind_from_url(raw_url));
            if result.kind == "file" && mime.starts_with("video/") {
                result.kind = "video".into();
            } else if result.kind == "file" && mime.starts_with("audio/") {
                result.kind = "audio".into();
            } else if result.kind == "file" && mime.starts_with("image/") {
                result.kind = "image".into();
            }
            result.filename = probe
                .filename
                .clone()
                .or_else(|| crate::util::filename_from_url(raw_url));
            result.size = probe.total;
            result.mime = if mime.is_empty() {
                None
            } else {
                Some(mime.clone())
            };
            result.resumable = probe.resumable;
            result.final_url = raw_url.to_string();
            result.candidates.push(Candidate {
                url: raw_url.to_string(),
                kind: result.kind.clone(),
                label: "Doğrudan bağlantı".into(),
                size: probe.total,
            });
            result.formats.push(Format {
                id: "direct".into(),
                label: match probe.total {
                    Some(size) => format!("Orijinal dosya · {}", human_size(size)),
                    None => "Orijinal dosya".into(),
                },
                ext: result
                    .filename
                    .as_deref()
                    .and_then(|n| n.rsplit('.').next())
                    .map(|s| s.to_ascii_lowercase()),
                height: None,
                filesize: probe.total,
                vcodec: None,
                acodec: None,
                note: None,
            });
            return Ok(result);
        }
        if is_manifest(raw_url) {
            // 2) HLS/DASH manifesti: varyantlari coz
            let mut result =
                ProbeResult::base(raw_url, if is_dash(raw_url) { "dash" } else { "hls" });
            result.resumable = true;
            if !is_dash(raw_url) {
                if let Ok(text) = fetch_text(client, raw_url).await {
                    result.formats = parse_hls_variants(&text, raw_url);
                }
            }
            result.filename =
                Some(crate::util::filename_from_url(raw_url).unwrap_or_else(|| "stream".into()));
            result.formats.push(Format {
                id: if is_dash(raw_url) {
                    "dash".into()
                } else {
                    "hls".into()
                },
                label: if is_dash(raw_url) {
                    "DASH akışı (yt-dlp)".into()
                } else {
                    "HLS akışı".into()
                },
                ext: Some(if is_dash(raw_url) { "mpd" } else { "m3u8" }.into()),
                height: None,
                filesize: None,
                vcodec: None,
                acodec: None,
                note: Some("manifest".into()),
            });
            result.needs_ytdlp = ytdlp_bin.is_some();
            return Ok(result);
        }
    }

    // 3) Sayfa: HTML'i indir ve adaylari cikar
    let mut title: Option<String> = None;
    let mut final_url = raw_url.to_string();
    let mut candidates: Vec<Candidate> = Vec::new();
    if !known_media {
        match fetch_html(client, raw_url).await {
            Ok((html, resolved)) => {
                final_url = resolved.clone();
                let (found, found_title) = extract_candidates(&html, &resolved);
                candidates = found;
                title = found_title;
            }
            Err(err) => {
                warnings.push(err);
            }
        }
    }

    let ytdlp_available = ytdlp_bin.is_some();
    let mut formats: Vec<Format> = Vec::new();
    let mut duration_secs: Option<f64> = None;

    // Adaylarin ilk manifesti varsa varyantlari cikar.
    if let Some(manifest) = candidates.iter().find(|c| c.kind == "hls") {
        if let Ok(text) = fetch_text(client, &manifest.url).await {
            formats = parse_hls_variants(&text, &manifest.url);
        }
    }

    // 4) yt-dlp: bilinen medya sitesi, sayfada aday yok ya da kullanici medya dedi
    let want_ytdlp = !avoid_ytdlp
        && ytdlp_available
        && (known_media || prefer_media || candidates.is_empty() || formats.is_empty());
    if want_ytdlp {
        if let Some(binary) = ytdlp_bin {
            let info =
                tokio::time::timeout(YTDLP_TIMEOUT, ytdlp::list_formats(binary, raw_url)).await;
            match info {
                Ok(Ok(info)) => {
                    if title.is_none() {
                        title = info.title.clone();
                    }
                    duration_secs = info.duration_secs;
                    if info.formats.is_empty() {
                        warnings.push("yt-dlp bu bağlantı için format listesi vermedi".into());
                    }
                    formats = ytdlp_formats_to_formats(&info.formats);
                    if !formats.is_empty() {
                        // yt-dlp format kimlikleri "media" turu icin gecerlidir.
                        if let Some(first) = formats.first_mut() {
                            first.note = Some("en iyi kalite".into());
                        }
                    }
                    if info.is_live {
                        warnings.push(
                            "Canlı yayın: indirme başladığı andan itibaren kaydedilir".into(),
                        );
                    }
                }
                Ok(Err(err)) => warnings.push(format!("yt-dlp: {err}")),
                Err(_) => warnings.push("yt-dlp zaman aşımına uğradı".into()),
            }
        }
    } else if !known_media && candidates.is_empty() && !avoid_ytdlp && !ytdlp_available {
        warnings.push(
            "Sayfada doğrudan medya bulunamadı. Video indirmek için yt-dlp kurun (Ayarlar → Medya)."
                .into(),
        );
    }

    let has_av = candidates
        .iter()
        .any(|c| c.kind == "video" || c.kind == "audio");
    let kind = if !formats.is_empty() || (!candidates.is_empty() && known_media) || has_av {
        "media"
    } else if candidates.is_empty() {
        "page"
    } else {
        "file"
    };

    // Aday yoksa ve elimizde yt-dlp formati varsa "en iyi" sanal aday ekle.
    if candidates.is_empty() && !formats.is_empty() {
        candidates.push(Candidate {
            url: raw_url.to_string(),
            kind: "media".into(),
            label: "Medya sayfası (en iyi kalite)".into(),
            size: formats.first().and_then(|f| f.filesize),
        });
    }

    let mut result = ProbeResult::base(raw_url, kind);
    result.final_url = final_url;
    // Dosya adi: gercek dosya adayi > manifest adayi > sayfa adresi.
    let best_for_name = candidates
        .iter()
        .find(|c| matches!(c.kind.as_str(), "video" | "audio" | "file"))
        .or_else(|| candidates.first());
    let name_from_url = best_for_name
        .and_then(|c| crate::util::filename_from_url(&c.url))
        .or_else(|| crate::util::filename_from_url(raw_url));
    // Medya sayfalarinda URL'den turetilen ad anlamsiz olur (YouTube'da "watch").
    // Baslik varsa onu kullan; yoksa eski davranisa don.
    result.filename = if kind == "media" {
        media_filename_from_title(title.as_deref(), formats.first()).or(name_from_url)
    } else {
        name_from_url
    };
    result.title = title;
    result.size = candidates.iter().find_map(|c| c.size);
    result.mime = candidates.first().map(|c| c.kind.clone());
    result.duration_secs = duration_secs;
    result.resumable = candidates
        .iter()
        .any(|c| matches!(c.kind.as_str(), "file" | "video" | "audio"));
    result.needs_ytdlp = ytdlp_available
        && formats
            .iter()
            .any(|f| f.note.as_deref() == Some("en iyi kalite"));
    result.formats = formats;
    result.candidates = candidates;
    result.warnings = warnings;

    if result.formats.is_empty() && result.candidates.is_empty() {
        return Err(
            "Bu bağlantıda indirilebilir bir kaynak bulunamadı. Sayfa giriş istiyor ya da JavaScript ile yükleniyor olabilir."
                .into(),
        );
    }
    Ok(result)
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Medya sayfasinda basliktan Windows-guvenli dosya adi uretir: `Baslik.mp4`.
///
/// YouTube gibi adreslerde URL'den turetilen ad anlamsiz oluyor ("watch");
/// baslik varsa kullaniciya onu gosteriyoruz. Uzanti secilen formattan gelir.
fn media_filename_from_title(title: Option<&str>, best: Option<&Format>) -> Option<String> {
    let title = title?.trim();
    if title.is_empty() {
        return None;
    }
    let base = crate::util::sanitize_filename(title);
    if base == "download" {
        return None;
    }
    let ext = best
        .and_then(|f| f.ext.clone())
        .map(|e| e.trim_start_matches('.').to_string())
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| "mp4".into());
    Some(format!("{base}.{ext}"))
}

pub fn torrent_name_hint(source: &str) -> Option<String> {
    if let Some(rest) = source.strip_prefix("magnet:?") {
        for pair in rest.split('&') {
            if let Some(name) = pair.strip_prefix("dn=") {
                let decoded = percent_encoding::percent_decode_str(name)
                    .decode_utf8_lossy()
                    .to_string();
                if !decoded.trim().is_empty() {
                    return Some(crate::util::sanitize_filename(&decoded));
                }
            }
        }
    }
    crate::util::filename_from_url(source).or_else(|| Some("torrent".into()))
}

/// Indirme turunu kaynaktan tahmin et.
pub fn kind_for_source(source: &str, media: Option<bool>) -> Kind {
    if let Some(forced) = media {
        return if forced { Kind::Media } else { Kind::Http };
    }
    if is_torrent_source(source) {
        Kind::Torrent
    } else if is_known_media_host(source) || is_manifest(source) {
        Kind::Media
    } else {
        Kind::Http
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnet_algilanir() {
        assert!(is_torrent_source(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=ornek"
        ));
        assert!(is_torrent_source("https://example.com/a.torrent"));
        assert!(!is_torrent_source("https://example.com/a.mp4"));
    }

    #[test]
    fn magnet_adi_cozulur() {
        let name = torrent_name_hint("magnet:?xt=urn:btih:abc&dn=Ubuntu%2024.04.iso&tr=udp://x");
        assert_eq!(name.as_deref(), Some("Ubuntu 24.04.iso"));
    }

    #[test]
    fn bilinen_medya_hostlari() {
        assert!(is_known_media_host("https://www.youtube.com/watch?v=1"));
        assert!(is_known_media_host("https://youtu.be/abc"));
        assert!(is_known_media_host("https://m.instagram.com/reel/x"));
        assert!(!is_known_media_host("https://example.com/a.mp4"));
    }

    #[test]
    fn html_adaylari_cikarilir() {
        let html = r#"
        <html><head>
          <title>Kedi videosu &amp; arkadaslari</title>
          <meta property="og:title" content="Kedi">
          <meta property="og:video" content="https://cdn.example.com/v/kedi.mp4">
          <meta name="twitter:player:stream" content="//cdn.example.com/v/kedi2.mp4">
          <script type="application/ld+json">
            {"@type":"VideoObject","contentUrl":"https://cdn.example.com/v/json.mp4"}
          </script>
        </head><body>
          <video src="/media/local.webm"></video>
          <video><source src="https://cdn.example.com/v/source.mp4"></video>
          <iframe src="https://player.vimeo.com/video/123"></iframe>
          <script>var src = "https:\/\/cdn.example.com\/hls\/master.m3u8?token=1";</script>
        </body></html>"#;
        let (candidates, title) = extract_candidates(html, "https://site.example/sayfa/1");
        assert_eq!(title.as_deref(), Some("Kedi videosu & arkadaslari"));
        let urls: Vec<&str> = candidates.iter().map(|c| c.url.as_str()).collect();
        assert!(urls.contains(&"https://cdn.example.com/v/kedi.mp4"));
        assert!(urls.contains(&"https://cdn.example.com/v/kedi2.mp4"));
        assert!(urls.contains(&"https://cdn.example.com/v/json.mp4"));
        assert!(urls.contains(&"https://site.example/media/local.webm"));
        assert!(urls.contains(&"https://cdn.example.com/v/source.mp4"));
        assert!(urls.contains(&"https://player.vimeo.com/video/123"));
        assert!(
            urls.iter().any(|u| u.contains("master.m3u8?token=1")),
            "manifest bulunamadi: {urls:?}"
        );
        // manifestler onde
        assert_eq!(candidates[0].kind, "hls");
    }

    #[test]
    fn reklam_ve_veri_adresleri_atlanir() {
        let html = r#"
          <video src="data:video/mp4;base64,AAAA"></video>
          <video src="blob:https://site.example/123"></video>
          <a href="/reklam">reklam</a>
        "#;
        let (candidates, _) = extract_candidates(html, "https://site.example/");
        assert!(candidates.is_empty(), "beklenmeyen adaylar: {candidates:?}");
    }

    #[test]
    fn hls_varyantlari_cozulur() {
        let playlist = "#EXTM3U\n\
            #EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080,CODECS=\"avc1.640028,mp4a.40.2\"\n\
            1080/index.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=1200000,RESOLUTION=1280x720\n\
            https://cdn.example.com/720/index.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=400000\n\
            low/index.m3u8\n";
        let formats = parse_hls_variants(playlist, "https://cdn.example.com/master.m3u8");
        assert_eq!(formats.len(), 3);
        assert_eq!(formats[0].height, Some(1080));
        assert!(formats[0].label.contains("1080p"));
        assert_eq!(formats[0].id, "https://cdn.example.com/1080/index.m3u8");
        assert_eq!(formats[1].id, "https://cdn.example.com/720/index.m3u8");
        assert_eq!(formats[2].height, None);
    }

    #[test]
    fn tek_akis_playlisti() {
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\nseg0.ts\n";
        let formats = parse_hls_variants(playlist, "https://x/y/index.m3u8");
        assert_eq!(formats.len(), 1);
        assert_eq!(formats[0].id, "https://x/y/index.m3u8");
    }

    #[test]
    fn video_only_formata_ses_eklenir() {
        let list = vec![
            YtdlpFormat {
                format_id: "137".into(),
                ext: Some("mp4".into()),
                height: Some(1080),
                width: None,
                filesize: Some(50_000_000),
                filesize_approx: None,
                vcodec: Some("avc1".into()),
                acodec: Some("none".into()),
                format_note: Some("1080p".into()),
                tbr: None,
                protocol: None,
            },
            YtdlpFormat {
                format_id: "140".into(),
                ext: Some("m4a".into()),
                height: None,
                width: None,
                filesize: None,
                filesize_approx: Some(4_000_000),
                vcodec: Some("none".into()),
                acodec: Some("mp4a".into()),
                format_note: Some("medium".into()),
                tbr: None,
                protocol: None,
            },
            YtdlpFormat {
                format_id: "18".into(),
                ext: Some("mp4".into()),
                height: Some(360),
                width: None,
                filesize: None,
                filesize_approx: None,
                vcodec: Some("avc1".into()),
                acodec: Some("mp4a".into()),
                format_note: Some("360p".into()),
                tbr: None,
                protocol: None,
            },
        ];
        let formats = ytdlp_formats_to_formats(&list);
        let video = formats
            .iter()
            .find(|f| f.height == Some(1080))
            .expect("1080p yok");
        assert_eq!(
            video.id, "137+bestaudio/137",
            "video-only formata ses eklenmeli"
        );
        let audio = formats
            .iter()
            .find(|f| f.ext.as_deref() == Some("m4a"))
            .expect("ses yok");
        assert_eq!(audio.id, "140", "ses formatina dokunulmamali");
        let combined = formats
            .iter()
            .find(|f| f.height == Some(360))
            .expect("360p yok");
        assert_eq!(combined.id, "18", "sesli videoya dokunulmamali");
        // Siralama: 1080 -> 360 -> ses (height yok)
        assert_eq!(formats[0].height, Some(1080));
    }

    #[test]
    fn tur_tahmini() {
        assert_eq!(
            kind_for_source("magnet:?xt=urn:btih:a", None),
            Kind::Torrent
        );
        assert_eq!(
            kind_for_source("https://youtube.com/watch?v=a", None),
            Kind::Media
        );
        assert_eq!(
            kind_for_source("https://cdn.example.com/a.m3u8", None),
            Kind::Media
        );
        assert_eq!(
            kind_for_source("https://cdn.example.com/a.zip", None),
            Kind::Http
        );
        assert_eq!(
            kind_for_source("https://cdn.example.com/a.zip", Some(true)),
            Kind::Media
        );
    }

    #[test]
    fn boyut_bicimi() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(16 * 1024 * 1024), "16.0 MB");
    }

    #[test]
    fn medya_adi_basliktan_uretilir() {
        // Gercek sorun: YouTube'da liste "watch" gosteriyordu, baslik elde oldugu halde.
        let format = Format {
            id: "137+bestaudio/137".into(),
            label: "1080p · mp4".into(),
            ext: Some("mp4".into()),
            height: Some(1080),
            filesize: None,
            vcodec: Some("avc1".into()),
            acodec: Some("none".into()),
            note: Some("en iyi kalite".into()),
        };
        let name = media_filename_from_title(
            Some("Kimi K3.1 Pro'yu Canlı Zorluyoruz: Açık Model Nereye Kadar Gider?"),
            Some(&format),
        )
        .expect("ad uretilmedi");
        assert!(name.starts_with("Kimi K3.1"), "{name}");
        assert!(name.ends_with(".mp4"), "{name}");
        assert!(!name.contains(':'), "yasak karakter: {name}");

        // Baslik yoksa URL'den gelen ad kullanilir (None doner).
        assert_eq!(media_filename_from_title(None, Some(&format)), None);
        assert_eq!(media_filename_from_title(Some("   "), Some(&format)), None);

        // Uzanti bilinmiyorsa mp4 varsayilir.
        assert_eq!(
            media_filename_from_title(Some("Deneme videosu"), None).as_deref(),
            Some("Deneme videosu.mp4")
        );
    }
}
