//! HLS (`.m3u8`) indirme — yt-dlp olmadan.
//!
//! Kapsam bilinçli olarak dar tutuldu:
//! - **VOD medya playlist'i** (segment listesi) indirilir.
//! - **Canlı akış** (ENDLIST yok) desteklenir: playlist yoklanır, yeni segmentler
//!   geldikçe indirilir (ENDLIST / iptal / 6 saat sınırı).
//! - Master playlist verilirse en yüksek bant genişlikli varyant seçilir.
//! - `EXT-X-KEY` ile **AES-128** şifre çözme yapılır (standart HLS şifrelemesi).
//! - `SAMPLE-AES` / FairPlay / Widevine gibi **DRM yöntemleri reddedilir** — bu proje
//!   DRM kırmaz (bkz. README "Yasal not").
//! - `EXT-X-BYTERANGE` desteklenir: aynı dosyadan dilimler `Range` ile istenir ve
//!   offset verilmezse önceki parçanın bitişinden devam edilir.
//!
//! Her segment `parts/<id>/seg-00001.part` olarak yazılır. Dosya varsa segment
//! bitmiş sayılır: duraklatma/devam ve süreç yeniden başlaması bu sayede çalışır.

use super::fetch::fetch_bytes;
use super::http::RequestHeaders;
use super::{CancelToken, RateLimiter};
use futures_util::StreamExt;
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Aynı anda indirilecek segment sayısı.
pub const SEGMENT_CONCURRENCY: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsKey {
    /// Anahtarın mutlak adresi.
    pub uri: String,
    /// `IV=0x…` verilmişse; yoksa segment sırasından türetilir.
    pub iv: Option<[u8; 16]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsSegment {
    pub url: String,
    pub key: Option<HlsKey>,
    /// `EXT-X-BYTERANGE:<uzunluk>[@<baslangic>]` — ayni dosyadan bir dilim.
    /// `(offset, length)`. Yoksa segment dosyanin tamamidir.
    pub range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HlsPlaylist {
    pub segments: Vec<HlsSegment>,
    /// `EXT-X-MAP:URI="init.mp4"` — fMP4 akışlarında ilk parça.
    pub init: Option<String>,
    /// Canlı akış işareti (`#EXT-X-ENDLIST` yoksa true).
    pub live: bool,
}

/// Göreli adresi tabana göre çözer (`resolve_url` ile aynı davranış).
fn resolve(base: &str, reference: &str) -> Option<String> {
    let base = url::Url::parse(base).ok()?;
    let joined = base.join(reference.trim()).ok()?;
    Some(joined.to_string())
}

fn parse_iv(text: &str) -> Option<[u8; 16]> {
    let hex = text
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// `#EXT-X-KEY:METHOD=AES-128,URI="key.bin",IV=0x…` satırını çözer.
/// Desteklenmeyen yöntemlerde (DRM) açık hata döner.
fn parse_key(line: &str, base: &str) -> Result<Option<HlsKey>, String> {
    let rest = line
        .strip_prefix("#EXT-X-KEY:")
        .ok_or_else(|| "anahtar satırı değil".to_string())?;
    let mut method = String::new();
    let mut uri: Option<String> = None;
    let mut iv: Option<[u8; 16]> = None;

    // Değerler virgülle ayrılır; URI/IV tırnak içinde olabilir.
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in rest.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => parts.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }

    for part in parts {
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        match name.trim().to_ascii_uppercase().as_str() {
            "METHOD" => method = value.to_string(),
            "URI" => uri = Some(value.to_string()),
            "IV" => iv = parse_iv(value),
            _ => {}
        }
    }

    match method.to_ascii_uppercase().as_str() {
        "NONE" => Ok(None),
        "AES-128" => {
            let uri = uri.ok_or_else(|| "AES-128 anahtarı için URI yok".to_string())?;
            let resolved =
                resolve(base, &uri).ok_or_else(|| format!("anahtar adresi çözülemedi: {uri}"))?;
            Ok(Some(HlsKey { uri: resolved, iv }))
        }
        other => Err(format!(
            "Bu akış {other} ile şifrelenmiş (DRM). Hydra DL DRM korumalı içerik indirmez."
        )),
    }
}

/// Playlist çözümünün sonucu: master ise varyant adresi, değilse segment listesi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedPlaylist {
    Master(String),
    Media(HlsPlaylist),
}

/// Medya playlist'ini çözer. Master playlist verilirse en yüksek varyantın
/// adresini döndürür (indirmeyi çağıran taraf yapar).
pub fn parse_playlist(text: &str, base: &str) -> Result<ParsedPlaylist, String> {
    if text.contains("#EXT-X-STREAM-INF") {
        // Master: en yüksek bant genişliği (BANDWIDTH) olan varyantı seç.
        let mut best: Option<(u64, String)> = None;
        let mut pending: Option<u64> = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
                let bandwidth = rest
                    .split(',')
                    .find_map(|part| part.trim().strip_prefix("BANDWIDTH="))
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                pending = Some(bandwidth);
                continue;
            }
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(bandwidth) = pending.take() {
                if let Some(url) = resolve(base, line) {
                    if best.as_ref().map(|(bw, _)| bandwidth > *bw).unwrap_or(true) {
                        best = Some((bandwidth, url));
                    }
                }
            }
        }
        let (_, url) = best.ok_or_else(|| "master playlist'te varyant bulunamadı".to_string())?;
        return Ok(ParsedPlaylist::Master(url));
    }

    let mut segments = Vec::new();
    let mut key: Option<HlsKey> = None;
    let mut init: Option<String> = None;
    let mut ended = false;
    let mut pending_duration = false;
    // `EXT-X-BYTERANGE` bir sonraki segment satirina uygulanir: (acik offset?, uzunluk).
    let mut pending_range: Option<(Option<u64>, u64)> = None;
    // Offset verilmemis BYTERANGE onceki parcanin bitisinden devam eder; bu yalniz
    // ayni dosya icinde anlamlidir, dosya degisince sifirlanir.
    let mut previous_end: u64 = 0;
    let mut previous_url: Option<String> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            key = parse_key(&format!("#EXT-X-KEY:{rest}"), base)?;
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            if let Some(uri) = rest
                .split(',')
                .find_map(|part| part.trim().strip_prefix("URI="))
                .map(|value| value.trim_matches('"'))
            {
                init = resolve(base, uri);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            // `<uzunluk>[@<baslangic>]`; baslangic yoksa onceki segmentin
            // bitisinden devam eder (RFC 8216 §4.3.2.2).
            pending_range = parse_byterange(rest);
            continue;
        }
        if line == "#EXT-X-ENDLIST" {
            ended = true;
            continue;
        }
        if line.starts_with("#EXTINF") {
            pending_duration = true;
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        // Segment adresi: bir önceki satır #EXTINF olmalı.
        if !pending_duration {
            continue;
        }
        pending_duration = false;
        let Some(url) = resolve(base, line) else {
            continue;
        };
        // Offset verilmemis BYTERANGE: onceki segmentin bitisinden devam
        // (yalniz ayni URL icinde; farkli dosyaya gecince bastan).
        let range = pending_range.take().map(|(offset, length)| {
            let start = match offset {
                Some(value) => value,
                None => {
                    if previous_url.as_deref() == Some(url.as_str()) {
                        previous_end
                    } else {
                        0
                    }
                }
            };
            (start, length)
        });
        if let Some((offset, length)) = range {
            previous_end = offset.saturating_add(length);
            previous_url = Some(url.clone());
        } else {
            previous_url = None;
            previous_end = 0;
        }
        segments.push(HlsSegment {
            url,
            key: key.clone(),
            range,
        });
    }

    if segments.is_empty() {
        return Err("Playlist'te segment bulunamadı (canlı akış olabilir).".into());
    }
    Ok(ParsedPlaylist::Media(HlsPlaylist {
        segments,
        init,
        live: !ended,
    }))
}

