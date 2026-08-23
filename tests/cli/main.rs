use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{Duration, Utc};
use rusqlite::{Connection, params};
use tempfile::TempDir;

use llmusage::{
    logging::{read_recent_log_entries, runtime_status},
    paths::AppPaths,
    query::{Dashboard, QueryFilter, ReportTimezone},
    store::Store,
};

#[path = "../support/env.rs"]
mod test_env;
#[path = "../support/process.rs"]
mod test_process;

fn assert_json_has_no_camel_case_keys(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                assert!(
                    !has_camel_case_boundary(key),
                    "JSON key should be snake_case, got {key}"
                );
                assert_json_has_no_camel_case_keys(value);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                assert_json_has_no_camel_case_keys(item);
            }
        }
        _ => {}
    }
}

fn assert_json_has_no_cost_keys(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                assert!(
                    !key.to_ascii_lowercase().contains("cost"),
                    "JSON key should not expose cost, got {key}"
                );
                assert_json_has_no_cost_keys(value);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                assert_json_has_no_cost_keys(item);
            }
        }
        _ => {}
    }
}

fn assert_json_has_no_agent_keys(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                assert!(
                    key != "agent" && key != "agents",
                    "focused JSON should not expose comparison field {key}"
                );
                assert_json_has_no_agent_keys(value);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                assert_json_has_no_agent_keys(item);
            }
        }
        _ => {}
    }
}

fn daily_dates(value: &serde_json::Value) -> Vec<String> {
    value["daily"]
        .as_array()
        .expect("daily array")
        .iter()
        .map(|row| {
            row["period"]
                .as_str()
                .expect("daily row period")
                .to_string()
        })
        .collect()
}

