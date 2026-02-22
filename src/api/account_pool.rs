use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::error::ApiError;
use super::rate_limiter::{ApiPermit, ApiQuotaTracker};

/// Credentials for a single ScreenScraper account.
#[derive(Debug, Clone)]
pub struct AccountCredentials {
    pub ssid: String,
    pub sspassword: String,
}

/// A single account with its own quota tracker.
struct AccountEntry {
    credentials: AccountCredentials,
    tracker: ApiQuotaTracker,
    exhausted: AtomicBool,
}

/// Pool of accounts with smart rotation.
/// When the current account's quota is exhausted, advances to the next.
pub struct AccountPool {
    entries: Vec<AccountEntry>,
    current: AtomicUsize,
}

/// Permit returned by AccountPool::acquire().
/// Holds the rate-limit permit and the credentials to use.
pub struct AccountPermit {
    pub ssid: String,
    pub sspassword: String,
    pub account_index: usize,
    _permit: ApiPermit,
}

/// Info needed to construct an account entry.
pub struct AccountInfo {
    pub ssid: String,
    pub sspassword: String,
    pub max_threads: u32,
    pub max_per_minute: u32,
    pub daily_limit: u32,
    pub requests_today: u32,
}

impl AccountPool {
    /// Create a pool from account info (fetched at startup).
    pub fn new(accounts: Vec<AccountInfo>) -> Self {
        let entries = accounts
            .into_iter()
            .map(|info| AccountEntry {
                credentials: AccountCredentials { ssid: info.ssid, sspassword: info.sspassword },
                tracker: ApiQuotaTracker::new(
                    info.max_threads,
                    info.max_per_minute,
                    info.daily_limit,
                    info.requests_today,
                ),
                exhausted: AtomicBool::new(false),
            })
            .collect();

        Self { entries, current: AtomicUsize::new(0) }
    }

    /// Create a pool for anonymous mode (no user credentials).
    pub fn anonymous(max_threads: u32) -> Self {
        Self {
            entries: vec![AccountEntry {
                credentials: AccountCredentials { ssid: String::new(), sspassword: String::new() },
                tracker: ApiQuotaTracker::new(max_threads, 1, 10000, 0),
                exhausted: AtomicBool::new(false),
            }],
            current: AtomicUsize::new(0),
        }
    }

    /// Acquire a permit from the next available account.
    /// Rotates to the next account if the current one is exhausted.
    pub async fn acquire(&self) -> Result<AccountPermit, ApiError> {
        let len = self.entries.len();
        let start = self.current.load(Ordering::Relaxed);

        for i in 0..len {
            let idx = (start + i) % len;
            let entry = &self.entries[idx];

            if entry.exhausted.load(Ordering::Relaxed) {
                continue;
            }

            if entry.tracker.is_daily_exhausted() {
                entry.exhausted.store(true, Ordering::Relaxed);
                if i == 0 {
                    tracing::warn!(
                        "Account '{}' daily quota exhausted, rotating to next",
                        entry.credentials.ssid
                    );
                }
                continue;
            }

            match entry.tracker.acquire().await {
                Ok(permit) => {
                    // Advance current pointer if we rotated
                    if i > 0 {
                        self.current.store(idx, Ordering::Relaxed);
                        tracing::info!(
                            "Rotated to account '{}' (account {}/{})",
                            entry.credentials.ssid,
                            idx + 1,
                            len
                        );
                    }
                    return Ok(AccountPermit {
                        ssid: entry.credentials.ssid.clone(),
                        sspassword: entry.credentials.sspassword.clone(),
                        account_index: idx,
                        _permit: permit,
                    });
                }
                Err(ApiError::DailyQuotaExceeded) => {
                    entry.exhausted.store(true, Ordering::Relaxed);
                }
                Err(e) => return Err(e),
            }
        }

        Err(ApiError::AllAccountsExhausted)
    }

    /// Force-mark an account as exhausted (e.g., when API returns 430).
    pub fn mark_exhausted(&self, account_index: usize) {
        if let Some(entry) = self.entries.get(account_index) {
            entry.exhausted.store(true, Ordering::Relaxed);
            tracing::warn!("Account '{}' marked as exhausted", entry.credentials.ssid);
        }
    }

    /// Total requests made across all accounts today.
    pub fn total_requests_today(&self) -> u32 {
        self.entries.iter().map(|e| e.tracker.requests_today()).sum()
    }

    /// Total daily limit across all accounts.
    pub fn total_daily_limit(&self) -> u32 {
        self.entries.iter().map(|e| e.tracker.daily_limit()).sum()
    }

    /// Number of configured accounts.
    pub fn account_count(&self) -> usize {
        self.entries.len()
    }

