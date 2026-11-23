//! Indirme motoru: iptal sinyali, global hiz limiti ve HTTP indirici.

pub mod dash;
pub mod fetch;
pub mod hls;
pub mod http;

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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
    /// Su anda indiren **acik indirme** sayisi (baglanti degil, indirme).
    active: AtomicUsize,
    next_free: Mutex<Instant>,
}

/// Paylasimda indirme basina alt sinir: cok kalabalikta akis tamamen durmasin.
const MIN_SHARE_BYTES: u64 = 1024;

impl RateLimiter {
    pub fn new(limit_bytes_per_sec: u64) -> Arc<Self> {
        Arc::new(Self {
            limit: AtomicU64::new(limit_bytes_per_sec),
            active: AtomicUsize::new(1),
            next_free: Mutex::new(Instant::now()),
        })
    }

    pub fn set_limit(&self, limit_bytes_per_sec: u64) {
        self.limit.store(limit_bytes_per_sec, Ordering::Relaxed);
    }

    /// Kac indirme acik? Butce bunlara bolunur (0 = sinirsiz kalir).
    pub fn set_active(&self, active: usize) {
        self.active.store(active.max(1), Ordering::Relaxed);
    }

    /// Bu baglantinin kullanabilecegi hiz (bayt/sn). 0 = sinirsiz.
    ///
    /// Neden boluyoruz: tek bir global kova, bir indirmenin tum butceyi
    /// yemesine izin veriyordu (10 indirme varsa biri hepsini alir). Adil
    /// paylasim, kullanicinin "10 indirme" ayarinin gercekten is gormesini saglar.
    pub fn effective_limit(&self) -> u64 {
        let limit = self.limit.load(Ordering::Relaxed);
        if limit == 0 {
            return 0;
        }
        let active = self.active.load(Ordering::Relaxed).max(1) as u64;
        (limit / active).max(MIN_SHARE_BYTES.min(limit))
    }

    /// `bytes` kadar veri icin siraya gir; gerekiyorsa uyur.
    pub async fn consume(&self, bytes: u64) {
        let limit = self.effective_limit();
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

    /// Butce acik indirmelere bolunur; tek indirme tum limiti almaz.
    #[test]
    fn hiz_butcesi_acik_indirmelere_paylastirilir() {
        let limiter = RateLimiter::new(1_000_000);
        // Tek indirme: tum butce.
        assert_eq!(limiter.effective_limit(), 1_000_000);
        // 4 indirme: her biri ceyrek.
        limiter.set_active(4);
        assert_eq!(limiter.effective_limit(), 250_000);
        // Sifir/ anlamsiz sayi tek sayilir (sifira bolme yok).
        limiter.set_active(0);
        assert_eq!(limiter.effective_limit(), 1_000_000);
        // Sinirsiz limitte paylasim da sinirsiz kalir.
        let unlimited = RateLimiter::new(0);
        unlimited.set_active(8);
        assert_eq!(unlimited.effective_limit(), 0, "0 = sinirsiz");
    }

    /// Paylasimdan sonra bile alt sinir korunur: cok indirme varsa her biri
    /// en az 1 KB/s alir (akis tamamen durmasin).
    #[test]
    fn paylasim_sifira_dusurmez() {
        let limiter = RateLimiter::new(1_000_000);
        limiter.set_active(5_000);
        assert!(
            limiter.effective_limit() >= 1024,
            "cok bolmede bile alt sinir var: {}",
            limiter.effective_limit()
        );
        // Alt sinir limitin kendisini asamaz (limit < alt sinir ise limit kalir).
        let tiny = RateLimiter::new(500);
        tiny.set_active(100);
        assert_eq!(tiny.effective_limit(), 500, "limitten fazla verilmez");
    }

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
