//! Indirme motoru: iptal sinyali, global hiz limiti ve HTTP indirici.

pub mod dash;
pub mod fetch;
pub mod hls;
pub mod http;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};
use tokio::time::Instant;

/// Tum indirmeler arasinda paylasilan iptal sinyali.
#[derive(Debug)]
pub struct CancelToken {
    tx: watch::Sender<bool>,
    rx: watch::Receiver<bool>,
}

impl CancelToken {
    pub fn new() -> Arc<Self> {
        let (tx, rx) = watch::channel(false);
        Arc::new(Self { tx, rx })
    }

    pub fn cancel(&self) {
        let _ = self.tx.send(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.rx.borrow()
    }

    pub async fn cancelled(&self) {
        let mut rx = self.rx.clone();
        if *rx.borrow() {
            return;
        }
        // Sinyal yalnizca bir kez gelir; hata (sender dustu) da iptal sayilir.
        let _ = rx.changed().await;
    }
}

/// Havuzlu (global) token bucket: tum baglantilar ayni butceden yer.
/// `limit == 0` sinirsiz demektir.
#[derive(Debug)]
pub struct RateLimiter {
    limit: AtomicU64,
    next_free: Mutex<Instant>,
}

impl RateLimiter {
    pub fn new(limit_bytes_per_sec: u64) -> Arc<Self> {
        Arc::new(Self {
            limit: AtomicU64::new(limit_bytes_per_sec),
            next_free: Mutex::new(Instant::now()),
        })
    }

    pub fn set_limit(&self, limit_bytes_per_sec: u64) {
        self.limit.store(limit_bytes_per_sec, Ordering::Relaxed);
    }

    /// `bytes` kadar veri icin siraya gir; gerekiyorsa uyur.
    pub async fn consume(&self, bytes: u64) {
        let limit = self.limit.load(Ordering::Relaxed);
        if limit == 0 || bytes == 0 {
            return;
        }
        let now = Instant::now();
        let mut next = self.next_free.lock().await;
        // Uzun bosluktan sonra birikmis "kredi" olusmasin.
        let start = (*next).max(now);
        let duration = Duration::from_secs_f64(bytes as f64 / limit as f64);
        *next = start + duration;
        drop(next);
        if start > now {
            tokio::time::sleep_until(start).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn iptal_sinyali_bekleyeni_uyandirir() {
        let token = CancelToken::new();
        let t2 = token.clone();
        let waiter = tokio::spawn(async move { t2.cancelled().await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!token.is_cancelled());
        token.cancel();
        tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .unwrap()
            .unwrap();
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn iptal_edilmis_token_hemen_doner() {
        let token = CancelToken::new();
        token.cancel();
        tokio::time::timeout(Duration::from_millis(50), token.cancelled())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn hiz_limiti_zaman_ekler() {
        let limiter = RateLimiter::new(100_000); // 100 KB/s
        let start = Instant::now();
        limiter.consume(50_000).await;
        limiter.consume(50_000).await;
        // Ikinci 50 KB en erken ~0.5 sn sonra tamamlanmali.
        assert!(
            start.elapsed() >= Duration::from_millis(400),
            "elapsed={:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn sinirsiz_limiter_hemen_doner() {
        let limiter = RateLimiter::new(0);
        let start = Instant::now();
        limiter.consume(10_000_000).await;
        assert!(start.elapsed() < Duration::from_millis(100));
    }
}
