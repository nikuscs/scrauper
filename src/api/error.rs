use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Rate limited (429): too many requests per minute or thread limit exceeded")]
    RateLimited,

    #[error("Daily quota exceeded (430): scraping limit reached for today")]
    DailyQuotaExceeded,

    #[allow(dead_code)]
    #[error("All configured accounts are exhausted for today")]
    AllAccountsExhausted,

    #[error("Too many unknown ROMs (431): too many unrecognized ROMs scraped today")]
    TooManyUnknownRoms,

    #[error("Game not found (404)")]
    GameNotFound,

    #[error("API closed for non-members (401): server CPU load too high")]
    ApiClosedOverloaded,

    #[error("API closed (423): server is down for maintenance")]
    ApiClosedDown,

    #[error("Software blacklisted (426): this software version is blocked")]
    SoftwareBlacklisted,

    #[error("Invalid credentials (403): check devid/devpassword/ssid/sspassword")]
    InvalidCredentials,

    #[error("Bad request (400): {0}")]
    BadRequest(String),

    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Failed to parse API response: {0}")]
    Deserialize(String),

    #[error("Unexpected HTTP status {0}: {1}")]
    UnexpectedStatus(u16, String),
}

impl ApiError {
    /// Map an HTTP status code to an ApiError variant.
    pub fn from_status(status: u16, body: &str) -> Self {
        match status {
            400 => ApiError::BadRequest(body.to_string()),
            401 => ApiError::ApiClosedOverloaded,
            403 => ApiError::InvalidCredentials,
            404 => ApiError::GameNotFound,
            423 => ApiError::ApiClosedDown,
            426 => ApiError::SoftwareBlacklisted,
            429 => ApiError::RateLimited,
            430 => ApiError::DailyQuotaExceeded,
            431 => ApiError::TooManyUnknownRoms,
            _ => ApiError::UnexpectedStatus(status, body.to_string()),
        }
    }

    /// Whether this error should trigger a retry.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ApiError::RateLimited | ApiError::ApiClosedOverloaded | ApiError::Request(_))
    }

    /// Whether this error should abort the entire session.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            ApiError::DailyQuotaExceeded
                | ApiError::AllAccountsExhausted
                | ApiError::TooManyUnknownRoms
                | ApiError::ApiClosedDown
                | ApiError::SoftwareBlacklisted
                | ApiError::InvalidCredentials
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_status_known_codes() {
        assert!(matches!(ApiError::from_status(400, "bad"), ApiError::BadRequest(_)));
        assert!(matches!(ApiError::from_status(401, ""), ApiError::ApiClosedOverloaded));
        assert!(matches!(ApiError::from_status(403, ""), ApiError::InvalidCredentials));
        assert!(matches!(ApiError::from_status(404, ""), ApiError::GameNotFound));
        assert!(matches!(ApiError::from_status(423, ""), ApiError::ApiClosedDown));
        assert!(matches!(ApiError::from_status(426, ""), ApiError::SoftwareBlacklisted));
        assert!(matches!(ApiError::from_status(429, ""), ApiError::RateLimited));
        assert!(matches!(ApiError::from_status(430, ""), ApiError::DailyQuotaExceeded));
        assert!(matches!(ApiError::from_status(431, ""), ApiError::TooManyUnknownRoms));
    }

    #[test]
    fn test_from_status_unknown() {
        assert!(matches!(ApiError::from_status(500, "err"), ApiError::UnexpectedStatus(500, _)));
    }

    #[test]
    fn test_is_retryable() {
        assert!(ApiError::RateLimited.is_retryable());
        assert!(ApiError::ApiClosedOverloaded.is_retryable());
        assert!(!ApiError::GameNotFound.is_retryable());
        assert!(!ApiError::InvalidCredentials.is_retryable());
        assert!(!ApiError::DailyQuotaExceeded.is_retryable());
        assert!(!ApiError::AllAccountsExhausted.is_retryable());
    }

    #[test]
    fn test_is_fatal() {
        assert!(ApiError::DailyQuotaExceeded.is_fatal());
        assert!(ApiError::TooManyUnknownRoms.is_fatal());
        assert!(ApiError::ApiClosedDown.is_fatal());
        assert!(ApiError::SoftwareBlacklisted.is_fatal());
        assert!(ApiError::InvalidCredentials.is_fatal());
        assert!(ApiError::AllAccountsExhausted.is_fatal());
        assert!(!ApiError::RateLimited.is_fatal());
        assert!(!ApiError::GameNotFound.is_fatal());
        assert!(!ApiError::ApiClosedOverloaded.is_fatal());
    }

    #[test]
    fn test_error_display() {
        let err = ApiError::RateLimited;
        let msg = format!("{}", err);
        assert!(msg.contains("429"));
    }

    #[test]
    fn test_error_display_all_variants() {
        let variants: Vec<(ApiError, &str)> = vec![
            (ApiError::RateLimited, "429"),
            (ApiError::DailyQuotaExceeded, "430"),
            (ApiError::AllAccountsExhausted, "exhausted"),
            (ApiError::TooManyUnknownRoms, "431"),
            (ApiError::GameNotFound, "404"),
            (ApiError::ApiClosedOverloaded, "401"),
            (ApiError::ApiClosedDown, "423"),
            (ApiError::SoftwareBlacklisted, "426"),
            (ApiError::InvalidCredentials, "403"),
        ];
        for (err, code) in variants {
            let msg = format!("{err}");
            assert!(msg.contains(code), "Expected '{}' in '{}'", code, msg);
        }
    }

    #[test]
    fn test_bad_request_preserves_body() {
        let err = ApiError::from_status(400, "invalid param xyz");
        if let ApiError::BadRequest(body) = err {
            assert_eq!(body, "invalid param xyz");
        } else {
            panic!("Expected BadRequest");
        }
    }

    #[test]
    fn test_unexpected_status_preserves_info() {
        let err = ApiError::from_status(502, "bad gateway");
        if let ApiError::UnexpectedStatus(code, body) = err {
            assert_eq!(code, 502);
            assert_eq!(body, "bad gateway");
        } else {
            panic!("Expected UnexpectedStatus");
        }
    }

    #[test]
    fn test_deserialize_error_display() {
        let err = ApiError::Deserialize("missing field".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("missing field"));
    }

    #[test]
    fn test_is_retryable_all_non_retryable() {
        let non_retryable = vec![
            ApiError::DailyQuotaExceeded,
            ApiError::TooManyUnknownRoms,
            ApiError::AllAccountsExhausted,
            ApiError::GameNotFound,
            ApiError::ApiClosedDown,
            ApiError::SoftwareBlacklisted,
            ApiError::InvalidCredentials,
            ApiError::BadRequest("test".to_string()),
            ApiError::Deserialize("test".to_string()),
            ApiError::UnexpectedStatus(500, "test".to_string()),
        ];
        for err in non_retryable {
            assert!(!err.is_retryable(), "{:?} should not be retryable", err);
        }
    }

    #[test]
    fn test_is_fatal_non_fatal() {
        let non_fatal = vec![
            ApiError::RateLimited,
            ApiError::GameNotFound,
            ApiError::ApiClosedOverloaded,
            ApiError::BadRequest("test".to_string()),
            ApiError::Deserialize("test".to_string()),
            ApiError::UnexpectedStatus(500, "test".to_string()),
        ];
        for err in non_fatal {
            assert!(!err.is_fatal(), "{:?} should not be fatal", err);
        }
    }
}
