//! Segmentli HTTP indirme: Range, devam ettirme, iptal ve hiz limiti.

use super::{CancelToken, RateLimiter};
use crate::model::Segment;
use crate::util;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use reqwest::header::{
    ACCEPT_ENCODING, ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
    CONTENT_TYPE, COOKIE, RANGE, REFERER, RETRY_AFTER,
};
use reqwest::{Client, StatusCode};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;

pub const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36 HydraDL/0.1";

/// Gecici hatada ust uste kac kez denenecegi (ilk deneme dahil).
pub const MAX_ATTEMPTS: u32 = 5;
/// Ilk tekrar denemeden onceki bekleme; her denemede ikiye katlanir.
const BASE_BACKOFF: Duration = Duration::from_millis(500);
/// Tek bir beklemenin ust siniri (`Retry-After` dahil).
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Gecici sunucu hatasi mi? 408/425/429 ve 5xx (501 haric) tekrar denenir.
pub fn is_retryable_status(status: StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 425 | 429)
        || (status.is_server_error() && status != StatusCode::NOT_IMPLEMENTED)
}

/// `Retry-After` basligini cozer: saniye (`120`) veya HTTP tarihi
/// (`Sun, 06 Nov 1994 08:49:37 GMT`) olabilir. Gecmis tarih → 0.
pub fn parse_retry_after(value: &str, now: DateTime<Utc>) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(secs) = value.parse::<u64>() {
        // Sunucu sacmalamasin: en fazla 1 saat.
        return Some(Duration::from_secs(secs.min(3600)));
    }
    let when = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    let delta = when.signed_duration_since(now);
    Some(delta.to_std().unwrap_or(Duration::ZERO))
}

/// `attempt` (0 tabanli) tekrar denemesinden once beklenecek sure.
/// `Retry-After` varsa ona uyulur; yoksa ustel geri cekilme + jitter.
pub fn backoff_delay(attempt: u32, retry_after: Option<Duration>, jitter_ms: u64) -> Duration {
    if let Some(after) = retry_after {
        return after.min(MAX_BACKOFF);
    }
    let factor = 1u64 << attempt.min(6);
    let base = BASE_BACKOFF.saturating_mul(factor.min(u32::MAX as u64) as u32);
    (base + Duration::from_millis(jitter_ms)).min(MAX_BACKOFF)
}

/// Segmentlerin ayni anda uyanmamasi icin kucuk, tekrarlanabilir jitter.
fn jitter_for(seg_index: usize, attempt: u32) -> u64 {
    ((seg_index as u64 * 97 + attempt as u64 * 41) % 4) * 60
}

/// Tekrar deneme bilgisini arayuze tasiyan geri cagri: (deneme, toplam, bekleme).
pub type RetryHook<'a> = &'a (dyn Fn(u32, u32, Duration) + Send + Sync);

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
    /// Sunucu hata durumu dondurdu; `Retry-After` varsa saniye cinsinden.
    Status(u16, Option<Duration>),
    Network(String),
    Io(String),
    Incomplete(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Cancelled => write!(f, "iptal edildi"),
            HttpError::RangeIgnored => write!(f, "sunucu Range istegini yok saydi"),
            HttpError::Status(code, _) => write!(f, "sunucu {code} dondurdu"),
            HttpError::Network(m) => write!(f, "ag hatasi: {m}"),
            HttpError::Io(m) => write!(f, "disk hatasi: {m}"),
            HttpError::Incomplete(m) => write!(f, "eksik indirme: {m}"),
        }
    }
}

impl HttpError {
    /// Tekrar denemeye deger mi? (iptal, disk hatasi ve Range yok sayma hayir.)
    pub fn retry_after(&self) -> Option<Option<Duration>> {
        match self {
            HttpError::Status(code, after) => {
                let status = StatusCode::from_u16(*code).ok()?;
                is_retryable_status(status).then_some(*after)
            }
            HttpError::Network(_) | HttpError::Incomplete(_) => Some(None),
            _ => None,
        }
    }
}

impl std::error::Error for HttpError {}

/// Testlerde ve varsayılan kurulumda kullanılan proxysiz istemci.
#[cfg(test)]
pub fn build_client() -> Client {
    build_client_with_proxy("")
}