/// `"<uzunluk>[@<baslangic>]"` → `(acik offset?, uzunluk)`. Bozuksa `None`.
fn parse_byterange(text: &str) -> Option<(Option<u64>, u64)> {
    let raw = text.trim();
    let (length_text, offset_text) = match raw.split_once('@') {
        Some((len, offset)) => (len, Some(offset)),
        None => (raw, None),
    };
    let length: u64 = length_text.trim().parse().ok()?;
    if length == 0 {
        return None;
    }
    let offset = match offset_text {
        Some(value) => Some(value.trim().parse::<u64>().ok()?),
        None => None,
    };
    Some((offset, length))
}

/// AES-128-CBC + PKCS7 şifre çözme.
pub fn decrypt_aes128(data: &[u8], key: &[u8], iv: &[u8; 16]) -> Result<Vec<u8>, String> {
    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    type Decryptor = cbc::Decryptor<aes::Aes128>;

    if key.len() != 16 {
        return Err(format!(
            "AES-128 anahtarı 16 bayt olmalı, {} geldi",
            key.len()
        ));
    }
    if data.is_empty() || !data.len().is_multiple_of(16) {
        return Err(format!(
            "Şifreli veri 16'nın katı olmalı, {} bayt geldi",
            data.len()
        ));
    }
    let mut buffer = data.to_vec();
    let plain = Decryptor::new(key.into(), iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|_| "Şifre çözme başarısız (anahtar/IV hatalı olabilir)".to_string())?;
    Ok(plain.to_vec())
}

