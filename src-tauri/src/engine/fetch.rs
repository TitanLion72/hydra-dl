//! Segment indirme için ortak HTTP yardımcıları (HLS ve DASH paylaşır).

use super::http::RequestHeaders;
use reqwest::header::{ACCEPT_ENCODING, COOKIE, RANGE, REFERER};
use reqwest::Client;
use std::time::Duration;

/// Parça başına en fazla deneme sayısı (ilk deneme dahil).
const MAX_SEGMENT_ATTEMPTS: u32 = 4;

/// Hata mesajı geçici mi (yeniden denenebilir)?
///
/// Neden metin üzerinden: `fetch_inner` hata tipini `String` olarak döndürüyor
/// (HLS ve DASH aynı yolu paylaşıyor). Sınıflandırma tek yerde tutulur ve saf
/// fonksiyon olarak test edilir.
pub(crate) fn is_transient(message: &str) -> bool {
    // Kalıcı: parça beklenenden büyük (bozuk yönlendirme) — tekrar denemek boşuna.
    if message.contains("beklenenden büyük") {
        return false;
    }
    if let Some(code) = http_code(message) {
        // 5xx ve 429 geçici; diğer 4xx kalıcı (yetki, bulunamadı).
        return code >= 500 || code == 429;
    }
    // Ağ / gövde okuma hataları geçici.
    message.starts_with("bağlantı hatası") || message.starts_with("gövde okunamadı")
}

/// `"HTTP 503 Service Unavailable"` → `Some(503)`.
fn http_code(message: &str) -> Option<u16> {
    let rest = message.strip_prefix("HTTP ")?;
    rest.split_whitespace().next()?.parse().ok()
}

/// Parçayı geçici hatalarda yeniden dener (üstel geri çekilme).
///
/// Neden gerekli: bir HLS akışı yüzlerce parçadan oluşur; tek bir anlık ağ
/// hatası tüm indirmeyi çöpe atardı. Kalıcı hatalar (404, sınır aşımı) hemen
/// döner — boşuna beklemeyiz.
async fn with_retry<F, Fut>(mut attempt_fetch: F) -> Result<Vec<u8>, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    let mut last_error = String::new();
    for attempt in 0..MAX_SEGMENT_ATTEMPTS {
        match attempt_fetch().await {
            Ok(bytes) => return Ok(bytes),
            Err(message) => {
                last_error = message;
                if !is_transient(&last_error) || attempt + 1 == MAX_SEGMENT_ATTEMPTS {
                    return Err(last_error);
                }
                // 200 ms, 400 ms, 800 ms.
                let delay = Duration::from_millis(200u64 << attempt);
                tokio::time::sleep(delay).await;
            }
        }
    }
    Err(last_error)
}

/// Bir segmenti (tam) indirir. `limit` aşılırsa hata verir: bozuk/yanlış
/// yönlendirme büyük dosyaları sessizce diske yazmasın.
pub(crate) async fn fetch_bytes(
    client: &Client,
    url: &str,
    headers: RequestHeaders<'_>,
    limit: Option<u64>,
) -> Result<Vec<u8>, String> {
    with_retry(|| fetch_inner(client, url, headers, None, limit)).await
}

/// Byte-range ile indirir (`bytes=start-...`). SegmentBase/SegmentList için.
pub(crate) async fn fetch_range(
    client: &Client,
    url: &str,
    headers: RequestHeaders<'_>,
    range: (u64, Option<u64>),
    limit: Option<u64>,
) -> Result<Vec<u8>, String> {
    with_retry(|| fetch_inner(client, url, headers, Some(range), limit)).await
}

async fn fetch_inner(
    client: &Client,
    url: &str,
    headers: RequestHeaders<'_>,
    range: Option<(u64, Option<u64>)>,
    limit: Option<u64>,
) -> Result<Vec<u8>, String> {
    let mut request = client.get(url).header(ACCEPT_ENCODING, "identity");
    if let Some(referer) = RequestHeaders::safe_value(headers.referer.unwrap_or("")) {
        request = request.header(REFERER, referer);
    }
    if let Some(cookie) = RequestHeaders::safe_value(headers.cookie.unwrap_or("")) {
        request = request.header(COOKIE, cookie);
    }
    if let Some((start, length)) = range {
        let spec = match length {
            Some(length) => format!("bytes={start}-{}", start + length - 1),
            None => format!("bytes={start}-"),
        };
        request = request.header(RANGE, spec);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("bağlantı hatası: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("gövde okunamadı: {e}"))?;
    if let Some(limit) = limit {
        if bytes.len() as u64 > limit {
            return Err(format!(
                "segment beklenenden büyük: {} > {limit} bayt",
                bytes.len()
            ));
        }
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_basligi_dogru_kurulur() {
        // Başlık biçimi saf olarak test edilebilir olsun diye aynı mantığı burada doğruluyoruz.
        let spec = |start: u64, length: Option<u64>| match length {
            Some(length) => format!("bytes={start}-{}", start + length - 1),
            None => format!("bytes={start}-"),
        };
        assert_eq!(spec(0, Some(1024)), "bytes=0-1023");
        assert_eq!(spec(2048, None), "bytes=2048-");
    }

    #[tokio::test]
    async fn gecersiz_adres_hata_doner() {
        let client = Client::new();
        let result = fetch_bytes(
            &client,
            "http://127.0.0.1:1/yok",
            RequestHeaders::new(None, None),
            None,
        )
        .await;
        assert!(result.is_err(), "bağlantı kurulamayan adres hata vermeli");
    }

    #[test]
    fn gecici_hata_siniflandirmasi() {
        // Geçici: ağ hatası, 5xx, 429 — yeniden denenir.
        for message in [
            "bağlantı hatası: connection reset",
            "gövde okunamadı: unexpected eof",
            "HTTP 503 Service Unavailable",
            "HTTP 500 Internal Server Error",
            "HTTP 429 Too Many Requests",
        ] {
            assert!(is_transient(message), "geçici sayılmalıydı: {message}");
        }
        // Kalıcı: 4xx (429 hariç), sınır aşımı — yeniden denenmez.
        for message in [
            "HTTP 404 Not Found",
            "HTTP 403 Forbidden",
            "HTTP 401 Unauthorized",
            "segment beklenenden büyük: 999 > 10 bayt",
            "anahtar bulunamadı",
        ] {
            assert!(!is_transient(message), "kalıcı sayılmalıydı: {message}");
        }
    }

    /// HLS/DASH parçası: geçici hatada yeniden denenir, kalıcıda hemen döner.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gecici_hata_sonrasi_parca_yeniden_denenir() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;

        let hits = Arc::new(AtomicU64::new(0));
        let hits_server = hits.clone();
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = server.accept().await else {
                    return;
                };
                let hits = hits_server.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 2048];
                    let _ = socket.read(&mut buf).await;
                    let hit = hits.fetch_add(1, Ordering::SeqCst);
                    let body = b"parca-icerigi";
                    let response = if hit < 2 {
                        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_string()
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                    };
                    let _ = socket.write_all(response.as_bytes()).await;
                    if hit >= 2 {
                        let _ = socket.write_all(body).await;
                    }
                    let _ = socket.shutdown().await;
                });
            }
        });

        let client = Client::new();
        let url = format!("http://{addr}/segment.ts");
        let bytes = fetch_bytes(&client, &url, RequestHeaders::new(None, None), None)
            .await
            .expect("iki geçici hatadan sonra parça inmeli");
        assert_eq!(bytes, b"parca-icerigi");
        assert!(
            hits.load(Ordering::SeqCst) >= 3,
            "en az iki yeniden deneme olmalı"
        );
    }
}
