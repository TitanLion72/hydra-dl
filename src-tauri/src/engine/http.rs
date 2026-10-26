//! Segmentli HTTP indirme: Range, devam ettirme, iptal ve hiz limiti.

use super::{CancelToken, RateLimiter};
use crate::model::Segment;
use crate::util;
use futures_util::StreamExt;
use reqwest::header::{
    ACCEPT_ENCODING, ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
    CONTENT_TYPE, RANGE, REFERER,
};
use reqwest::{Client, StatusCode};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;

pub const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36 HydraDL/0.1";

#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub total: Option<u64>,
    pub resumable: bool,
    pub filename: Option<String>,
    pub mime: Option<String>,
}

#[derive(Debug)]
pub enum HttpError {
    Cancelled,
    /// Sunucu Range'i yok saydi (200 dondu); tek baglantiya dusulmeli.
    RangeIgnored,
    Network(String),
    Io(String),
    Incomplete(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Cancelled => write!(f, "iptal edildi"),
            HttpError::RangeIgnored => write!(f, "sunucu Range istegini yok saydi"),
            HttpError::Network(m) => write!(f, "ag hatasi: {m}"),
            HttpError::Io(m) => write!(f, "disk hatasi: {m}"),
            HttpError::Incomplete(m) => write!(f, "eksik indirme: {m}"),
        }
    }
}

impl std::error::Error for HttpError {}

pub fn build_client() -> Client {
    Client::builder()
        .user_agent(DEFAULT_UA)
        .connect_timeout(std::time::Duration::from_secs(20))
        .pool_max_idle_per_host(20)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("reqwest client olusturulamadi")
}

fn parse_content_range(value: &str) -> Option<u64> {
    // "bytes 0-0/12345" veya "bytes 0-0/*"
    let total = value.rsplit('/').next()?.trim();
    if total == "*" {
        None
    } else {
        total.parse::<u64>().ok()
    }
}

/// Indirmeden once boyut / Range destegi / dosya adi kesfet.
pub async fn probe(client: &Client, url: &str) -> Result<Probe, String> {
    let head = client
        .head(url)
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await;

    if let Ok(resp) = head {
        if resp.status().is_success() {
            let headers = resp.headers();
            let total = headers
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|n| *n > 0);
            let resumable = headers
                .get(ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_ascii_lowercase().contains("bytes"))
                .unwrap_or(false);
            return Ok(Probe {
                total,
                resumable,
                filename: headers
                    .get(CONTENT_DISPOSITION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(util::filename_from_disposition),
                mime: headers
                    .get(CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.split(';').next().unwrap_or(v).trim().to_string()),
            });
        }
    }

    // HEAD desteklenmiyor / reddedildi: 1 baytlik Range ile dene.
    let resp = client
        .get(url)
        .header(ACCEPT_ENCODING, "identity")
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(|e| format!("baglanti kurulamadi: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("sunucu {status} dondurdu"));
    }
    let headers = resp.headers();
    let ranged = status == StatusCode::PARTIAL_CONTENT;
    let total = if ranged {
        headers
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range)
    } else {
        headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
    };
    let resumable = ranged
        || headers
            .get(ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_ascii_lowercase().contains("bytes"))
            .unwrap_or(false);

    Ok(Probe {
        total,
        resumable,
        filename: headers
            .get(CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(util::filename_from_disposition),
        mime: headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or(v).trim().to_string()),
    })
}

/// Bir indirme denemesinin girdileri.
pub struct DownloadRequest<'a> {
    pub client: &'a Client,
    pub url: &'a str,
    pub referer: Option<&'a str>,
    pub segments: &'a [Segment],
    pub parts_dir: &'a Path,
    pub cancel: &'a CancelToken,
    pub progress: &'a [AtomicU64],
    pub limiter: &'a RateLimiter,
}