/// Segment sırasından türetilen IV (HLS belirtimi: 128-bit big-endian sayaç).
pub fn iv_from_sequence(sequence: u64) -> [u8; 16] {
    let mut iv = [0u8; 16];
    iv[8..].copy_from_slice(&sequence.to_be_bytes());
    iv
}

fn part_path(parts_dir: &Path, index: usize) -> PathBuf {
    parts_dir.join(format!("seg-{index:05}.part"))
}

#[derive(Debug)]
pub enum HlsError {
    Cancelled,
    Message(String),
}

impl std::fmt::Display for HlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HlsError::Cancelled => write!(f, "İndirme iptal edildi"),
            HlsError::Message(text) => write!(f, "{text}"),
        }
    }
}

struct SegmentJob {
    index: usize,
    segment: HlsSegment,
    /// Bu segmentte kullanılacak IV (açık IV yoksa sıra numarası).
    iv: [u8; 16],
}

/// Playlist ön kontrolü: indirme eklerken segment sayısını ve türünü öğrenmek için.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsProbe {
    pub segments: usize,
    pub live: bool,
    /// Birleştirilmiş dosyanın uzantısı (`ts` veya `mp4`).
    pub ext: &'static str,
}

/// Playlist'i (ve gerekiyorsa varyantını) indirip özet çıkarır.
pub async fn probe_playlist(
    client: &Client,
    playlist_url: &str,
    headers: RequestHeaders<'_>,
) -> Result<HlsProbe, String> {
    let mut url = playlist_url.to_string();
    for _ in 0..3 {
        let bytes = fetch_bytes(client, &url, headers, Some(8 * 1024 * 1024)).await?;
        let text = String::from_utf8_lossy(&bytes).to_string();
        match parse_playlist(&text, &url)? {
            ParsedPlaylist::Master(variant) => url = variant,
            ParsedPlaylist::Media(list) => {
                let ext = if list
                    .segments
                    .first()
                    .map(|seg| {
                        let lower = seg.url.to_ascii_lowercase();
                        lower.contains(".m4s") || lower.contains(".mp4")
                    })
                    .unwrap_or(false)
                {
                    "mp4"
                } else {
                    "ts"
                };
                return Ok(HlsProbe {
                    segments: list.segments.len(),
                    live: list.live,
                    ext,
                });
            }
        }
    }
    Err("Playlist varyantı çözülemedi (iç içe master listesi).".into())
}

