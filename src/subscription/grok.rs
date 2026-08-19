use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;

use super::http::{client, status_error};
use super::types::{UsageMetric, UsageOutput};

#[derive(Debug, Clone)]
struct Credentials {
    token: String,
    email: Option<String>,
}

pub fn auth_path(user_home: &Path) -> PathBuf {
    std::env::var_os("GROK_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home.join(".grok"))
        .join("auth.json")
}

pub fn has_credentials(user_home: &Path) -> bool {
    auth_path(user_home).exists()
}

pub async fn fetch(
    user_home: &Path,
    subscriptions_url: &str,
    task_usage_url: &str,
    timeout: Duration,
) -> Result<UsageOutput> {
    let path = auth_path(user_home);
    let before = std::fs::read(&path)?;
    let document: Value = serde_json::from_slice(&before)?;
    let credentials = credential_candidates(&document)?;
    let http = client(timeout)?;
    let mut last_error = None;

    for creds in &credentials {
        match fetch_one(&http, creds, subscriptions_url, task_usage_url).await {
            Ok(output) if !output.metrics.is_empty() || output.plan.is_some() => {
                let after = std::fs::read(&path).unwrap_or_default();
                if after != before {
                    anyhow::bail!("Grok credentials were modified during fetch");
                }
                return Ok(output);
            }
            Ok(_) => {}
            Err(error) => last_error = Some(error),
        }
    }

    let after = std::fs::read(&path).unwrap_or_default();
    if after != before {
        anyhow::bail!("Grok credentials were modified during fetch");
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Grok usage unavailable")))
}

fn credential_candidates(document: &Value) -> Result<Vec<Credentials>> {
    let entries = document
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Grok auth.json must contain an object."))?;
    let mut candidates: Vec<_> = entries
        .iter()
        .filter_map(|(scope, value)| {
            let entry = value.as_object()?;
            let token = entry
                .get("key")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())?
                .to_string();
            let email = entry
                .get("email")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            let priority = if scope.contains("auth.x.ai") { 0 } else { 1 };
            Some((priority, Credentials { token, email }))
        })
        .collect();
    candidates.sort_by_key(|(priority, _)| *priority);
    let credentials: Vec<_> = candidates
        .into_iter()
        .map(|(_, credentials)| credentials)
        .collect();
    if credentials.is_empty() {
        anyhow::bail!("No Grok token found. Run 'grok login'.");
    }
    Ok(credentials)
}

async fn fetch_one(
    http: &reqwest::Client,
    creds: &Credentials,
    subscriptions_url: &str,
    task_usage_url: &str,
) -> Result<UsageOutput> {
    let mut plan = None;
    let mut metrics = Vec::new();

    if let Ok(value) = get_json(http, creds, task_usage_url, "Grok").await {
        collect_task_usage_metrics(&value, &mut metrics);
    }
    if let Ok(value) = get_json(http, creds, subscriptions_url, "Grok").await {
        plan = parse_subscription_plan(&value);
    }

    if metrics.is_empty() && plan.is_none() {
        anyhow::bail!("Grok usage unavailable: no usage or active subscription data returned");
    }

    Ok(UsageOutput {
        provider: "Grok Build".into(),
        account: None,
        credential_source: None,
        plan,
        email: creds.email.clone(),
        metrics,
    })
}

async fn get_json(
    http: &reqwest::Client,
    creds: &Credentials,
    url: &str,
    provider: &str,
) -> Result<Value> {
    let response = http
        .get(url)
        .header("Authorization", format!("Bearer {}", creds.token))
        .header("X-XAI-Token-Auth", "xai-grok-cli")
        .header("Accept", "application/json")
        .header("User-Agent", "Grok Build")
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(status_error(provider, status));
    }
    Ok(response.json().await?)
}

fn parse_subscription_plan(value: &Value) -> Option<String> {
    let subscriptions = value.get("subscriptions")?.as_array()?;
    let chosen = subscriptions.iter().find(|sub| {
        sub.get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status.eq_ignore_ascii_case("active"))
    })?;
    let tier = chosen.get("tier").and_then(Value::as_str)?;
    let trimmed = tier
        .trim_start_matches("SUBSCRIPTION_TIER_")
        .trim_start_matches("TIER_");
    Some(title_words(trimmed))
}

fn title_words(raw: &str) -> String {
    raw.replace(['_', '-'], " ")
        .split_whitespace()
        .map(|word| {
            let lower = word.to_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn numeric_value(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return number.is_finite().then_some(number);
    }
    if let Some(text) = value.as_str() {
        return text.parse::<f64>().ok().filter(|number| number.is_finite());
    }
    value
        .as_object()
        .and_then(|object| object.get("val").or_else(|| object.get("value")))
        .and_then(numeric_value)
}

fn push_limit_metric(
    metrics: &mut Vec<UsageMetric>,
    label: &str,
    used: Option<f64>,
    limit: Option<f64>,
    reset: Option<String>,
) {
    let Some(limit) = limit.filter(|limit| *limit > 0.0) else {
        return;
    };
    let used = used.unwrap_or(0.0).clamp(0.0, limit);
    let used_percent = (used / limit * 100.0).clamp(0.0, 100.0);
    let remaining_label = format!("{:.0}/{:.0} left", limit - used, limit);
    metrics.push(UsageMetric {
        label: label.into(),
        used_percent,
        remaining_percent: 100.0 - used_percent,
        remaining_label: Some(remaining_label),
        resets_at: reset,
    });
}

fn collect_task_usage_metrics(value: &Value, metrics: &mut Vec<UsageMetric>) {
    if let Value::Object(object) = value {
        let reset = object
            .get("resetTime")
            .or_else(|| object.get("resetsAt"))
            .or_else(|| object.get("resetAt"))
            .and_then(Value::as_str)
            .map(ToString::to_string);
        push_limit_metric(
            metrics,
            "Weekly",
            object.get("usage").and_then(numeric_value),
            object.get("limit").and_then(numeric_value),
            reset.clone(),
        );
        push_limit_metric(
            metrics,
            "Frequent",
            object.get("frequentUsage").and_then(numeric_value),
            object.get("frequentLimit").and_then(numeric_value),
            reset.clone(),
        );
        push_limit_metric(
            metrics,
            "Occasional",
            object.get("occasionalUsage").and_then(numeric_value),
            object.get("occasionalLimit").and_then(numeric_value),
            reset,
        );
        for child in object.values() {
            collect_task_usage_metrics(child, metrics);
        }
    } else if let Value::Array(items) = value {
        for child in items {
            collect_task_usage_metrics(child, metrics);
        }
    }
}
