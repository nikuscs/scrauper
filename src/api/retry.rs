use std::future::Future;
use std::time::Duration;

use crate::config::Network;

use super::error::ApiError;

/// Execute an async operation with exponential backoff retry.
pub async fn with_retry<F, Fut, T>(config: &Network, operation: F) -> Result<T, ApiError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, ApiError>>,
{
    for attempts in 0.. {
        let outcome = operation().await;
        let err = match outcome {
            Ok(result) => return Ok(result),
            Err(err) => err,
        };

        if err.is_fatal() {
            tracing::error!("Fatal API error (aborting): {}", err);
            return Err(err);
        }
        if !err.is_retryable() || attempts >= config.max_retries {
            return Err(err);
        }

        let attempt_num = attempts + 1;
        let base_delay = retry_delay(config, attempt_num);
        let delay = if matches!(err, ApiError::ApiClosedOverloaded) {
            Duration::from_secs(60)
        } else {
            base_delay
        };
        tokio::time::sleep(delay).await;
    }

    unreachable!("retry loop must return or error");
}

fn retry_delay(config: &Network, attempt: u32) -> Duration {
    let base_ms = config.retry_base_delay_ms;
    let delay_ms = base_ms * 2u64.pow(attempt.saturating_sub(1));
    Duration::from_millis(delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_network() -> Network {
        Network {
            max_retries: 3,
            retry_base_delay_ms: 100,
            max_threads_override: 0,
            request_timeout_secs: 30,
            download_timeout_secs: 120,
        }
    }

    #[test]
    fn test_retry_delay_first_attempt() {
        let config = test_network();
        assert_eq!(retry_delay(&config, 1), Duration::from_millis(100));
    }

    #[test]
    fn test_retry_delay_exponential() {
        let config = test_network();
        assert_eq!(retry_delay(&config, 2), Duration::from_millis(200));
        assert_eq!(retry_delay(&config, 3), Duration::from_millis(400));
    }

    #[test]
    fn test_retry_delay_zero_attempt() {
        let config = test_network();
        // saturating_sub(1) on 0 = 0, so 2^0 = 1
        assert_eq!(retry_delay(&config, 0), Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_with_retry_succeeds_first_try() {
        let config = test_network();
        let result = with_retry(&config, || async { Ok::<_, ApiError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_retry_fatal_no_retry() {
        let config = test_network();
        let call_count = std::sync::atomic::AtomicU32::new(0);
        let result = with_retry(&config, || {
            call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async { Err::<i32, ApiError>(ApiError::InvalidCredentials) }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_with_retry_non_retryable_no_retry() {
        let config = test_network();
        let call_count = std::sync::atomic::AtomicU32::new(0);
        let result = with_retry(&config, || {
            call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async { Err::<i32, ApiError>(ApiError::GameNotFound) }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_with_retry_retries_on_retryable_error() {
        let mut config = test_network();
        config.max_retries = 2;
        config.retry_base_delay_ms = 10; // Very short for test speed

        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        let result = with_retry(&config, || {
            let count = cc.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async move {
                if count < 1 {
                    Err::<i32, ApiError>(ApiError::RateLimited)
                } else {
                    Ok(99)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 99);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_with_retry_exhausts_retries() {
        let mut config = test_network();
        config.max_retries = 2;
        config.retry_base_delay_ms = 10;

        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        let result = with_retry(&config, || {
            cc.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async { Err::<i32, ApiError>(ApiError::RateLimited) }
        })
        .await;

        assert!(result.is_err());
        // Initial call + 2 retries = 3 total
        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 3);
    }

    #[test]
    fn test_retry_delay_high_attempt() {
        let config = test_network();
        // attempt 4: base * 2^3 = 100 * 8 = 800
        assert_eq!(retry_delay(&config, 4), Duration::from_millis(800));
    }

    #[tokio::test]
    async fn test_with_retry_overloaded_exhausts_retries() {
        // ApiClosedOverloaded is retryable, so with max_retries=0 it should fail immediately
        let mut config = test_network();
        config.max_retries = 0;
        config.retry_base_delay_ms = 10;

        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        let result = with_retry(&config, || {
            cc.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async { Err::<i32, ApiError>(ApiError::ApiClosedOverloaded) }
        })
        .await;

        assert!(result.is_err());
        // max_retries=0 means 1 attempt, no retries
        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_with_retry_retryable_then_success_on_third() {
        let mut config = test_network();
        config.max_retries = 3;
        config.retry_base_delay_ms = 10;

        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        let result = with_retry(&config, || {
            let count = cc.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async move {
                if count < 2 {
                    Err::<i32, ApiError>(ApiError::RateLimited)
                } else {
                    Ok(77)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 77);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn test_with_retry_overloaded_uses_60s_delay() {
        // Use tokio::time::pause() to avoid actually waiting 60 seconds
        tokio::time::pause();

        let mut config = test_network();
        config.max_retries = 1;
        config.retry_base_delay_ms = 100;

        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        let start = tokio::time::Instant::now();
        let result = with_retry(&config, || {
            let count = cc.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async move {
                if count < 1 {
                    Err::<i32, ApiError>(ApiError::ApiClosedOverloaded)
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        let elapsed = start.elapsed();
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 2);
        // With paused time, the 60s sleep is instant but the elapsed time reflects it
        assert!(elapsed >= Duration::from_secs(60), "Expected >=60s delay, got {:?}", elapsed);
    }

    #[tokio::test]
    async fn test_with_retry_normal_error_uses_exponential_delay() {
        tokio::time::pause();

        let mut config = test_network();
        config.max_retries = 1;
        config.retry_base_delay_ms = 500;

        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        let start = tokio::time::Instant::now();
        let result = with_retry(&config, || {
            let count = cc.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            async move {
                if count < 1 {
                    Err::<i32, ApiError>(ApiError::RateLimited)
                } else {
                    Ok(99)
                }
            }
        })
        .await;

        let elapsed = start.elapsed();
        assert_eq!(result.unwrap(), 99);
        // RateLimited should use exponential backoff (500ms for attempt 1), NOT 60s
        assert!(elapsed < Duration::from_secs(5), "Expected <5s delay, got {:?}", elapsed);
    }
}