/// Playlist'i indirir ve segmentleri paralel çeker, sıralı birleştirir.
///
/// Dönen değer yazılan toplam bayt. `on_progress(done, total)` her biten
/// segmentte çağrılır (toplam segment sayısı bilindiği için yüzde hesaplanabilir).
#[allow(clippy::too_many_arguments)]
pub async fn download_playlist(
    client: &Client,
    playlist_url: &str,
    headers: RequestHeaders<'_>,
    parts_dir: &Path,
    cancel: &CancelToken,
    limiter: &RateLimiter,
    on_progress: impl Fn(usize, usize, u64) + Send + Sync,
) -> Result<u64, HlsError> {
    tokio::fs::create_dir_all(parts_dir)
        .await
        .map_err(|e| HlsError::Message(format!("klasör oluşturulamadı: {e}")))?;

    // Master playlist ise en yüksek varyantı takip et (en fazla 2 seviye).
    let mut playlist_url = playlist_url.to_string();
    let mut media: Option<HlsPlaylist> = None;
    for _ in 0..3 {
        let text = fetch_bytes(client, &playlist_url, headers, Some(8 * 1024 * 1024))
            .await
            .map_err(HlsError::Message)?;
        let text = String::from_utf8_lossy(&text).to_string();
        match parse_playlist(&text, &playlist_url).map_err(HlsError::Message)? {
            ParsedPlaylist::Master(variant) => playlist_url = variant,
            ParsedPlaylist::Media(list) => {
                media = Some(list);
                break;
            }
        }
    }
    let playlist = media.ok_or_else(|| {
        HlsError::Message("Playlist varyantı çözülemedi (iç içe master listesi).".into())
    })?;

    // Canlı akış: playlist'i yoklayıp yeni segmentleri geldikçe indir.
    // (`#EXT-X-ENDLIST` görülünce ya da kullanıcı iptal edince durur.)
    let total = if playlist.live {
        download_live(
            client,
            &playlist_url,
            headers,
            parts_dir,
            cancel,
            limiter,
            &on_progress,
        )
        .await?
    } else {
        // Şifreleme anahtarlarını bir kez çek (aynı URI tekrar indirilmesin).
        let mut keys: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
        for segment in &playlist.segments {
            let Some(key) = &segment.key else { continue };
            if keys.contains_key(&key.uri) {
                continue;
            }
            let bytes = fetch_bytes(client, &key.uri, headers, Some(1024))
                .await
                .map_err(HlsError::Message)?;
            if bytes.len() != 16 {
                return Err(HlsError::Message(format!(
                    "AES anahtarı 16 bayt olmalı, {} geldi",
                    bytes.len()
                )));
            }
            keys.insert(key.uri.clone(), bytes);
        }

        // fMP4 init parçası (varsa) ilk sıraya yazılır.
        if let Some(init) = &playlist.init {
            let target = parts_dir.join("seg-init.part");
            let bytes = fetch_bytes(client, init, headers, Some(32 * 1024 * 1024))
                .await
                .map_err(HlsError::Message)?;
            tokio::fs::write(&target, &bytes)
                .await
                .map_err(|e| HlsError::Message(format!("init yazılamadı: {e}")))?;
        }

        let jobs: Vec<SegmentJob> = playlist
            .segments
            .iter()
            .enumerate()
            .map(|(index, segment)| SegmentJob {
                index,
                iv: segment
                    .key
                    .as_ref()
                    .and_then(|key| key.iv)
                    .unwrap_or_else(|| iv_from_sequence(index as u64)),
                segment: segment.clone(),
            })
            .collect();

        let total = jobs.len();
        let done = Arc::new(AtomicU64::new(0));
        let written = Arc::new(AtomicU64::new(0));
        let on_progress = Arc::new(on_progress);
        // Biten segment sayısı için ilerleme: diskteki mevcut dosyalar da sayılır.
        let already: usize = {
            let mut count = 0;
            for job in &jobs {
                if tokio::fs::metadata(part_path(parts_dir, job.index))
                    .await
                    .map(|m| m.len() > 0)
                    .unwrap_or(false)
                {
                    count += 1;
                }
            }
            count
        };
        on_progress(already, total, 0);
        done.store(already as u64, Ordering::Relaxed);

        let outcome = futures_util::stream::iter(jobs.into_iter().map(|job| {
            let client = client.clone();
            let parts_dir = parts_dir.to_path_buf();
            let cancel: &CancelToken = cancel;
            let limiter: &RateLimiter = limiter;
            let done = done.clone();
            let written = written.clone();
            let on_progress = on_progress.clone();
            let keys = keys.clone();
            async move {
                if cancel.is_cancelled() {
                    return Err(HlsError::Cancelled);
                }
                let target = part_path(&parts_dir, job.index);
                if tokio::fs::metadata(&target)
                    .await
                    .map(|m| m.len() > 0)
                    .unwrap_or(false)
                {
                    return Ok(());
                }
                // BYTERANGE varsa ayni dosyadan yalniz o dilim istenir.
                let raw = match job.segment.range {
                    Some((offset, length)) => crate::engine::fetch::fetch_range_end(
                        &client,
                        &job.segment.url,
                        headers,
                        offset,
                        length,
                        Some(256 * 1024 * 1024),
                    )
                    .await
                    .map_err(HlsError::Message)?,
                    None => {
                        fetch_bytes(&client, &job.segment.url, headers, Some(256 * 1024 * 1024))
                            .await
                            .map_err(HlsError::Message)?
                    }
                };
                limiter.consume(raw.len() as u64).await;
                let plain = match &job.segment.key {
                    Some(key) => {
                        let material = keys
                            .get(&key.uri)
                            .ok_or_else(|| HlsError::Message("anahtar bulunamadı".to_string()))?;
                        decrypt_aes128(&raw, material, &job.iv).map_err(HlsError::Message)?
                    }
                    None => raw,
                };
                if cancel.is_cancelled() {
                    return Err(HlsError::Cancelled);
                }
                // Yarım dosya "tamam" sayılmasın: önce geçici ada yaz, sonra taşı.
                let temp = target.with_extension("tmp");
                tokio::fs::write(&temp, &plain)
                    .await
                    .map_err(|e| HlsError::Message(format!("segment yazılamadı: {e}")))?;
                tokio::fs::rename(&temp, &target)
                    .await
                    .map_err(|e| HlsError::Message(format!("segment taşınamadı: {e}")))?;
                let finished = done.fetch_add(1, Ordering::Relaxed) as usize + 1;
                let so_far =
                    written.fetch_add(plain.len() as u64, Ordering::Relaxed) + plain.len() as u64;
                on_progress(finished, total, so_far);
                Ok::<(), HlsError>(())
            }
        }))
        .buffer_unordered(SEGMENT_CONCURRENCY)
        .collect::<Vec<_>>();

        for result in outcome.await {
            result?;
        }

        total
    };

    // Sıralı birleştirme (VOD ve canlı akış için ortak).
    let mut total_bytes = 0u64;
    let mut file = tokio::fs::File::create(parts_dir.join("hls.part"))
        .await
        .map_err(|e| HlsError::Message(format!("çıktı oluşturulamadı: {e}")))?;
    use tokio::io::AsyncWriteExt;
    let init_path = parts_dir.join("seg-init.part");
    if tokio::fs::metadata(&init_path).await.is_ok() {
        let bytes = tokio::fs::read(&init_path)
            .await
            .map_err(|e| HlsError::Message(format!("init okunamadı: {e}")))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| HlsError::Message(format!("init yazılamadı: {e}")))?;
        total_bytes += bytes.len() as u64;
    }
    for index in 0..total {
        let path = part_path(parts_dir, index);
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| HlsError::Message(format!("{}. segment okunamadı: {e}", index + 1)))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| HlsError::Message(format!("birleştirme hatası: {e}")))?;
        total_bytes += bytes.len() as u64;
    }
    file.flush()
        .await
        .map_err(|e| HlsError::Message(format!("diske yazılamadı: {e}")))?;
    Ok(total_bytes)
}

