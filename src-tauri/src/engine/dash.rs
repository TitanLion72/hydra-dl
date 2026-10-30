//! DASH indirme motoru: manifest → segmentler → birleşik dosya.
//!
//! `crate::dash` manifesti saf olarak çözer; bu modül ağ işini yapar.
//! Ses ve video ayrı akışlardaysa **iki dosya** üretilir (birleştirme ffmpeg
//! gerektirir); çağıran taraf bunu kullanıcıya söyler.

use super::fetch::{fetch_bytes, fetch_range};
use super::http::RequestHeaders;
use super::{CancelToken, RateLimiter};
use crate::dash::{self as manifest, DashTrack, TrackKind};
use futures_util::StreamExt;
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Aynı anda indirilecek segment sayısı.
pub const SEGMENT_CONCURRENCY: usize = 6;
/// Tek segment için üst sınır (bozuk manifest diskleri doldurmasın).
const MAX_SEGMENT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug)]
pub enum DashError {
    Cancelled,
    Message(String),
}

impl std::fmt::Display for DashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DashError::Cancelled => write!(f, "İndirme iptal edildi"),
            DashError::Message(text) => write!(f, "{text}"),
        }
    }
}

/// Manifest özeti: kaç akış, kaç segment, tahmini boyut yok (bilinmiyor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashProbe {
    pub video_id: Option<String>,
    pub audio_id: Option<String>,
    pub segments: usize,
    pub height: Option<u64>,
    /// Ses ayrı akışta mı? (kullanıcıya söylenir)
    pub separate_audio: bool,
}

/// MPD'yi indirip seçilecek akışları raporlar.
pub async fn probe(
    client: &Client,
    mpd_url: &str,
    headers: RequestHeaders<'_>,
) -> Result<DashProbe, String> {
    let parsed = load_manifest(client, mpd_url, headers).await?;
    let (video, audio) = manifest::pick_tracks(&parsed);
    Ok(DashProbe {
        video_id: video.as_ref().map(|track| track.id.clone()),
        audio_id: audio.as_ref().map(|track| track.id.clone()),
        segments: video
            .as_ref()
            .map(|track| track.segments.len())
            .unwrap_or(0),
        height: video.as_ref().and_then(|track| track.height),
        separate_audio: video.is_some() && audio.is_some(),
    })
}

async fn load_manifest(
    client: &Client,
    mpd_url: &str,
    headers: RequestHeaders<'_>,
) -> Result<manifest::DashManifest, String> {
    let bytes = fetch_bytes(client, mpd_url, headers, Some(16 * 1024 * 1024)).await?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    manifest::parse_mpd(&text, mpd_url)
}

/// Bir akışı indirir: init + segmentler → `output` dosyası.
async fn download_track(
    client: &Client,
    track: &DashTrack,
    headers: RequestHeaders<'_>,
    output: &Path,
    cancel: &CancelToken,
    limiter: &RateLimiter,
    progress: &(impl Fn(usize, usize, u64) + Send + Sync),
) -> Result<u64, DashError> {
    use tokio::io::AsyncWriteExt;

    let temp = output.with_extension("part");
    let mut file = tokio::fs::File::create(&temp)
        .await
        .map_err(|e| DashError::Message(format!("dosya oluşturulamadı: {e}")))?;
    let mut total = 0u64;

    // Init parçası (fMP4 başlığı) önce yazılır.
    if let Some(init) = &track.init {
        let bytes = fetch_range(
            client,
            &init.url,
            headers,
            init.range.unwrap_or((0, None)),
            None,
        )
        .await
        .map_err(DashError::Message)?;
        limiter.consume(bytes.len() as u64).await;
        file.write_all(&bytes)
            .await
            .map_err(|e| DashError::Message(format!("init yazılamadı: {e}")))?;
        total += bytes.len() as u64;
    }

    let done = Arc::new(AtomicU64::new(0));
    let written = Arc::new(AtomicU64::new(0));
    let count = track.segments.len();

    // Segmentler paralel indirilir, ama **sırayla** diske yazılır: DASH segmentleri
    // birbirine bağımlı olabilir, sıra bozulursa dosya bozulur.
    let mut pending: std::collections::BTreeMap<usize, Vec<u8>> = std::collections::BTreeMap::new();
    let mut next_to_write = 0usize;

    // İş listesi sahiplenilmiş (owned): async bloklarda ödünç referans taşımıyoruz.
    // Aksi hâlde `&PathBuf`/`&RequestHeaders` yakalanınca future `Send` olmuyor.
    let jobs: Vec<(usize, String, Option<crate::dash::ByteRange>)> = track
        .segments
        .iter()
        .enumerate()
        .map(|(index, segment)| (index, segment.url.clone(), segment.range))
        .collect();
    let referer = headers.referer.map(|value| value.to_string());
    let cookie = headers.cookie.map(|value| value.to_string());

    let results = futures_util::stream::iter(jobs.into_iter().map(|(index, url, range)| {
        let client = client.clone();
        let range = range.unwrap_or((0, None));
        let cancel: &CancelToken = cancel;
        let limiter: &RateLimiter = limiter;
        let done = done.clone();
        let written = written.clone();
        let referer = referer.clone();
        let cookie = cookie.clone();
        let progress = &progress;
        async move {
            if cancel.is_cancelled() {
                return Err(DashError::Cancelled);
            }
            let headers = RequestHeaders::new(referer.as_deref(), cookie.as_deref());
            let bytes = fetch_range(&client, &url, headers, range, Some(MAX_SEGMENT_BYTES))
                .await
                .map_err(DashError::Message)?;
            limiter.consume(bytes.len() as u64).await;
            let finished = done.fetch_add(1, Ordering::Relaxed) as usize + 1;
            let so_far =
                written.fetch_add(bytes.len() as u64, Ordering::Relaxed) + bytes.len() as u64;
            progress(finished, count, so_far);
            Ok::<(usize, Vec<u8>), DashError>((index, bytes))
        }
    }))
    .buffer_unordered(SEGMENT_CONCURRENCY);

    let mut stream = Box::pin(results);
    while let Some(result) = stream.next().await {
        let (index, bytes) = result?;
        pending.insert(index, bytes);
        // Yazılabildiği kadarını sırayla yaz.
        while let Some(chunk) = pending.remove(&next_to_write) {
            file.write_all(&chunk)
                .await
                .map_err(|e| DashError::Message(format!("segment yazılamadı: {e}")))?;
            total += chunk.len() as u64;
            next_to_write += 1;
        }
    }
    file.flush()
        .await
        .map_err(|e| DashError::Message(format!("diske yazılamadı: {e}")))?;
    drop(file);
    tokio::fs::rename(&temp, output)
        .await
        .map_err(|e| DashError::Message(format!("dosya taşınamadı: {e}")))?;
    Ok(total)
}