/// Proxy destekli HTTP istemcisi.
///
/// `proxy` boşsa sistem varsayılanı (env değişkenleri) kullanılır. Kabul edilen
/// biçimler: `http://host:port`, `https://host:port`, `socks5://host:port`,
/// `socks5h://host:port`. Geçersiz değer **sessizce yok sayılmaz**: istemci
/// proxysiz kurulur ve `proxy_problem` ile neden bildirilir.
pub fn build_client_with_proxy(proxy: &str) -> Client {
    let mut builder = Client::builder()
        .user_agent(DEFAULT_UA)
        .connect_timeout(std::time::Duration::from_secs(20))
        .pool_max_idle_per_host(20)
        .redirect(reqwest::redirect::Policy::limited(10));
    if !proxy.trim().is_empty() {
        let url = proxy.trim();
        if let Ok(parsed) = reqwest::Proxy::all(url) {
            builder = builder.proxy(parsed);
        } else {
            eprintln!("[hydra] geçersiz proxy adresi yok sayıldı: {url}");
        }
    }
    builder.build().expect("reqwest client olusturulamadi")
}

/// Proxy adresi geçerli mi? (ayar kaydında kullanıcıya geri bildirim için)
pub fn proxy_problem(proxy: &str) -> Option<String> {
    let trimmed = proxy.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let ok_scheme = ["http://", "https://", "socks5://", "socks5h://"]
        .iter()
        .any(|scheme| lower.starts_with(scheme));
    if !ok_scheme {
        return Some(
            "Proxy adresi http://, https://, socks5:// veya socks5h:// ile başlamalı".into(),
        );
    }
    if reqwest::Proxy::all(trimmed).is_err() {
        return Some("Proxy adresi çözümlenemedi (host:port biçimini kontrol et)".into());
    }
    None
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
///
/// Gecici sunucu hatasinda (429/503/5xx) baglanti kurulurken de yeniden denenir;
/// aksi halde indirme hic baslamadan "sunucu 503 dondurdu" ile biterdi.
/// Basliksiz probe (testler ve basit cagrilar icin sarmalayici).
#[cfg(test)]
pub async fn probe(client: &Client, url: &str) -> Result<Probe, String> {
    probe_with(client, url, RequestHeaders::default()).await
}

/// Baslikli probe: giris isteyen sitelerde cerez/referer gerekir.
pub async fn probe_with(
    client: &Client,
    url: &str,
    headers: RequestHeaders<'_>,
) -> Result<Probe, String> {
    let mut attempt = 0u32;
    loop {
        match probe_once(client, url, headers).await {
            Ok(probe) => return Ok(probe),
            Err(err) => {
                let Some(retry_after) = err.retry_after() else {
                    return Err(err.to_string());
                };
                attempt += 1;
                if attempt >= MAX_ATTEMPTS {
                    return Err(format!("{err} ({attempt} deneme sonrası vazgeçildi)"));
                }
                let delay = backoff_delay(attempt - 1, retry_after, jitter_for(0, attempt));
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Tek bir probe denemesi (HEAD, olmazsa 1 baytlik Range).
async fn probe_once(
    client: &Client,
    url: &str,
    headers: RequestHeaders<'_>,
) -> Result<Probe, HttpError> {
    let mut head_req = client.head(url).header(ACCEPT_ENCODING, "identity");
    if let Some(referer) = RequestHeaders::safe_value(headers.referer.unwrap_or("")) {
        head_req = head_req.header(REFERER, referer);
    }
    if let Some(cookie) = RequestHeaders::safe_value(headers.cookie.unwrap_or("")) {
        head_req = head_req.header(COOKIE, cookie);
    }
    let head = head_req.send().await;

    if let Ok(resp) = head {
        let retry_after = resp
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| parse_retry_after(v, Utc::now()));
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
        // Gecici hata: GET'e dusmek yerine bekle ve tekrar dene.
        if is_retryable_status(resp.status()) {
            return Err(HttpError::Status(resp.status().as_u16(), retry_after));
        }
    }

    // HEAD desteklenmiyor / reddedildi: 1 baytlik Range ile dene.
    let mut range_req = client
        .get(url)
        .header(ACCEPT_ENCODING, "identity")
        .header(RANGE, "bytes=0-0");
    if let Some(referer) = RequestHeaders::safe_value(headers.referer.unwrap_or("")) {
        range_req = range_req.header(REFERER, referer);
    }
    if let Some(cookie) = RequestHeaders::safe_value(headers.cookie.unwrap_or("")) {
        range_req = range_req.header(COOKIE, cookie);
    }
    let resp = range_req
        .send()
        .await
        .map_err(|e| HttpError::Network(format!("baglanti kurulamadi: {e}")))?;

    let status = resp.status();
    let retry_after = resp
        .headers()
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_retry_after(v, Utc::now()));
    if !status.is_success() {
        return Err(HttpError::Status(status.as_u16(), retry_after));
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

/// İstek başlıkları: sayfa adresi (Referer) ve oturum çerezi (Cookie).
///
/// Çerez kullanıcı tarayıcısından gelir; **başlık enjeksiyonuna** karşı
/// temizlenir. `Cookie`/`Referer` değerlerinde CR/LF bulunursa o başlık
/// gönderilmez (istek kaçırma girişimi).
#[derive(Debug, Clone, Copy, Default)]
pub struct RequestHeaders<'a> {
    pub referer: Option<&'a str>,
    pub cookie: Option<&'a str>,
}

impl<'a> RequestHeaders<'a> {
    pub fn new(referer: Option<&'a str>, cookie: Option<&'a str>) -> Self {
        Self { referer, cookie }
    }

    /// Başlık değeri güvenli mi? (CR/LF/NUL yok, boş değil, >8 KB değil)
    pub fn safe_value(value: &str) -> Option<&str> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.len() > 8 * 1024 {
            return None;
        }
        // CR, LF ve NUL: başlık bölme / enjeksiyon.
        if trimmed.bytes().any(|b| b == b'\r' || b == b'\n' || b == 0) {
            return None;
        }
        Some(trimmed)
    }
}

pub struct DownloadRequest<'a> {
    pub client: &'a Client,
    pub url: &'a str,
    /// Sayfa adresi + çerez (giriş isteyen siteler için).
    pub headers: RequestHeaders<'a>,
    pub segments: &'a [Segment],
    pub parts_dir: &'a Path,
    pub cancel: &'a CancelToken,
    pub progress: &'a [AtomicU64],
    pub limiter: &'a RateLimiter,
    /// Tekrar deneme basladiginda cagrilir (arayuzde "yeniden deneniyor" gostermek icin).
    pub on_retry: Option<RetryHook<'a>>,
}

/// Segmentleri paralel indirir. `progress[i]`, i. segmentin mutlak yazilmis
/// bayt sayisini tutar (manager oradan okur). Basarida tum parcalar tamdir.
pub async fn download_segments(request: DownloadRequest<'_>) -> Result<(), HttpError> {
    let DownloadRequest {
        client,
        url,
        headers,
        segments,
        parts_dir,
        cancel,
        progress,
        limiter,
        on_retry,
    } = request;
    fs::create_dir_all(parts_dir)
        .await
        .map_err(|e| HttpError::Io(e.to_string()))?;

    let tasks = segments.iter().zip(progress.iter()).map(|(seg, done)| {
        download_one(
            client, url, headers, seg, parts_dir, cancel, done, limiter, on_retry,
        )
    });
    // Ilk kalici hata digerlerini de durdurur; yarim kalan parcalar sonraki denemede surer.
    futures_util::future::try_join_all(tasks).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn download_one(
    client: &Client,
    url: &str,
    headers: RequestHeaders<'_>,
    seg: &Segment,
    parts_dir: &Path,
    cancel: &CancelToken,
    progress: &AtomicU64,
    limiter: &RateLimiter,
    on_retry: Option<RetryHook<'_>>,
) -> Result<(), HttpError> {
    let mut attempt = 0u32;
    loop {
        match attempt_once(
            client, url, headers, seg, parts_dir, cancel, progress, limiter,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(err) => {
                let Some(retry_after) = err.retry_after() else {
                    return Err(err);
                };
                attempt += 1;
                if attempt >= MAX_ATTEMPTS {
                    // Kullaniciya kac deneme yapildigini soyle.
                    return Err(HttpError::Network(format!(
                        "{err} ({attempt} deneme sonrası vazgeçildi)"
                    )));
                }
                let delay = backoff_delay(attempt - 1, retry_after, jitter_for(seg.index, attempt));
                if let Some(hook) = on_retry {
                    hook(attempt + 1, MAX_ATTEMPTS, delay);
                }
                // Beklerken iptal edilirse uyanmadan cik.
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = cancel.cancelled() => return Err(HttpError::Cancelled),
                }
            }
        }
    }
}

/// Tek bir deneme: kaldigi yerden Range ile indirir.
#[allow(clippy::too_many_arguments)]
async fn attempt_once(
    client: &Client,
    url: &str,
    headers: RequestHeaders<'_>,
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
    if let Some(referer) = RequestHeaders::safe_value(headers.referer.unwrap_or("")) {
        request = request.header(REFERER, referer);
    }
    // Cerez: giris isteyen sitelerde indirme icin gerekli. CRLF iceren deger
    // (baslik enjeksiyonu) sessizce atlanir.
    if let Some(cookie) = RequestHeaders::safe_value(headers.cookie.unwrap_or("")) {
        request = request.header(COOKIE, cookie);
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
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_retry_after(v, Utc::now()));

    if status == StatusCode::RANGE_NOT_SATISFIABLE {
        if expected == Some(done) {
            return Ok(());
        }
        return Err(HttpError::Network("416 Range Not Satisfiable".into()));
    }
    if !status.is_success() {
        return Err(HttpError::Status(status.as_u16(), retry_after));
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

    /// Ilk `fail_times` istekte `status` donen, sonra normal servis eden sunucu.
    /// `retry_after` verilirse `Retry-After` basligi eklenir.
    async fn spawn_flaky_server(
        data: Vec<u8>,
        fail_times: usize,
        status: u16,
        retry_after: Option<String>,
    ) -> (String, Arc<AtomicU64>) {
        #[derive(Clone)]
        struct FlakyState {
            data: Arc<Vec<u8>>,
            fail_times: usize,
            status: u16,
            retry_after: Option<String>,
            hits: Arc<AtomicU64>,
        }

        async fn handler(State(st): State<FlakyState>, headers: HeaderMap) -> Response {
            let hit = st.hits.fetch_add(1, Ordering::SeqCst);
            if (hit as usize) < st.fail_times {
                let mut builder = Response::builder().status(st.status);
                if let Some(after) = &st.retry_after {
                    builder = builder.header(header::RETRY_AFTER, after);
                }
                return builder.body(Body::empty()).unwrap();
            }
            let total = st.data.len() as u64;
            let range = headers
                .get(header::RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bytes="));
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
                let slice = st.data[start as usize..=end as usize].to_vec();
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
            Response::builder()
                .status(200)
                .header(header::CONTENT_LENGTH, total.to_string())
                .header(header::ACCEPT_RANGES, "bytes")
                .body(Body::from(st.data.as_ref().clone()))
                .unwrap()
        }

        let hits = Arc::new(AtomicU64::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/file", get(handler))
            .with_state(FlakyState {
                data: Arc::new(data),
                fail_times,
                status,
                retry_after,
                hits: hits.clone(),
            });
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}/file"), hits)
    }

    /// Tek segmentli bir istek yapar; (sonuc, deneme sayisi, beklemeler) doner.
    async fn try_single(
        url: &str,
        segments: &[Segment],
    ) -> (Result<(), HttpError>, u32, Vec<(u32, u64)>) {
        let client = build_client();
        let dir = tempfile::tempdir().unwrap();
        let parts = dir.path().join("parts");
        let cancel = CancelToken::new();
        let limiter = RateLimiter::new(0);
        let progress: Vec<AtomicU64> = segments.iter().map(|_| AtomicU64::new(0)).collect();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let hook = {
            let seen = seen.clone();
            move |attempt: u32, max: u32, wait: Duration| {
                assert!(attempt <= max);
                seen.lock()
                    .unwrap()
                    .push((attempt, wait.as_millis().min(u64::MAX as u128) as u64));
            }
        };
        let result = download_segments(DownloadRequest {
            client: &client,
            url,
            headers: RequestHeaders::default(),
            segments,
            parts_dir: &parts,
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
            on_retry: Some(&hook),
        })
        .await;
        let snapshot = seen.lock().unwrap().clone();
        (result, snapshot.len() as u32, snapshot)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gecici_503_tekrar_denenir_ve_tamamlanir() {
        let data = test_data(60_000);
        let (url, hits) = spawn_flaky_server(data.clone(), 2, 503, None).await;
        let segments = vec![Segment {
            index: 0,
            start: 0,
            end: Some(data.len() as u64 - 1),
            done: 0,
        }];
        let (result, retries, waits) = try_single(&url, &segments).await;
        assert!(result.is_ok(), "sonuc: {result:?}");
        assert_eq!(retries, 2, "iki kez yeniden denendi: {waits:?}");
        // Ustel geri cekilme: ikinci bekleme ilkinden uzun.
        assert!(waits[1].1 > waits[0].1, "beklemeler: {waits:?}");
        assert_eq!(waits[0].0, 2, "ilk tekrar 2. deneme olarak bildirildi");
        assert!(hits.load(Ordering::SeqCst) >= 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retry_after_beklemesi_kullanilir() {
        let data = test_data(20_000);
        let (url, _) = spawn_flaky_server(data.clone(), 1, 429, Some("1".into())).await;
        let segments = vec![Segment {
            index: 0,
            start: 0,
            end: Some(data.len() as u64 - 1),
            done: 0,
        }];
        let (result, retries, waits) = try_single(&url, &segments).await;
        assert!(result.is_ok(), "sonuc: {result:?}");
        assert_eq!(retries, 1);
        // 1 sn = 1000 ms; Retry-After varsa jitter eklenmez.
        assert_eq!(waits[0].1, 1000, "bekleme: {waits:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn kalici_404_hemen_basarisiz_olur() {
        let data = test_data(5_000);
        let (url, _) = spawn_flaky_server(data, usize::MAX, 404, None).await;
        let segments = vec![Segment {
            index: 0,
            start: 0,
            end: Some(4_999),
            done: 0,
        }];
        let (result, retries, _) = try_single(&url, &segments).await;
        assert!(
            matches!(result, Err(HttpError::Status(404, _))),
            "sonuc: {result:?}"
        );
        assert_eq!(retries, 0, "404 tekrar denenmemeli");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ust_uste_hatada_deneme_siniri_asilmaz() {
        let data = test_data(5_000);
        let (url, hits) = spawn_flaky_server(data, usize::MAX, 503, None).await;
        let segments = vec![Segment {
            index: 0,
            start: 0,
            end: Some(4_999),
            done: 0,
        }];
        let (result, retries, _) = try_single(&url, &segments).await;
        assert!(
            matches!(result, Err(HttpError::Network(_))),
            "sonuc: {result:?}"
        );
        assert_eq!(retries, MAX_ATTEMPTS - 1, "deneme siniri");
        assert_eq!(hits.load(Ordering::SeqCst), MAX_ATTEMPTS as u64);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn probe_gecici_hatada_tekrar_dener() {
        let data = test_data(8_000);
        let (url, hits) = spawn_flaky_server(data.clone(), 2, 503, None).await;
        let client = build_client();
        let probe = probe(&client, &url).await.unwrap();
        assert_eq!(probe.total, Some(data.len() as u64));
        assert!(probe.resumable);
        // HEAD + GET 0-0 denemeleri: iki 503'ten sonra basarili.
        assert!(
            hits.load(Ordering::SeqCst) >= 3,
            "istek sayisi: {}",
            hits.load(Ordering::SeqCst)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn probe_404_tekrar_denenmez() {
        let data = test_data(100);
        let (url, hits) = spawn_flaky_server(data, usize::MAX, 404, None).await;
        let client = build_client();
        let err = probe(&client, &url).await.unwrap_err();
        assert!(err.contains("404"), "hata: {err}");
        // HEAD bir kez + GET 0-0 bir kez (retry yok).
        assert!(
            hits.load(Ordering::SeqCst) <= 2,
            "istek: {}",
            hits.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn retry_after_basligi_cozulur() {
        let now = DateTime::parse_from_rfc2822("Sun, 06 Nov 1994 08:49:37 GMT")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            parse_retry_after("120", now),
            Some(Duration::from_secs(120))
        );
        // Gecmis tarih -> 0 (hemen dene).
        assert_eq!(
            parse_retry_after("Sun, 06 Nov 1994 08:00:00 GMT", now),
            Some(Duration::ZERO)
        );
        // 1 saatten uzun deger kirpilir.
        assert_eq!(
            parse_retry_after("99999", now),
            Some(Duration::from_secs(3600))
        );
        assert_eq!(parse_retry_after("yarin", now), None);
        assert_eq!(parse_retry_after("  ", now), None);
        // HTTP tarihi ileri: 2 dakika.
        assert_eq!(
            parse_retry_after("Sun, 06 Nov 1994 08:51:37 GMT", now),
            Some(Duration::from_secs(120))
        );
    }

    #[test]
    fn geri_cekilme_hesabi() {
        // Retry-After yok: 0.5s, 1s, 2s ... ve jitter.
        assert_eq!(backoff_delay(0, None, 0), Duration::from_millis(500));
        assert_eq!(backoff_delay(1, None, 0), Duration::from_millis(1000));
        assert_eq!(backoff_delay(2, None, 0), Duration::from_millis(2000));
        assert_eq!(backoff_delay(3, None, 120), Duration::from_millis(4120));
        // Retry-After varsa ona uyulur (jitter yok), 30 sn ile kirpilir.
        assert_eq!(
            backoff_delay(0, Some(Duration::from_secs(5)), 999),
            Duration::from_secs(5)
        );
        assert_eq!(
            backoff_delay(0, Some(Duration::from_secs(600)), 0),
            Duration::from_secs(30)
        );
        // Cok yuksek attempt tasmasin.
        assert_eq!(backoff_delay(99, None, 0), Duration::from_secs(30));
    }

    #[test]
    fn hangi_durumlar_tekrar_denenir() {
        for code in [408u16, 425, 429, 500, 502, 503, 504] {
            assert!(
                is_retryable_status(StatusCode::from_u16(code).unwrap()),
                "{code} tekrar denenmeli"
            );
        }
        for code in [400u16, 401, 403, 404, 410, 416, 501] {
            assert!(
                !is_retryable_status(StatusCode::from_u16(code).unwrap()),
                "{code} tekrar denenmemeli"
            );
        }
        assert!(!is_retryable_status(StatusCode::OK));
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
            headers: RequestHeaders::default(),
            segments: &segments,
            parts_dir: &parts,
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
            on_retry: None,
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
            headers: RequestHeaders::default(),
            segments: &segments,
            parts_dir: &parts,
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
            on_retry: None,
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
            headers: RequestHeaders::default(),
            segments: &segments,
            parts_dir: dir.path(),
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
            on_retry: None,
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
            headers: RequestHeaders::default(),
            segments: &segments,
            parts_dir: &parts,
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
            on_retry: None,
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
    fn proxy_adresi_dogrulanir() {
        // Gecerli bicimler.
        for iyi in [
            "http://127.0.0.1:8080",
            "https://proxy.example:3128",
            "socks5://127.0.0.1:1080",
            "socks5h://proxy.local:1080",
            "http://kullanici:sifre@proxy:8080",
        ] {
            assert!(proxy_problem(iyi).is_none(), "kabul edilmeli: {iyi}");
        }
        // Bos: sorun yok (proxy kullanilmaz).
        assert!(proxy_problem("").is_none());
        assert!(proxy_problem("   ").is_none());

        // Gecersiz semalar reddedilir.
        for kotu in [
            "127.0.0.1:8080",
            "ftp://x:1",
            "socks4://x:1",
            "javascript:alert(1)",
        ] {
            assert!(proxy_problem(kotu).is_some(), "reddedilmeli: {kotu}");
        }
    }

    #[test]
    fn proxy_istemcisi_kurulabilir() {
        // Gecerli proxy ile istemci olusur (gercek baglanti yok).
        let _ = build_client_with_proxy("http://127.0.0.1:9");
        // Gecersiz proxy: panik yok, proxysiz istemci.
        let _ = build_client_with_proxy("saçma değer");
        let _ = build_client_with_proxy("");
    }

    #[test]
    fn baslik_degerleri_temizlenir() {
        // Gecerli degerler.
        assert_eq!(
            RequestHeaders::safe_value("sid=abc; tema=koyu"),
            Some("sid=abc; tema=koyu")
        );
        assert_eq!(
            RequestHeaders::safe_value("  https://x/y  "),
            Some("https://x/y")
        );

        // Baslik enjeksiyonu: CR/LF/NUL reddedilir.
        for kotu in [
            "sid=abc\r\nX-Kotu: 1",
            "sid=abc\nX-Kotu: 1",
            "sid=abc\rX",
            "sid=abc\0",
        ] {
            assert_eq!(
                RequestHeaders::safe_value(kotu),
                None,
                "reddedilmeli: {kotu:?}"
            );
        }
        // Bos / asiri uzun.
        assert_eq!(RequestHeaders::safe_value(""), None);
        assert_eq!(RequestHeaders::safe_value("   "), None);
        let uzun = "a".repeat(8 * 1024 + 1);
        assert_eq!(RequestHeaders::safe_value(&uzun), None);
        // Tam sinir kabul edilir.
        let sinir = "a".repeat(8 * 1024);
        assert!(RequestHeaders::safe_value(&sinir).is_some());
    }

    /// Cerez olmadan 403, cerezle 200 donen sunucu: indirme gercekten
    /// cerezi gonderiyor mu?
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cerez_gonderilir_ve_indirme_basarili_olur() {
        let data = test_data(40_000);
        let url = spawn_cookie_server(data.clone(), "oturum=dogru").await;

        let segments = vec![Segment {
            index: 0,
            start: 0,
            end: Some(data.len() as u64 - 1),
            done: 0,
        }];
        let client = build_client();
        let dir = tempfile::tempdir().unwrap();
        let parts = dir.path().join("parts");
        let cancel = CancelToken::new();
        let limiter = RateLimiter::new(0);
        let progress: Vec<AtomicU64> = segments.iter().map(|_| AtomicU64::new(0)).collect();

        // 1) Cerez yok -> 403.
        let without = download_segments(DownloadRequest {
            client: &client,
            url: &url,
            headers: RequestHeaders::default(),
            segments: &segments,
            parts_dir: &parts,
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
            on_retry: None,
        })
        .await;
        assert!(without.is_err(), "cerezsiz indirme basarili olmamali");

        // 2) Cerez var -> basarili + icerik dogru.
        let with = download_segments(DownloadRequest {
            client: &client,
            url: &url,
            headers: RequestHeaders::new(None, Some("oturum=dogru")),
            segments: &segments,
            parts_dir: &parts,
            cancel: &cancel,
            progress: &progress,
            limiter: &limiter,
            on_retry: None,
        })
        .await;
        assert!(with.is_ok(), "cerezli indirme basarili olmali: {with:?}");

        let target = dir.path().join("out.bin");
        concat_parts(&parts, &segments, &target).await.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), data);
    }

    /// Cerez isteyen sunucu (`Cookie: <beklenen>` yoksa 403).
    async fn spawn_cookie_server(data: Vec<u8>, expected_cookie: &'static str) -> String {
        async fn handler(
            State((data, expected)): State<(Arc<Vec<u8>>, &'static str)>,
            headers: HeaderMap,
        ) -> Response {
            let ok = headers
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains(expected))
                .unwrap_or(false);
            if !ok {
                return Response::builder().status(403).body(Body::empty()).unwrap();
            }
            let total = data.len() as u64;
            let range = headers
                .get(header::RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bytes="));
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
            Response::builder()
                .status(200)
                .header(header::CONTENT_LENGTH, total.to_string())
                .header(header::ACCEPT_RANGES, "bytes")
                .body(Body::from(data.as_ref().clone()))
                .unwrap()
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/file", get(handler))
            .with_state((Arc::new(data), expected_cookie));
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}/file")
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