/// Segmentleri paralel indirir. `progress[i]`, i. segmentin mutlak yazilmis
/// bayt sayisini tutar (manager oradan okur). Basarida tum parcalar tamdir.
pub async fn download_segments(request: DownloadRequest<'_>) -> Result<(), HttpError> {
    let DownloadRequest {
        client,
        url,
        referer,
        segments,
        parts_dir,
        cancel,
        progress,
        limiter,
    } = request;
    fs::create_dir_all(parts_dir)
        .await
        .map_err(|e| HttpError::Io(e.to_string()))?;

    let tasks = segments.iter().zip(progress.iter()).map(|(seg, done)| {
        download_one(client, url, referer, seg, parts_dir, cancel, done, limiter)
    });
    // Ilk hata digerlerini de durdurur; yarim kalan parcalar sonraki denemede surer.
    futures_util::future::try_join_all(tasks).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn download_one(
    client: &Client,
    url: &str,
    referer: Option<&str>,
    seg: &Segment,
    parts_dir: &Path,
    cancel: &CancelToken,
    progress: &AtomicU64,
    limiter: &RateLimiter,
) -> Result<(), HttpError> {
    let part = parts_dir.join(format!("{}.part", seg.index));
    let expected = seg.total_len();

    // Onceki kosudan kalan dosyayi dogrula; kayit ile disk uyusmazsa
    // diskteki gercek uzunlugu esas al.
    let disk_len = fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    let mut done = seg.done.min(disk_len);
    if let Some(len) = expected {
        done = done.min(len);
    }
    if disk_len != done {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&part)
            .await
            .map_err(|e| HttpError::Io(e.to_string()))?;
        file.set_len(done)
            .await
            .map_err(|e| HttpError::Io(e.to_string()))?;
    }
    progress.store(done, Ordering::Relaxed);

    if expected == Some(done) {
        return Ok(());
    }
    if cancel.is_cancelled() {
        return Err(HttpError::Cancelled);
    }

    let mut request = client
        .get(url)
        .header(ACCEPT_ENCODING, "identity")
        .header(ACCEPT_RANGES, "bytes");
    if let Some(referer) = referer {
        request = request.header(REFERER, referer);
    }
    let mut range_seg = seg.clone();
    range_seg.done = done;
    if let Some(range) = range_seg.range_header() {
        request = request.header(RANGE, range);
    }

    let response = request
        .send()
        .await
        .map_err(|e| HttpError::Network(e.to_string()))?;
    let status = response.status();

    if status == StatusCode::RANGE_NOT_SATISFIABLE {
        if expected == Some(done) {
            return Ok(());
        }
        return Err(HttpError::Network("416 Range Not Satisfiable".into()));
    }
    if !status.is_success() {
        return Err(HttpError::Network(format!("sunucu {status} dondurdu")));
    }
    // Range istedik ama tam govde geldi: sunucu desteklemiyor.
    if status == StatusCode::OK && (seg.start + done) > 0 {
        return Err(HttpError::RangeIgnored);
    }

    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&part)
        .await
        .map_err(|e| HttpError::Io(e.to_string()))?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            let _ = file.flush().await;
            return Err(HttpError::Cancelled);
        }
        let chunk = chunk.map_err(|e| HttpError::Network(e.to_string()))?;
        if chunk.is_empty() {
            continue;
        }
        let mut slice: &[u8] = &chunk;
        if let Some(len) = expected {
            let room = len.saturating_sub(done);
            if room == 0 {
                break;
            }
            if slice.len() as u64 > room {
                slice = &slice[..room as usize];
            }
        }
        file.write_all(slice)
            .await
            .map_err(|e| HttpError::Io(e.to_string()))?;
        done += slice.len() as u64;
        progress.store(done, Ordering::Relaxed);
        limiter.consume(slice.len() as u64).await;
    }

    file.flush()
        .await
        .map_err(|e| HttpError::Io(e.to_string()))?;
    drop(file);

    if cancel.is_cancelled() {
        return Err(HttpError::Cancelled);
    }
    if let Some(len) = expected {
        if done != len {
            return Err(HttpError::Incomplete(format!(
                "{}. parca: beklenen {len}, yazilan {done}",
                seg.index
            )));
        }
    }
    Ok(())
}

