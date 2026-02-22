use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::error::ApiError;

type GovernorLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Three-level concurrency control for ScreenScraper API:
/// 1. Semaphore: limits concurrent requests to maxthreads
/// 2. Governor: limits requests per minute
/// 3. AtomicU32: tracks daily request count
pub struct ApiQuotaTracker {
    thread_semaphore: Arc<Semaphore>,
    rate_limiter: Arc<GovernorLimiter>,
    daily_counter: Arc<AtomicU32>,
    daily_limit: u32,
    #[allow(dead_code)]
    max_threads: u32,
}

impl ApiQuotaTracker {
    pub fn new(
        max_threads: u32,
        max_per_minute: u32,
        daily_limit: u32,
        requests_today: u32,
    ) -> Self {
        let thread_semaphore = Arc::new(Semaphore::new(max_threads as usize));

        let per_minute = NonZeroU32::new(max_per_minute.max(1)).unwrap();
        let rate_limiter = Arc::new(RateLimiter::direct(Quota::per_minute(per_minute)));

        let daily_counter = Arc::new(AtomicU32::new(requests_today));

        Self { thread_semaphore, rate_limiter, daily_counter, daily_limit, max_threads }
    }

    /// Acquire a permit to make an API request.
    /// Waits for: thread semaphore + rate limiter. Checks daily quota.
    pub async fn acquire(&self) -> Result<ApiPermit, ApiError> {
        // Check daily quota first
        if self.is_daily_exhausted() {
            return Err(ApiError::DailyQuotaExceeded);
        }

        // Acquire thread permit (blocks if all threads in use)
        let permit = self
            .thread_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ApiError::DailyQuotaExceeded)?;

        // Wait for rate limiter
        self.rate_limiter.until_ready().await;

        // Increment daily counter
        self.daily_counter.fetch_add(1, Ordering::Relaxed);

        Ok(ApiPermit { _permit: permit })
    }

    pub fn is_daily_exhausted(&self) -> bool {
        self.daily_limit > 0 && self.daily_counter.load(Ordering::Relaxed) >= self.daily_limit
    }

    pub fn requests_today(&self) -> u32 {
        self.daily_counter.load(Ordering::Relaxed)
    }

    pub fn daily_limit(&self) -> u32 {
        self.daily_limit
    }

    #[allow(dead_code)]
    pub fn max_threads(&self) -> u32 {
        self.max_threads
    }
}

/// RAII permit that releases the thread semaphore on drop.
pub struct ApiPermit {
    _permit: OwnedSemaphorePermit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tracker() {
        let tracker = ApiQuotaTracker::new(4, 30, 5000, 100);
        assert_eq!(tracker.daily_limit(), 5000);
        assert_eq!(tracker.requests_today(), 100);
        assert_eq!(tracker.max_threads(), 4);
        assert!(!tracker.is_daily_exhausted());
    }

    #[test]
    fn test_daily_exhausted_at_limit() {
        let tracker = ApiQuotaTracker::new(1, 10, 100, 100);
        assert!(tracker.is_daily_exhausted());
    }

    #[test]
    fn test_daily_exhausted_over_limit() {
        let tracker = ApiQuotaTracker::new(1, 10, 100, 150);
        assert!(tracker.is_daily_exhausted());
    }

    #[test]
    fn test_daily_not_exhausted_zero_limit() {
        // daily_limit=0 means unlimited
        let tracker = ApiQuotaTracker::new(1, 10, 0, 999);
        assert!(!tracker.is_daily_exhausted());
    }

    #[tokio::test]
    async fn test_acquire_increments_counter() {
        let tracker = ApiQuotaTracker::new(2, 1000, 5000, 0);
        let _permit = tracker.acquire().await.unwrap();
        assert_eq!(tracker.requests_today(), 1);
    }

    #[tokio::test]
    async fn test_acquire_fails_when_exhausted() {
        let tracker = ApiQuotaTracker::new(1, 10, 100, 100);
        assert!(tracker.acquire().await.is_err());
    }

    #[tokio::test]
    async fn test_permit_drop_releases_semaphore() {
        let tracker = ApiQuotaTracker::new(1, 1000, 5000, 0);
        {
            let _permit = tracker.acquire().await.unwrap();
            // Semaphore is held
        }
        // After permit drop, semaphore should be available again
        let _permit2 = tracker.acquire().await.unwrap();
        assert_eq!(tracker.requests_today(), 2);
    }
}
