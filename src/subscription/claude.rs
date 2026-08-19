use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;

use super::http::{client, status_error};
use super::types::{UsageMetric, UsageOutput, capitalize};

const BETA_HEADER: &str = "oauth-2025-04-20";

#[derive(Debug, Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<Oauth>,
}

#[derive(Debug, Deserialize)]
struct Oauth {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
    #[serde(rename = "rateLimitTier")]
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
    seven_day_opus: Option<Window>,
}

#[derive(Debug, Deserialize)]
struct Window {
    utilization: f64,
    resets_at: Option<String>,
}

pub fn credentials_path(user_home: &Path) -> PathBuf {
    user_home.join(".claude").join(".credentials.json")
}

pub fn has_credentials(user_home: &Path) -> bool {
    credentials_path(user_home).exists()
}

pub async fn fetch(user_home: &Path, usage_url: &str, timeout: Duration) -> Result<UsageOutput> {
    let path = credentials_path(user_home);
    let content = std::fs::read_to_string(&path)?;
    let before = content.clone();
    let creds: Credentials = serde_json::from_str(&content)?;
    let oauth = creds
        .claude_ai_oauth
        .ok_or_else(|| anyhow::anyhow!("No Claude OAuth credentials. Run 'claude' to log in."))?;
    let access_token = oauth
        .access_token
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("No Claude access token."))?;
    let plan = oauth.subscription_type.as_ref().map(|kind| {
        let tier = oauth
            .rate_limit_tier
            .as_deref()
            .and_then(|value| value.rsplit('_').next());
        match tier {
            Some(mult) => format!("{} {}", capitalize(kind), mult),
            None => capitalize(kind),
        }
    });

    let http = client(timeout)?;
    let response = http
        .get(usage_url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Accept", "application/json")
        .header("anthropic-beta", BETA_HEADER)
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        let after = std::fs::read_to_string(&path).unwrap_or_default();
        debug_assert_eq!(before, after);
        return Err(status_error("Claude", status));
    }
    let body: UsageResponse = response.json().await?;
    let after = std::fs::read_to_string(&path).unwrap_or_default();
    if after != before {
        anyhow::bail!("Claude credentials were modified during fetch");
    }

    Ok(UsageOutput {
        provider: "Claude".into(),
        account: None,
        credential_source: None,
        plan,
        email: None,
        metrics: usage_metrics(&body),
    })
}

fn usage_metrics(resp: &UsageResponse) -> Vec<UsageMetric> {
    let mut metrics = Vec::new();
    if let Some(window) = &resp.five_hour {
        metrics.push(window_metric("Session", window));
    }
    if let Some(window) = &resp.seven_day {
        metrics.push(window_metric("Weekly", window));
    }
    if let Some(window) = &resp.seven_day_opus {
        metrics.push(window_metric("Opus", window));
    }
    metrics
}

fn window_metric(label: &str, window: &Window) -> UsageMetric {
    let used = window.utilization.clamp(0.0, 100.0);
    UsageMetric {
        label: label.into(),
        used_percent: used,
        remaining_percent: 100.0 - used,
        remaining_label: None,
        resets_at: window.resets_at.clone(),
    }
}