fn has_camel_case_boundary(value: &str) -> bool {
    let mut prev_lower = false;
    for ch in value.chars() {
        if prev_lower && ch.is_ascii_uppercase() {
            return true;
        }
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    false
}

struct ReportCliFixture {
    _temp: TempDir,
    home: PathBuf,
    paths: AppPaths,
}

impl ReportCliFixture {
    fn new() -> Result<Self> {
        let temp = TempDir::new()?;
        let home = temp.path().join("home");
        let root_dir = home.join(".llmusage");
        std::fs::create_dir_all(&home)?;
        let paths = AppPaths::with_root(root_dir)?;
        Store::new(&paths)?.bootstrap()?;
        Ok(Self {
            _temp: temp,
            home,
            paths,
        })
    }

    fn seed_event(&self, event: SeedEvent<'_>) -> Result<()> {
        let conn = Connection::open(&self.paths.db_path)?;
        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, host_id, source, provider_label, model, event_at, hour_start,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                project_hash, project_label, project_ref, path_hash,
                session_id, session_label, source_path_hash, created_at
            ) VALUES (
                ?1, ?22, ?2, ?3, ?4, ?5, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19,
                ?20, ?20, ?21, ?5
            )
            "#,
            params![
                event.event_key,
                event.source,
                event.provider_label,
                event.model,
                event.event_at,
                event.input_tokens,
                event.cache_read_tokens,
                event.cache_creation_tokens,
                event.output_tokens,
                event.reasoning_output_tokens,
                event.total_tokens,
                event.cost_with_cache_usd,
                event.cost_without_cache_usd,
                event.pricing_status,
                event.pricing_source,
                event.project_hash,
                event.project_label,
                event.project_ref,
                event.source_path_hash.unwrap_or(event.event_key),
                event.session_id,
                event.source_path_hash,
                event.host_id,
            ],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                host_id, source, provider_label, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                event_count, updated_at
            ) VALUES (?18, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 1, ?4)
            ON CONFLICT(host_id, source, provider_label, model, hour_start, project_hash) DO UPDATE SET
                input_tokens = input_tokens + excluded.input_tokens,
                cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
                cache_creation_tokens = cache_creation_tokens + excluded.cache_creation_tokens,
                output_tokens = output_tokens + excluded.output_tokens,
                reasoning_output_tokens = reasoning_output_tokens + excluded.reasoning_output_tokens,
                total_tokens = total_tokens + excluded.total_tokens,
                cost_with_cache_usd = cost_with_cache_usd + excluded.cost_with_cache_usd,
                cost_without_cache_usd = cost_without_cache_usd + excluded.cost_without_cache_usd,
                pricing_status = CASE
                    WHEN pricing_status = excluded.pricing_status THEN pricing_status
                    ELSE 'mixed'
                END,
                pricing_source = CASE
                    WHEN pricing_source IS excluded.pricing_source THEN pricing_source
                    ELSE 'mixed'
                END,
                event_count = event_count + excluded.event_count,
                updated_at = excluded.updated_at
            "#,
            params![
                event.source,
                event.provider_label,
                event.model,
                event.event_at,
                event.project_hash,
                event.project_label,
                event.project_ref,
                event.input_tokens,
                event.cache_read_tokens,
                event.cache_creation_tokens,
                event.output_tokens,
                event.reasoning_output_tokens,
                event.total_tokens,
                event.cost_with_cache_usd,
                event.cost_without_cache_usd,
                event.pricing_status,
                event.pricing_source,
                event.host_id,
            ],
        )?;
        Ok(())
    }

    fn upsert_host(&self, host_id: &str, label: &str) -> Result<()> {
        Store::new(&self.paths)?
            .hosts()
            .upsert(&llmusage::store::Host {
                host_id: host_id.to_string(),
                label: label.to_string(),
                transport: "ssh".to_string(),
                ssh_target: Some(format!("{label}@example")),
                command: "llmusage".to_string(),
                added_at: "2026-08-20T00:00:00Z".to_string(),
                last_contacted_at: None,
                last_error: None,
                import_watermark: None,
            })?;
        Ok(())
    }

    fn json(&self, args: &[&str]) -> Result<serde_json::Value> {
        self.json_with_env(args, &[])
    }

    fn json_with_env(&self, args: &[&str], envs: &[(&str, &str)]) -> Result<serde_json::Value> {
        let output = self.output_with_env(args, envs)?;
        if !output.status.success() {
            bail!("command failed: {output:?}");
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    fn output(&self, args: &[&str]) -> Result<std::process::Output> {
        self.output_with_env(args, &[])
    }

    fn output_with_env(
        &self,
        args: &[&str],
        envs: &[(&str, &str)],
    ) -> Result<std::process::Output> {
        let mut command = test_process::llmusage_command();
        command
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(args)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("CODEX_HOME", self.home.join(".codex"))
            .env("OPENCODE_HOME", self.home.join("opencode"));
        for (key, value) in envs {
            command.env(key, value);
        }
        command.output().context("spawn llmusage CLI subprocess")
    }
}

struct SeedEvent<'a> {
    event_key: &'a str,
    host_id: &'a str,
    source: &'a str,
    provider_label: &'a str,
    model: &'a str,
    event_at: &'a str,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
    project_hash: &'a str,
    project_label: &'a str,
    project_ref: Option<&'a str>,
    session_id: Option<&'a str>,
    source_path_hash: Option<&'a str>,
    cost_with_cache_usd: f64,
    cost_without_cache_usd: f64,
    pricing_status: &'a str,
    pricing_source: Option<&'a str>,
}

impl Default for SeedEvent<'_> {
    fn default() -> Self {
        Self {
            event_key: "codex:test:1",
            host_id: "local",
            source: "codex",
            provider_label: "",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            input_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 0,
            project_hash: "project-a",
            project_label: "Project A",
            project_ref: None,
            session_id: None,
            source_path_hash: None,
            cost_with_cache_usd: 0.0,
            cost_without_cache_usd: 0.0,
            pricing_status: "unpriced",
            pricing_source: None,
        }
    }
}

mod local_flow;
mod operations;
mod reports;
