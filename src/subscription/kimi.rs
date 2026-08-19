use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;

use super::http::{client, status_error};
use super::types::{UsageMetric, UsageOutput, capitalize};

#[derive(Debug, Deserialize)]
struct Credentials {
    access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    usage: Option<QuotaDetail>,
    limits: Option<Vec<LimitEntry>>,
    user: Option<UserInfo>,
}

#[derive(Debug, Deserialize)]
struct QuotaDetail {
    limit: Option<String>,
    remaining: Option<String>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LimitEntry {
    window: Option<LimitWindow>,
    detail: Option<QuotaDetail>,
}

#[derive(Debug, Deserialize)]
struct LimitWindow {
    duration: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    membership: Option<Membership>,
}

#[derive(Debug, Deserialize)]
struct Membership {
    level: Option<String>,
}

pub fn credentials_paths(user_home: &Path) -> Vec<PathBuf> {
    let kimi_code_home = std::env::var("KIMI_CODE_HOME")
        .ok()
        .filter(|home| !home.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home.join(".kimi-code"));
    vec![
        kimi_code_home.join("credentials").join("kimi-code.json"),
        user_home
            .join(".kimi")
            .join("credentials")
            .join("kimi-code.json"),
    ]
}

pub fn has_credentials(user_home: &Path) -> bool {
    credentials_paths(user_home)
        .iter()
        .any(|path| path.exists())
}

pub async fn fetch(user_home: &Path, usage_url: &str, timeout: Duration) -> Result<UsageOutput> {
    let path = credentials_paths(user_home)
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| anyhow::anyhow!("No Kimi credentials found. Run 'kimi' to log in."))?;
    let before = std::fs::read(&path)?;
    let creds: Credentials = serde_json::from_slice(&before)?;
    let access_token = creds
        .access_token
        .ok_or_else(|| anyhow::anyhow!("No Kimi access token."))?;

    let http = client(timeout)?;
    let response = http
        .get(usage_url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Accept", "application/json")
        .send()
        .await?;
    let status = response.status();
    let after = std::fs::read(&path).unwrap_or_default();
    if after != before {
        anyhow::bail!("Kimi credentials were modified during fetch");
    }
    if !status.is_success() {
        return Err(status_error("Kimi", status));
    }
    let body: UsageResponse = response.json().await?;

    let plan = body
        .user
        .as_ref()
        .and_then(|user| user.membership.as_ref())
        .and_then(|membership| membership.level.as_ref())
        .map(|level| {
            capitalize(
                level
                    .trim_start_matches("LEVEL_")
                    .replace('_', " ")
                    .as_str(),
            )
        });

    let mut metrics = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(limits) = &body.limits {
        for entry in limits {
            if let Some(detail) = &entry.detail {
                let label = match entry.window.as_ref().and_then(|window| window.duration) {
                    Some(duration) if duration <= 3600 => "Session",
                    _ => "Weekly",
                };
                if let Some(metric) = parse_quota_detail(label, detail) {
                    let key = metric_key(&metric);
                    if seen.insert(key) {
                        metrics.push(metric);
                    }
                }
            }
        }
    }
    if let Some(usage) = &body.usage
        && let Some(metric) = parse_quota_detail("Weekly", usage)
    {
        let key = metric_key(&metric);
        if seen.insert(key) {
            metrics.push(metric);
        }
    }

    Ok(UsageOutput {
        provider: "Kimi".into(),
        account: None,
        credential_source: None,
        plan,
        email: None,
        metrics,
    })
}

fn metric_key(metric: &UsageMetric) -> String {
    format!(
        "{}:{}:{}:{}",
        metric.label,
        metric.used_percent,
        metric.remaining_label.as_deref().unwrap_or(""),
        metric.resets_at.as_deref().unwrap_or("")
    )
}

fn parse_quota_detail(label: &str, detail: &QuotaDetail) -> Option<UsageMetric> {
    let limit: i64 = detail.limit.as_ref()?.parse().ok()?;
    let remaining: i64 = detail.remaining.as_ref()?.parse().ok()?;
    if limit <= 0 {
        return None;
    }
    let used = (limit - remaining).max(0);
    let used_pct = (used as f64 / limit as f64 * 100.0).clamp(0.0, 100.0);
    Some(UsageMetric {
        label: label.into(),
        used_percent: used_pct,
        remaining_percent: 100.0 - used_pct,
        remaining_label: Some(format!("{remaining}/{limit} left")),
        resets_at: detail.reset_time.clone(),
    })
}