/// Tek bir segmenti indirir, sifresini cozer (VOD ve canli akis ortak).
async fn fetch_segment_plain(
    client: &Client,
    segment: &HlsSegment,
    headers: RequestHeaders<'_>,
    keys: &std::collections::HashMap<String, Vec<u8>>,
    index: usize,
) -> Result<Vec<u8>, HlsError> {
    let raw = match segment.range {
        Some((offset, length)) => crate::engine::fetch::fetch_range_end(
            client,
            &segment.url,
            headers,
            offset,
            length,
            Some(256 * 1024 * 1024),
        )
        .await
        .map_err(HlsError::Message)?,
        None => fetch_bytes(client, &segment.url, headers, Some(256 * 1024 * 1024))
            .await
            .map_err(HlsError::Message)?,
    };
    match &segment.key {
        Some(key) => {
            let material = keys
                .get(&key.uri)
                .ok_or_else(|| HlsError::Message("anahtar bulunamadı".to_string()))?;
            let iv = key.iv.unwrap_or_else(|| iv_from_sequence(index as u64));
            decrypt_aes128(&raw, material, &iv).map_err(HlsError::Message)
        }
        None => Ok(raw),
    }
}

/// Canli akis: playlist'i yoklar, yeni segmentleri geldikce indirir.
///
/// Durma kosullari: ENDLIST goruldu, kullanici iptal etti, ya da guvenlik
/// siniri (6 saat / 20 000 segment) asildi. Donen deger yazilan segment sayisi.
#[allow(clippy::too_many_arguments)]
async fn download_live(
    client: &Client,
    playlist_url: &str,
    headers: RequestHeaders<'_>,
    parts_dir: &Path,
    cancel: &CancelToken,
    limiter: &RateLimiter,
    on_progress: &(impl Fn(usize, usize, u64) + Send + Sync),
) -> Result<usize, HlsError> {
    /// Yoklama araligi: segment suresi genelde 2-6 sn; 2 sn iyi bir denge.
    const POLL_INTERVAL: Duration = Duration::from_secs(2);
    /// Canli kaydin guvenlik siniri: 6 saat.
    const MAX_LIVE_SECONDS: u64 = 6 * 3600;
    // Ayni segment tekrar indirilmesin: (url, aralik) kumesi.
    let mut seen: std::collections::HashSet<(String, Option<(u64, u64)>)> =
        std::collections::HashSet::new();
    let mut keys: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    let mut index = 0usize;
    let mut written_bytes = 0u64;
    let started = std::time::Instant::now();

    loop {
        if cancel.is_cancelled() {
            return Err(HlsError::Cancelled);
        }
        if started.elapsed().as_secs() > MAX_LIVE_SECONDS {
            return Err(HlsError::Message(
                "Canlı kayıt süresi sınırına ulaşıldı (6 saat).".into(),
            ));
        }

        let bytes = fetch_bytes(client, playlist_url, headers, Some(8 * 1024 * 1024))
            .await
            .map_err(HlsError::Message)?;
        let text = String::from_utf8_lossy(&bytes).to_string();
        let ParsedPlaylist::Media(list) =
            parse_playlist(&text, playlist_url).map_err(HlsError::Message)?
        else {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        };

        for segment in &list.segments {
            let key = (segment.url.clone(), segment.range);
            if !seen.insert(key) {
                continue;
            }
            if let Some(k) = &segment.key {
                if !keys.contains_key(&k.uri) {
                    let material = fetch_bytes(client, &k.uri, headers, Some(1024))
                        .await
                        .map_err(HlsError::Message)?;
                    if material.len() != 16 {
                        return Err(HlsError::Message(format!(
                            "AES anahtarı 16 bayt olmalı, {} geldi",
                            material.len()
                        )));
                    }
                    keys.insert(k.uri.clone(), material);
                }
            }
            let plain = fetch_segment_plain(client, segment, headers, &keys, index).await?;
            limiter.consume(plain.len() as u64).await;
            if cancel.is_cancelled() {
                return Err(HlsError::Cancelled);
            }
            let target = part_path(parts_dir, index);
            let temp = target.with_extension("tmp");
            tokio::fs::write(&temp, &plain)
                .await
                .map_err(|e| HlsError::Message(format!("segment yazılamadı: {e}")))?;
            tokio::fs::rename(&temp, &target)
                .await
                .map_err(|e| HlsError::Message(format!("segment taşınamadı: {e}")))?;
            index += 1;
            written_bytes += plain.len() as u64;
            // Toplam bilinmiyor (canli): 0 gec, arayuz yuzde yerine bayt gosterir.
            on_progress(index, 0, written_bytes);
            if index >= 20_000 {
                return Ok(index);
            }
        }

        if !list.live {
            return Ok(index);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// HLS indirmesi için zaman aşımı tanımlı istemci (segmentler kısa sürer).
#[allow(dead_code)] // arayüz/ileride kullanım için sabit; şimdilik bilgi amaçlı.
pub fn segment_timeout() -> Duration {
    Duration::from_secs(60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canlı akış: playlist her istekte büyür, bitince ENDLIST gelir.
    /// İndirme yeni segmentleri bir kez almalı ve sırayla birleştirmeli.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn canli_akis_yeni_segmentleri_indirir() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let hits = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits_server = hits.clone();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let hits = hits_server.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();

                    let (body, content_type) = if path.ends_with(".m3u8") {
                        let round = hits.fetch_add(1, Ordering::SeqCst) + 1;
                        // Tur 1: seg1, Tur 2: seg1+seg2, Tur 3: seg1..seg3 + ENDLIST
                        let mut text = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");
                        for index in 1..=round.min(3) {
                            text.push_str(&format!("#EXTINF:2.0,\nhttp://{addr}/seg{index}.ts\n"));
                        }
                        if round >= 3 {
                            text.push_str("#EXT-X-ENDLIST\n");
                        }
                        (text, "application/vnd.apple.mpegurl")
                    } else {
                        // Segment gövdesi: adından türetilmiş sabit 16 bayt.
                        let index = path
                            .trim_start_matches("/seg")
                            .trim_end_matches(".ts")
                            .parse::<u8>()
                            .unwrap_or(0);
                        (String::from_utf8(vec![index; 16]).unwrap(), "video/mp2t")
                    };

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.write_all(body.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        let playlist_url = format!("http://{addr}/live.m3u8");
        let client = crate::engine::http::build_client();
        let dir = tempfile::tempdir().unwrap();
        let parts = dir.path().join("parts");
        let cancel = CancelToken::new();
        let limiter = RateLimiter::new(0);

        let bytes = download_playlist(
            &client,
            &playlist_url,
            RequestHeaders::default(),
            &parts,
            &cancel,
            &limiter,
            |_done, _total, _bytes| {},
        )
        .await
        .expect("canli akis indirilmeli");

        // 3 segment x 16 bayt; ayni segment iki kez indirilmemeli.
        assert_eq!(bytes, 48, "uc segment birlesmeli, tekrarsiz");
        let merged = tokio::fs::read(parts.join("hls.part")).await.unwrap();
        assert_eq!(merged.len(), 48);
        assert_eq!(&merged[0..16], &vec![1u8; 16][..], "sira korunmali (seg1)");
        assert_eq!(&merged[16..32], &vec![2u8; 16][..], "sira korunmali (seg2)");
        assert_eq!(&merged[32..48], &vec![3u8; 16][..], "sira korunmali (seg3)");
        assert!(hits.load(Ordering::SeqCst) >= 3, "playlist yoklanmali");
    }

    const MASTER: &str = "#EXTM3U\n\
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\n\
low/index.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080\n\
high/index.m3u8\n";

    fn media(text: &str, base: &str) -> HlsPlaylist {
        match parse_playlist(text, base) {
            Ok(ParsedPlaylist::Media(list)) => list,
            other => panic!("medya playlist bekleniyordu: {other:?}"),
        }
    }

    #[test]
    fn master_playlist_en_yuksek_varyanti_secer() {
        match parse_playlist(MASTER, "https://cdn.example.com/master.m3u8") {
            Ok(ParsedPlaylist::Master(url)) => {
                assert_eq!(url, "https://cdn.example.com/high/index.m3u8")
            }
            other => panic!("master bekleniyordu: {other:?}"),
        }
    }

    #[test]
    fn medya_playlist_segmentleri_cozulur() {
        let text = "#EXTM3U\n\
#EXT-X-VERSION:3\n\
#EXT-X-TARGETDURATION:6\n\
#EXTINF:6.0,\n\
seg-1.ts\n\
#EXTINF:6.0,\n\
https://other.example.com/seg-2.ts\n\
#EXTINF:4.0,\n\
../son/seg-3.ts\n\
#EXT-X-ENDLIST\n";
        let list = media(text, "https://cdn.example.com/v/1/index.m3u8");
        assert_eq!(list.segments.len(), 3);
        assert!(list.segments[0].url.ends_with("/v/1/seg-1.ts"));
        assert_eq!(list.segments[1].url, "https://other.example.com/seg-2.ts");
        assert!(list.segments[2].url.ends_with("/v/son/seg-3.ts"));
        assert!(!list.live, "ENDLIST varsa canli degil");
        assert!(list.segments.iter().all(|s| s.key.is_none()));
    }

    #[test]
    fn endlist_yoksa_canli_isaretlenir() {
        let text = "#EXTM3U\n#EXTINF:6.0,\na.ts\n";
        assert!(media(text, "https://x/y.m3u8").live);
    }

    #[test]
    fn aes128_anahtari_ve_iv_cozulur() {
        let text = "#EXTM3U\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\",IV=0x000102030405060708090a0b0c0d0e0f\n\
#EXTINF:6.0,\n\
a.ts\n\
#EXT-X-ENDLIST\n";
        let list = media(text, "https://cdn.example.com/x/index.m3u8");
        let key = list.segments[0].key.as_ref().expect("anahtar var");
        assert_eq!(key.uri, "https://cdn.example.com/x/key.bin");
        assert_eq!(
            key.iv,
            Some([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
        );
    }

    #[test]
    fn drm_yontemi_reddedilir() {
        for method in ["SAMPLE-AES", "SAMPLE-AES-CTR", "ISO-23001-7"] {
            let text = format!(
                "#EXTM3U\n#EXT-X-KEY:METHOD={method},URI=\"skd://x\"\n#EXTINF:6,\na.ts\n#EXT-X-ENDLIST\n"
            );
            let err = parse_playlist(&text, "https://x/y.m3u8").expect_err("DRM reddedilmeli");
            assert!(err.contains("DRM"), "beklenen DRM mesaji, gelen: {err}");
            assert!(err.contains(method), "yontem adi mesajda: {err}");
        }
    }

    #[test]
    fn method_none_sifrelemeyi_kaldirir() {
        let text = "#EXTM3U\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"k.bin\"\n\
#EXTINF:6,\na.ts\n\
#EXT-X-KEY:METHOD=NONE\n\
#EXTINF:6,\nb.ts\n\
#EXT-X-ENDLIST\n";
        let list = media(text, "https://x/y.m3u8");
        assert!(list.segments[0].key.is_some());
        assert!(
            list.segments[1].key.is_none(),
            "METHOD=NONE anahtari kaldirir"
        );
    }

    #[test]
    fn byterange_parcalanir() {
        // Ayni dosyadan uc bolum: ikisinde offset acik, ucuncude devam eder.
        let text = "#EXTM3U\n\
#EXT-X-BYTERANGE:1000@0\n\
#EXTINF:6,\n\
a.ts\n\
#EXT-X-BYTERANGE:2000@1000\n\
#EXTINF:6,\n\
a.ts\n\
#EXT-X-BYTERANGE:500\n\
#EXTINF:6,\n\
a.ts\n\
#EXT-X-ENDLIST\n";
        let ParsedPlaylist::Media(list) = parse_playlist(text, "https://x/y.m3u8").unwrap() else {
            panic!("medya listesi bekleniyordu");
        };
        assert_eq!(list.segments.len(), 3);
        assert_eq!(list.segments[0].range, Some((0, 1000)));
        assert_eq!(list.segments[1].range, Some((1000, 2000)));
        assert_eq!(
            list.segments[2].range,
            Some((3000, 500)),
            "offset yoksa onceki parcanin bitisinden devam eder"
        );
    }

    #[test]
    fn byterange_olmayan_segmentte_aralik_yok() {
        let text = "#EXTM3U\n#EXTINF:6,\na.ts\n#EXT-X-ENDLIST\n";
        let ParsedPlaylist::Media(list) = parse_playlist(text, "https://x/y.m3u8").unwrap() else {
            panic!("medya listesi bekleniyordu");
        };
        assert_eq!(list.segments[0].range, None);
    }

    #[test]
    fn byterange_bozuk_deger_yok_sayilir() {
        let text = "#EXTM3U\n#EXT-X-BYTERANGE:abc\n#EXTINF:6,\na.ts\n#EXT-X-ENDLIST\n";
        let ParsedPlaylist::Media(list) = parse_playlist(text, "https://x/y.m3u8").unwrap() else {
            panic!("medya listesi bekleniyordu");
        };
        assert_eq!(list.segments[0].range, None, "bozuk deger aralik uretmez");
    }

    #[test]
    fn ext_x_map_init_parcasi_cozulur() {
        let text = "#EXTM3U\n\
#EXT-X-MAP:URI=\"init.mp4\"\n\
#EXTINF:6,\n\
a.m4s\n\
#EXT-X-ENDLIST\n";
        let list = media(text, "https://x/v/index.m3u8");
        assert_eq!(list.init.as_deref(), Some("https://x/v/init.mp4"));
        assert_eq!(list.segments.len(), 1);
    }

    #[test]
    fn iv_sequence_buyuk_sonlu_yazilir() {
        assert_eq!(iv_from_sequence(0), [0u8; 16]);
        let iv = iv_from_sequence(0x0102);
        assert_eq!(&iv[14..], &[0x01, 0x02]);
        assert_eq!(&iv[..14], &[0u8; 14]);
    }

    /// AES-128-CBC/PKCS7 gidiş-dönüş: şifrele → çöz → aynı düz metin.
    /// (Şifreleme yalnız testte; uygulama sadece çözer.)
    #[test]
    fn aes128_pkcs7_gidis_donus() {
        use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        let key = [0x11u8; 16];
        let iv = [0x22u8; 16];
        let plain = b"attack at dawn, Hydra cok basli".to_vec();
        let cipher = cbc::Encryptor::<aes::Aes128>::new(&key.into(), &iv.into())
            .encrypt_padded_vec_mut::<Pkcs7>(&plain);
        let out = decrypt_aes128(&cipher, &key, &iv).unwrap();
        assert_eq!(out, plain);
        assert_ne!(cipher, plain, "sifreli metin duz metinden farkli");
    }

    /// NIST SP 800-38A F.2.1 (CBC-AES128) ilk blok vektörü, PKCS7 dolgusuyla
    /// tamamlanmış tek blok: son blok düz metin "6bc1…172a" + dolgu.
    #[test]
    fn aes128_nist_vektoru_ilk_blok() {
        let key: [u8; 16] = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let iv: [u8; 16] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ];
        use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        // NIST ilk blok düz metni; PKCS7 için 16 baytlık blok tam dolgu ister.
        let block: [u8; 16] = [
            0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93,
            0x17, 0x2a,
        ];
        let buffer = block.to_vec();
        let cipher = cbc::Encryptor::<aes::Aes128>::new(&key.into(), &iv.into())
            .encrypt_padded_vec_mut::<Pkcs7>(&buffer);
        // Vektörün ilk 16 baytı NIST ile aynı olmalı (kanıt: doğru AES-CBC).
        assert_eq!(
            &cipher[..16],
            &[
                0x76, 0x49, 0xab, 0xac, 0x81, 0x19, 0xb2, 0x46, 0xce, 0xe9, 0x8e, 0x9b, 0x12, 0xe9,
                0x19, 0x7d
            ]
        );
        assert_eq!(decrypt_aes128(&cipher, &key, &iv).unwrap(), block.to_vec());
    }

    #[test]
    fn aes128_hatali_veri_reddedilir() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        assert!(decrypt_aes128(&[], &key, &iv).is_err(), "bos veri");
        assert!(
            decrypt_aes128(&[0u8; 17], &key, &iv).is_err(),
            "16'nin kati olmayan veri"
        );
        assert!(
            decrypt_aes128(&[0u8; 16], &[0u8; 8], &iv).is_err(),
            "kisa anahtar"
        );
    }
}