    /// Get the name of the currently active account.
    #[cfg(test)]
    pub fn active_account_name(&self) -> &str {
        let idx = self.current.load(Ordering::Relaxed) % self.entries.len();
        &self.entries[idx].credentials.ssid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_account_info(ssid: &str, daily_limit: u32, requests_today: u32) -> AccountInfo {
        AccountInfo {
            ssid: ssid.to_string(),
            sspassword: format!("pass_{ssid}"),
            max_threads: 2,
            max_per_minute: 1000,
            daily_limit,
            requests_today,
        }
    }

    #[test]
    fn test_new_pool() {
        let pool = AccountPool::new(vec![
            test_account_info("user1", 5000, 0),
            test_account_info("user2", 5000, 0),
        ]);
        assert_eq!(pool.account_count(), 2);
        assert_eq!(pool.active_account_name(), "user1");
    }

    #[test]
    fn test_anonymous_pool() {
        let pool = AccountPool::anonymous(1);
        assert_eq!(pool.account_count(), 1);
        assert_eq!(pool.active_account_name(), "");
    }

    #[tokio::test]
    async fn test_acquire_returns_first_account() {
        let pool = AccountPool::new(vec![
            test_account_info("user1", 5000, 0),
            test_account_info("user2", 5000, 0),
        ]);
        let permit = pool.acquire().await.unwrap();
        assert_eq!(permit.ssid, "user1");
        assert_eq!(permit.sspassword, "pass_user1");
        assert_eq!(permit.account_index, 0);
        drop(permit);
    }

    #[tokio::test]
    async fn test_acquire_rotates_on_exhaustion() {
        let pool = AccountPool::new(vec![
            test_account_info("user1", 100, 100), // Already at limit
            test_account_info("user2", 5000, 0),
        ]);
        let permit = pool.acquire().await.unwrap();
        assert_eq!(permit.ssid, "user2");
        assert_eq!(permit.account_index, 1);
        drop(permit);
    }

    #[tokio::test]
    async fn test_acquire_all_exhausted() {
        let pool = AccountPool::new(vec![
            test_account_info("user1", 100, 100),
            test_account_info("user2", 200, 200),
        ]);
        assert!(matches!(pool.acquire().await, Err(ApiError::AllAccountsExhausted)));
    }

    #[tokio::test]
    async fn test_mark_exhausted() {
        let pool = AccountPool::new(vec![
            test_account_info("user1", 5000, 0),
            test_account_info("user2", 5000, 0),
        ]);

        // First acquire returns user1
        let permit = pool.acquire().await.unwrap();
        assert_eq!(permit.ssid, "user1");
        drop(permit);

        // Mark user1 as exhausted
        pool.mark_exhausted(0);

        // Next acquire should return user2
        let permit = pool.acquire().await.unwrap();
        assert_eq!(permit.ssid, "user2");
        drop(permit);
    }

    #[tokio::test]
    async fn test_mark_all_exhausted() {
        let pool = AccountPool::new(vec![
            test_account_info("user1", 5000, 0),
            test_account_info("user2", 5000, 0),
        ]);
        pool.mark_exhausted(0);
        pool.mark_exhausted(1);
        assert!(matches!(pool.acquire().await, Err(ApiError::AllAccountsExhausted)));
    }

    #[tokio::test]
    async fn test_single_account_pool() {
        let pool = AccountPool::new(vec![test_account_info("only_user", 5000, 0)]);
        let permit = pool.acquire().await.unwrap();
        assert_eq!(permit.ssid, "only_user");
        assert_eq!(permit.account_index, 0);
        drop(permit);
    }

    #[test]
    fn test_total_requests_and_limits() {
        let pool = AccountPool::new(vec![
            test_account_info("user1", 5000, 100),
            test_account_info("user2", 3000, 50),
        ]);
        assert_eq!(pool.total_requests_today(), 150);
        assert_eq!(pool.total_daily_limit(), 8000);
    }

    #[tokio::test]
    async fn test_acquire_wraps_around() {
        let pool = AccountPool::new(vec![
            test_account_info("user1", 100, 100), // exhausted
            test_account_info("user2", 100, 100), // exhausted
            test_account_info("user3", 5000, 0),  // available
        ]);
        let permit = pool.acquire().await.unwrap();
        assert_eq!(permit.ssid, "user3");
        assert_eq!(permit.account_index, 2);
        drop(permit);
    }

    #[test]
    fn test_mark_exhausted_invalid_index() {
        let pool = AccountPool::new(vec![test_account_info("user1", 5000, 0)]);
        // Should not panic
        pool.mark_exhausted(999);
    }
}
