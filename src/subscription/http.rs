use std::time::Duration;

use anyhow::Result;
use reqwest::StatusCode;

pub fn client(timeout: Duration) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("llmusage")
        .build()?)
}

pub fn status_error(provider: &str, status: StatusCode) -> anyhow::Error {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        anyhow::anyhow!(
            "{provider} usage unavailable: stored access token was rejected (HTTP {status}). \
             Run the provider CLI so it can refresh its own login, then retry."
        )
    } else {
        anyhow::anyhow!("{provider} usage request failed (HTTP {status})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn status_error_unauthorized_mentions_rejected_token() {
        let error = status_error("Codex", StatusCode::UNAUTHORIZED).to_string();
        assert!(
            error.contains("stored access token was rejected"),
            "{error}"
        );
    }

    #[test]
    fn status_error_internal_error_is_generic_failure() {
        let error = status_error("Codex", StatusCode::INTERNAL_SERVER_ERROR).to_string();
        assert!(error.contains("usage request failed"), "{error}");
        assert!(
            !error.contains("stored access token was rejected"),
            "{error}"
        );
    }
}