/// İndirme sonucu: oluşan dosyalar (video önce).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashOutput {
    pub video: Option<PathBuf>,
    pub audio: Option<PathBuf>,
    pub video_bytes: u64,
    pub audio_bytes: u64,
    /// Seçilen akışın yüksekliği (varsa).
    pub height: Option<u64>,
}

/// MPD'yi indirir, en iyi video (+ varsa ses) akışını indirir.
///
/// `stem` çıktı dosya köküdür; uzantı mime'a göre seçilir.
#[allow(clippy::too_many_arguments)]
pub async fn download<P>(
    client: &Client,
    mpd_url: &str,
    headers: RequestHeaders<'_>,
    out_dir: &Path,
    stem: &str,
    cancel: &CancelToken,
    limiter: &RateLimiter,
    progress: P,
) -> Result<DashOutput, DashError>
where
    P: Fn(usize, usize, u64) + Send + Sync,
{
    let parsed = load_manifest(client, mpd_url, headers)
        .await
        .map_err(DashError::Message)?;
    let (video, audio) = manifest::pick_tracks(&parsed);
    if video.is_none() && audio.is_none() {
        return Err(DashError::Message("MPD'de indirilebilir akış yok.".into()));
    }

    let extension = |track: &DashTrack| -> &'static str {
        let mime = track.mime.to_ascii_lowercase();
        match (track.kind, mime.contains("webm")) {
            (TrackKind::Video, true) => "webm",
            (TrackKind::Video, false) => "mp4",
            (TrackKind::Audio, true) => "weba",
            (TrackKind::Audio, false) => "m4a",
        }
    };

    let mut output = DashOutput {
        video: None,
        audio: None,
        video_bytes: 0,
        audio_bytes: 0,
        height: video.as_ref().and_then(|track| track.height),
    };

    if let Some(track) = &video {
        let path = out_dir.join(format!("{stem}.{}", extension(track)));
        output.video_bytes =
            download_track(client, track, headers, &path, cancel, limiter, &progress).await?;
        output.video = Some(path);
    }
    if let Some(track) = &audio {
        let path = out_dir.join(format!("{stem}.ses.{}", extension(track)));
        let silent = |_done: usize, _total: usize, _bytes: u64| {};
        output.audio_bytes =
            download_track(client, track, headers, &path, cancel, limiter, &silent).await?;
        output.audio = Some(path);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uzanti_mime_gore_secilir() {
        // `download` içindeki kapalı fonksiyonun kuralını burada doğruluyoruz.
        let pick = |kind: TrackKind, mime: &str| -> &'static str {
            let mime = mime.to_ascii_lowercase();
            match (kind, mime.contains("webm")) {
                (TrackKind::Video, true) => "webm",
                (TrackKind::Video, false) => "mp4",
                (TrackKind::Audio, true) => "weba",
                (TrackKind::Audio, false) => "m4a",
            }
        };
        assert_eq!(pick(TrackKind::Video, "video/mp4"), "mp4");
        assert_eq!(pick(TrackKind::Video, "video/webm"), "webm");
        assert_eq!(pick(TrackKind::Audio, "audio/mp4"), "m4a");
        assert_eq!(pick(TrackKind::Audio, "audio/webm"), "weba");
    }
}
