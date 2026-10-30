//! Segment indirme için ortak HTTP yardımcıları (HLS ve DASH paylaşır).

use super::http::RequestHeaders;
use reqwest::header::{ACCEPT_ENCODING, COOKIE, RANGE, REFERER};
use reqwest::Client;

/// Bir segmenti (tam) indirir. `limit` aşılırsa hata verir: bozuk/yanlış
/// yönlendirme büyük dosyaları sessizce diske yazmasın.
pub(crate) async fn fetch_bytes(
    client: &Client,
    url: &str,
    headers: RequestHeaders<'_>,
    limit: Option<u64>,
) -> Result<Vec<u8>, String> {
    fetch_inner(client, url, headers, None, limit).await
}

/// Byte-range ile indirir (`bytes=start-...`). SegmentBase/SegmentList için.
pub(crate) async fn fetch_range(
    client: &Client,
    url: &str,
    headers: RequestHeaders<'_>,
    range: (u64, Option<u64>),
    limit: Option<u64>,
) -> Result<Vec<u8>, String> {
    fetch_inner(client, url, headers, Some(range), limit).await
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
}