/// Parcalari tek dosyada birlestirir. Once `.hydra-tmp` dosyasina yazip
/// yeniden adlandirir; yarim kalan birlestirme hedefi bozmaz.
pub async fn concat_parts(
    parts_dir: &Path,
    segments: &[Segment],
    target: &Path,
) -> Result<(), HttpError> {
    let tmp = {
        let mut name = target.as_os_str().to_os_string();
        name.push(".hydra-tmp");
        PathBuf::from(name)
    };
    let mut out = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)
        .await
        .map_err(|e| HttpError::Io(e.to_string()))?;

    for seg in segments {
        let part = parts_dir.join(format!("{}.part", seg.index));
        let mut input = fs::File::open(&part)
            .await
            .map_err(|e| HttpError::Io(format!("{}: {e}", part.display())))?;
        tokio::io::copy(&mut input, &mut out)
            .await
            .map_err(|e| HttpError::Io(e.to_string()))?;
    }
    out.flush()
        .await
        .map_err(|e| HttpError::Io(e.to_string()))?;
    out.sync_all()
        .await
        .map_err(|e| HttpError::Io(e.to_string()))?;
    drop(out);

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| HttpError::Io(e.to_string()))?;
    }
    fs::rename(&tmp, target)
        .await
        .map_err(|e| HttpError::Io(format!("yeniden adlandirma: {e}")))?;
    Ok(())
}

