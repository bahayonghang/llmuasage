use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use serde::Deserialize;

use super::http::{client, status_error};
use super::types::{UsageMetric, UsageOutput, capitalize};

#[derive(Debug, Deserialize)]
struct Auth {
    tokens: Option<Tokens>,
}

#[derive(Debug, Deserialize)]
struct Tokens {
    access_token: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeAuthDocument {
    openai: Option<OpenCodeCredential>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum OpenCodeCredential {
    #[serde(rename = "oauth")]
    Oauth {
        access: String,
        #[serde(rename = "accountId")]
        account_id: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Usage {
    email: Option<String>,
    plan_type: Option<String>,
    rate_limit: Option<RateLimit>,
    #[serde(default)]
    additional_rate_limits: Vec<AdditionalRateLimit>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct RateLimit {
    primary_window: Option<Window>,
    secondary_window: Option<Window>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Window {
    used_percent: Option<f64>,
    limit_window_seconds: Option<i64>,
    #[serde(alias = "resets_at")]
    reset_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct AdditionalRateLimit {
    metered_feature: Option<String>,
    limit_name: Option<String>,
    rate_limit: Option<RateLimit>,
}

struct CodexAuth {
    access_token: String,
    account_id: Option<String>,
    credential_source: Option<String>,
    path: PathBuf,
}

pub fn has_credentials(user_home: &Path) -> bool {
    auth_candidates(user_home).iter().any(|path| path.exists())
        || opencode_auth_path(user_home).exists()
}

pub async fn fetch(user_home: &Path, usage_url: &str, timeout: Duration) -> Result<UsageOutput> {
    let auth = read_auth(user_home)?;
    let before = std::fs::read(&auth.path)?;
    let http = client(timeout)?;
    let mut request = http
        .get(usage_url)
        .header("Authorization", format!("Bearer {}", auth.access_token))
        .header("Accept", "application/json");
    if let Some(account_id) = &auth.account_id {
        request = request.header("ChatGPT-Account-Id", account_id);
    }
    let response = request.send().await?;
    let status = response.status();
    let after = std::fs::read(&auth.path).unwrap_or_default();
    if after != before {
        anyhow::bail!("Codex credentials were modified during fetch");
    }
    if !status.is_success() {
        return Err(status_error("Codex", status));
    }
    let body: Usage = response.json().await?;
    let plan = body.plan_type.as_deref().map(capitalize);
    let mut metrics = Vec::new();
    if let Some(rate_limit) = &body.rate_limit {
        push_rate_limit_metrics(&mut metrics, None, rate_limit);
    }
    for limit in &body.additional_rate_limits {
        if let Some(rate_limit) = &limit.rate_limit {
            let label = limit
                .limit_name
                .as_deref()
                .or(limit.metered_feature.as_deref())
                .map(capitalize);
            push_rate_limit_metrics(&mut metrics, label.as_deref(), rate_limit);
        }
    }

    Ok(UsageOutput {
        provider: "Codex".into(),
        account: None,
        credential_source: auth.credential_source,
        plan,
        email: body.email,
        metrics,
    })
}

fn auth_candidates(user_home: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(codex_home) = std::env::var("CODEX_HOME")
        && !codex_home.trim().is_empty()
    {
        paths.push(PathBuf::from(codex_home).join("auth.json"));
    }
    paths.push(user_home.join(".config").join("codex").join("auth.json"));
    paths.push(user_home.join(".codex").join("auth.json"));
    paths
}

fn opencode_auth_path(user_home: &Path) -> PathBuf {
    let data_dir = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home.join(".local").join("share"));
    data_dir.join("opencode").join("auth.json")
}

fn read_auth(user_home: &Path) -> Result<CodexAuth> {
    for path in auth_candidates(user_home) {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        if let Ok(auth) = serde_json::from_str::<Auth>(&content)
            && let Some(tokens) = auth.tokens
            && let Some(access_token) = tokens.access_token.filter(|token| !token.is_empty())
        {
            return Ok(CodexAuth {
                access_token,
                account_id: tokens.account_id,
                credential_source: None,
                path,
            });
        }
    }

    let path = opencode_auth_path(user_home);
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        let document: OpenCodeAuthDocument = serde_json::from_str(&content)?;
        if let Some(OpenCodeCredential::Oauth { access, account_id }) = document.openai
            && !access.trim().is_empty()
        {
            return Ok(CodexAuth {
                access_token: access,
                account_id,
                credential_source: Some("opencode".into()),
                path,
            });
        }
    }

    anyhow::bail!("No Codex credentials found. Run 'codex' to log in.")
}

fn metric_from_window(label: &str, window: &Window) -> UsageMetric {
    let pct = window.used_percent.unwrap_or(0.0).clamp(0.0, 100.0);
    UsageMetric {
        label: label.into(),
        used_percent: pct,
        remaining_percent: 100.0 - pct,
        remaining_label: None,
        resets_at: window
            .reset_at
            .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
            .map(|dt| dt.to_rfc3339()),
    }
}

fn rate_limit_window_label(window: &Window) -> Option<String> {
    let seconds = window.limit_window_seconds.filter(|seconds| *seconds > 0)?;
    if seconds == 7 * 24 * 60 * 60 {
        return Some("Weekly".to_string());
    }
    if seconds % (24 * 60 * 60) == 0 {
        return Some(format!("{}d", seconds / (24 * 60 * 60)));
    }
    if seconds % (60 * 60) == 0 {
        return Some(format!("{}h", seconds / (60 * 60)));
    }
    Some(format!("{seconds}s"))
}

fn metric_label(prefix: Option<&str>, window: &Window, fallback: &str) -> String {
    let dynamic = rate_limit_window_label(window);
    match prefix {
        Some(prefix) => format!(
            "{prefix} {}",
            dynamic
                .map(|label| label.to_ascii_lowercase())
                .unwrap_or_else(|| fallback.to_string())
        ),
        None => dynamic.unwrap_or_else(|| fallback.to_string()),
    }
}

fn push_rate_limit_metrics(
    metrics: &mut Vec<UsageMetric>,
    prefix: Option<&str>,
    rate_limit: &RateLimit,
) {
    if let Some(window) = &rate_limit.primary_window {
        metrics.push(metric_from_window(
            &metric_label(prefix, window, "5h"),
            window,
        ));
    }
    if let Some(window) = &rate_limit.secondary_window {
        metrics.push(metric_from_window(
            &metric_label(prefix, window, "Weekly"),
            window,
        ));
    }
}
