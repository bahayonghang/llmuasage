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
