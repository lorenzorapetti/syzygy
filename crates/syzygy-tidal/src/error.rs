/// Everything that can go wrong talking to TIDAL.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The API answered with a non-success status.
    #[error("API error ({status}): {body}")]
    Api { status: u16, body: String },

    /// JSON deserialization or another parse failure.
    #[error("Parse error: {0}")]
    Parse(String),

    /// Network or transport failure (timeout, DNS, connection refused).
    #[error("Network error: {0}")]
    Network(String),

    /// No tokens: nobody is signed in.
    #[error("Not authenticated")]
    NotAuthenticated,

    /// The refresh token was rejected (`invalid_grant`). The user has to sign
    /// in again; `Event::SessionExpired` has gone out.
    #[error("Session expired")]
    SessionExpired,

    /// No usable embedded client credentials were compiled in.
    #[error("Not configured: {0}")]
    NotConfigured(String),
}

impl Error {
    /// A network or transport failure.
    pub fn is_network(&self) -> bool {
        matches!(self, Error::Network(_))
    }

    /// Upstream is rate-limiting us. Never retry in a loop — a 429 is usually
    /// self-inflicted, so the fix is to stop asking.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Error::Api { status: 429, .. })
    }

    /// Seconds until the rate gate opens again, for a 429 this client made or
    /// synthesized. `None` for every other error.
    pub fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Error::Api { status: 429, body } => serde_json::from_str::<serde_json::Value>(body)
                .ok()?
                .get("retryAfterSecs")?
                .as_u64(),
            _ => None,
        }
    }

    /// This specific item cannot be played and no retry will change that.
    /// 404/410/451 are catalog/licensing terminal; a 401 is terminal only when
    /// its body carries a terminal playbackinfo sub-status.
    pub fn is_terminal_unplayable(&self) -> bool {
        match self {
            Error::Api {
                status: 404 | 410 | 451,
                ..
            } => true,
            Error::Api { status: 401, body } => crate::models::is_terminal_sub_status(body),
            _ => false,
        }
    }

    /// A log-safe message that omits API response bodies (which may carry
    /// account data for `/users/` and `/sessions` endpoints).
    pub fn log_safe(&self) -> String {
        match self {
            Error::Api { status, .. } => format!("API error (status {status})"),
            other => other.to_string(),
        }
    }

    /// A synthesized 429. `retryAfterSecs` is machine-readable; `userMessage`
    /// is what the user sees.
    pub(crate) fn rate_limited(secs: u64) -> Self {
        Error::Api {
            status: 429,
            body: format!(
                r#"{{"status":429,"retryAfterSecs":{secs},"userMessage":"Too many requests — retrying in {secs}s"}}"#
            ),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Parse(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Network(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn synthesized_429_carries_retry_after_and_a_human_message() {
        match Error::rate_limited(7) {
            Error::Api { status, ref body } => {
                assert_eq!(status, 429);
                let v: serde_json::Value =
                    serde_json::from_str(body).expect("body must be valid JSON");
                assert_eq!(v["status"].as_u64(), Some(429));
                assert_eq!(v["retryAfterSecs"].as_u64(), Some(7));
                assert!(!v["userMessage"].as_str().unwrap_or_default().is_empty());
            }
            other => panic!("expected Error::Api, got {other:?}"),
        }
        assert_eq!(Error::rate_limited(7).retry_after_secs(), Some(7));
    }

    #[test]
    fn terminal_and_rate_limited_errors_are_classified() {
        let api = |status, body: &str| Error::Api {
            status,
            body: body.into(),
        };

        let rl = api(429, "");
        assert!(rl.is_rate_limited());
        assert!(!rl.is_terminal_unplayable());

        for status in [404, 410, 451] {
            let e = api(status, "");
            assert!(e.is_terminal_unplayable(), "{status}");
            assert!(!e.is_rate_limited(), "{status}");
        }

        assert!(api(401, r#"{"status":401,"subStatus":4005}"#).is_terminal_unplayable());
        assert!(!api(401, r#"{"status":401,"subStatus":11003}"#).is_terminal_unplayable());
        assert!(!api(401, "").is_terminal_unplayable());

        let server = api(500, "");
        assert!(!server.is_rate_limited());
        assert!(!server.is_terminal_unplayable());
    }
}