pub async fn remove_dir(dir: &Path) {
    let _ = fs::remove_dir_all(dir).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::plan_segments;
    use axum::{
        body::Body,
        extract::State,
        http::{header, HeaderMap},
        response::Response,
        routing::get,
        Router,
    };
    use std::sync::Arc;

    /// Test sunucusu: `honor_range` false ise Range'i yok sayar.
    async fn spawn_file_server(data: Vec<u8>, honor_range: bool) -> String {
        async fn handler(
            State((data, honor)): State<(Arc<Vec<u8>>, bool)>,
            headers: HeaderMap,
        ) -> Response {
            let total = data.len() as u64;
            let range = headers
                .get(header::RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bytes="));

            if honor {
                if let Some(range) = range {
                    let mut split = range.split('-');
                    let start: u64 = split.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let end: u64 = split
                        .next()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(total.saturating_sub(1));
                    let start = start.min(total.saturating_sub(1));
                    let end = end.min(total.saturating_sub(1));
                    if start > end {
                        return Response::builder().status(416).body(Body::empty()).unwrap();
                    }
                    let slice = data[start as usize..=end as usize].to_vec();
                    return Response::builder()
                        .status(206)
                        .header(header::CONTENT_LENGTH, slice.len().to_string())
                        .header(
                            header::CONTENT_RANGE,
                            format!("bytes {start}-{end}/{total}"),
                        )
                        .header(header::ACCEPT_RANGES, "bytes")
                        .body(Body::from(slice))
                        .unwrap();
                }
            }

            let mut builder = Response::builder()
                .status(200)
                .header(header::CONTENT_LENGTH, total.to_string());
            if honor {
                builder = builder.header(header::ACCEPT_RANGES, "bytes");
            }
            builder.body(Body::from(data.as_ref().clone())).unwrap()
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/file", get(handler))
            .with_state((Arc::new(data), honor_range));
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}/file")
    }

    fn test_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 251) as u8).collect()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parsele_indirme_ve_birlestirme() {
        let data = test_data(2_600_000);
        let url = spawn_file_server(data.clone(), true).await;
        let client = build_client();
        let probe = probe(&client, &url).await.unwrap();
        assert_eq!(probe.total, Some(data.len() as u64));
        assert!(probe.resumable);

        let segments = plan_segments(probe.total, true, 4);
        assert!(segments.len() >= 2, "segment sayisi: {}", segments.len());

        let dir = tempfile::tempdir().unwrap();
        let parts = dir.path().join("parts");
        let cancel = CancelToken::new();
        let limiter = RateLimiter::new(0);
        let progress: Vec<AtomicU64> = segments.iter().map(|_| AtomicU64::new(0)).collect();

        download_segments(DownloadRequest {
            client: &client,
            url: &url,
            referer: None,
            segments: &segments,
            parts_dir: &parts,
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
        })
        .await
        .unwrap();

        let target = dir.path().join("out.bin");
        concat_parts(&parts, &segments, &target).await.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), data);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn yarim_kalan_parca_devam_eder() {
        let data = test_data(1_200_000);
        let url = spawn_file_server(data.clone(), true).await;
        let client = build_client();
        let mut segments = plan_segments(Some(data.len() as u64), true, 3);
        assert!(segments.len() >= 2);

        let dir = tempfile::tempdir().unwrap();
        let parts = dir.path().join("parts");
        tokio::fs::create_dir_all(&parts).await.unwrap();
        // Ilk segmentin 100 KB'i onceden indirilmis gibi davran.
        let partial: Vec<u8> = data[..100_000].to_vec();
        tokio::fs::write(parts.join("0.part"), &partial)
            .await
            .unwrap();
        segments[0].done = partial.len() as u64;

        let progress: Vec<AtomicU64> = segments.iter().map(|s| AtomicU64::new(s.done)).collect();
        let cancel = CancelToken::new();
        let limiter = RateLimiter::new(0);
        download_segments(DownloadRequest {
            client: &client,
            url: &url,
            referer: None,
            segments: &segments,
            parts_dir: &parts,
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
        })
        .await
        .unwrap();

        // Devam eden segment bastan yazilmamali: parca boyu tam olmali.
        let part0 = tokio::fs::metadata(parts.join("0.part"))
            .await
            .unwrap()
            .len();
        assert_eq!(part0, segments[0].total_len().unwrap());

        let target = dir.path().join("out.bin");
        concat_parts(&parts, &segments, &target).await.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), data);
    }

    #[tokio::test]
    async fn range_yok_sayilirsa_hata_verir() {
        let data = test_data(10_000);
        let url = spawn_file_server(data, false).await;
        let client = build_client();
        let segments = vec![Segment {
            index: 0,
            start: 1000,
            end: Some(1999),
            done: 0,
        }];
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelToken::new();
        let limiter = RateLimiter::new(0);
        let progress = vec![AtomicU64::new(0)];
        let err = download_segments(DownloadRequest {
            client: &client,
            url: &url,
            referer: None,
            segments: &segments,
            parts_dir: dir.path(),
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
        })
        .await
        .unwrap_err();
        assert!(
            matches!(err, HttpError::RangeIgnored),
            "beklenmeyen hata: {err:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn iptal_edilen_indirme_durur() {
        let data = test_data(4_000_000);
        let url = spawn_file_server(data.clone(), true).await;
        let client = build_client();
        let segments = plan_segments(Some(data.len() as u64), false, 1);
        let dir = tempfile::tempdir().unwrap();
        let parts = dir.path().join("parts");
        let cancel = CancelToken::new();
        let limiter = RateLimiter::new(200 * 1024); // yavaslat ki iptal yetissin
        let progress = vec![AtomicU64::new(0)];

        let token = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            token.cancel();
        });

        let result = download_segments(DownloadRequest {
            client: &client,
            url: &url,
            referer: None,
            segments: &segments,
            parts_dir: &parts,
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
        })
        .await;
        assert!(
            matches!(result, Err(HttpError::Cancelled)),
            "sonuc: {result:?}"
        );
        let written = tokio::fs::metadata(parts.join("0.part"))
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        assert!(written < data.len() as u64, "yazilan: {written}");
    }

    #[test]
    fn content_range_cozulur() {
        assert_eq!(parse_content_range("bytes 0-0/12345"), Some(12345));
        assert_eq!(parse_content_range("bytes 100-199/200"), Some(200));
        assert_eq!(parse_content_range("bytes 0-0/*"), None);
        assert_eq!(parse_content_range("sacma"), None);
    }

    #[tokio::test]
    async fn parcalar_birlesir() {
        let dir = tempfile::tempdir().unwrap();
        let parts = dir.path().join("parts");
        fs::create_dir_all(&parts).await.unwrap();
        let segs = vec![
            Segment {
                index: 0,
                start: 0,
                end: Some(3),
                done: 4,
            },
            Segment {
                index: 1,
                start: 4,
                end: Some(7),
                done: 4,
            },
        ];
        fs::write(parts.join("0.part"), b"abcd").await.unwrap();
        fs::write(parts.join("1.part"), b"efgh").await.unwrap();
        let target = dir.path().join("out.bin");
        concat_parts(&parts, &segs, &target).await.unwrap();
        assert_eq!(fs::read(&target).await.unwrap(), b"abcdefgh");
    }
}
