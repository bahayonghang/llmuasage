use std::{fs, path::PathBuf};

use anyhow::Result;
use llmusage::{
    app::AppContext,
    commands,
    models::SourceKind,
    query::{
        Dashboard, ReportTimezone,
        reports::{ReportFilter, SortOrder, load_daily_report},
    },
    store::Store,
};
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn ccusage_token_semantics_are_consistent_across_sources_and_queries() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;
    fixture.seed_claude_streaming_and_sidechain_replay()?;
    fixture.seed_opencode_authoritative_total()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;

        for source in [SourceKind::Codex, SourceKind::Claude, SourceKind::Opencode] {
            commands::sync::run_once_with_options(
                &app,
                &store,
                0,
                &commands::sync::SyncRunOptions {
                    source: Some(source),
                    ..Default::default()
                },
                None,
            )
            .await?;
        }

        let conn = Connection::open(&app.paths.db_path)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT source, input_tokens, cache_creation_tokens, cache_read_tokens,
                   output_tokens, reasoning_output_tokens, total_tokens
            FROM usage_event
            ORDER BY source
            "#,
        )?;
        let events = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert_eq!(
            events,
            vec![
                ("claude".to_string(), 20, 0, 5, 10, 0, 35),
                ("codex".to_string(), 60, 0, 40, 30, 10, 130),
                ("opencode".to_string(), 100, 40, 20, 30, 7, 250),
            ]
        );

        let event_total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM usage_event",
            [],
            |row| row.get(0),
        )?;
        let bucket_total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM usage_bucket_30m",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(event_total, 415);
        assert_eq!(bucket_total, event_total);

        let overview = Dashboard::open(&store)?.overview(&Default::default())?;
        assert_eq!(overview.total.total_tokens, event_total);
        let daily = load_daily_report(
            &store,
            &ReportFilter {
                since: None,
                until: None,
                order: SortOrder::Asc,
                timezone: ReportTimezone::Utc,
                locale: "en-US".to_string(),
                source: None,
                project: None,
                breakdown: true,
            },
        )?;
        assert_eq!(daily.totals.total_tokens, event_total);

        let (cost, rate_json): (f64, String) = conn.query_row(
            "SELECT cost_with_cache_usd, pricing_rate FROM usage_event WHERE source = 'codex'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let rate: serde_json::Value = serde_json::from_str(&rate_json)?;
        let expected_cost = (60.0 * rate["input_per_mtok"].as_f64().unwrap()
            + 40.0 * rate["cached_per_mtok"].as_f64().unwrap()
            + 30.0 * rate["output_per_mtok"].as_f64().unwrap())
            / 1_000_000.0;
        assert!((cost - expected_cost).abs() <= 1e-9);

        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

#[test]
fn legacy_source_requires_guarded_explicit_rebuild_before_new_writes() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.seed_codex_copied_event()?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = AppContext::discover()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        let source_options = commands::sync::SyncRunOptions {
            source: Some(SourceKind::Codex),
            ..Default::default()
        };
        commands::sync::run_once_with_options(&app, &store, 0, &source_options, None).await?;
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(2));

        store.clear_token_accounting_version(SourceKind::Codex)?;
        assert!(store.has_legacy_token_accounting(SourceKind::Codex)?);
        let status = store
            .sync_status()
            .load_source_sync_statuses()?
            .into_iter()
            .find(|status| status.source == "codex")
            .expect("codex sync status");
        assert!(status.legacy_token_accounting);
        assert!(
            status
                .token_accounting_warning
                .as_deref()
                .is_some_and(|warning| warning.contains("sync --rebuild --source codex"))
        );
        let before: i64 = Connection::open(&app.paths.db_path)?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        let error = commands::sync::run_once_with_options(&app, &store, 0, &source_options, None)
            .await
            .expect_err("legacy source must be read-only");
        assert!(error.to_string().contains("sync --rebuild --source"));
        let after: i64 = Connection::open(&app.paths.db_path)?.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(after, before);

        commands::sync::run_once_with_options(
            &app,
            &store,
            0,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            None,
        )
        .await?;
        assert_eq!(store.token_accounting_version(SourceKind::Codex)?, Some(2));
        assert!(!store.has_legacy_token_accounting(SourceKind::Codex)?);

        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}

struct Fixture {
    _root: TempDir,
    home: PathBuf,
    codex_home: PathBuf,
    opencode_home: PathBuf,
    saved_env: Vec<(String, Option<String>)>,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = TempDir::new()?;
        let home = root.path().join("home");
        let codex_home = home.join(".codex");
        let opencode_home = root.path().join("opencode-home");
        fs::create_dir_all(home.join(".claude/projects/demo"))?;
        fs::create_dir_all(&codex_home)?;
        fs::create_dir_all(&opencode_home)?;

        let mut saved_env = Vec::new();
        for key in ["HOME", "USERPROFILE", "CODEX_HOME", "OPENCODE_HOME"] {
            saved_env.push((key.to_string(), std::env::var(key).ok()));
        }
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("CODEX_HOME", &codex_home);
            std::env::set_var("OPENCODE_HOME", &opencode_home);
        }

        Ok(Self {
            _root: root,
            home,
            codex_home,
            opencode_home,
            saved_env,
        })
    }

    fn seed_codex_copied_event(&self) -> Result<()> {
        let dir = self.codex_home.join("sessions/2026/07/15");
        fs::create_dir_all(&dir)?;
        let usage = serde_json::json!({
            "input_tokens": 100,
            "cached_input_tokens": 40,
            "output_tokens": 30,
            "reasoning_output_tokens": 10,
            "total_tokens": 130
        });
        let contents = [
            serde_json::json!({
                "type": "session_meta",
                "payload": {"id": "session-a", "model": "gpt-5"}
            })
            .to_string(),
            serde_json::json!({
                "timestamp": "2026-07-15T01:00:00Z",
                "payload": {
                    "type": "token_count",
                    "info": {"last_token_usage": usage, "total_token_usage": usage}
                }
            })
            .to_string(),
        ]
        .join("\n");
        fs::write(dir.join("rollout-a.jsonl"), &contents)?;
        fs::write(dir.join("rollout-copy.jsonl"), contents)?;
        Ok(())
    }

    fn seed_claude_streaming_and_sidechain_replay(&self) -> Result<()> {
        let dir = self.home.join(".claude/projects/demo");
        let partial = claude_line("req-a", false, 10, 2, 4);
        let complete = claude_line("req-a", false, 20, 5, 10);
        fs::write(
            dir.join("session.jsonl"),
            format!("{partial}\n{complete}\n"),
        )?;
        fs::write(
            dir.join("sidechain.jsonl"),
            format!("{}\n", claude_line("req-side", true, 20, 5, 10)),
        )?;
        Ok(())
    }

    fn seed_opencode_authoritative_total(&self) -> Result<()> {
        let conn = Connection::open(self.opencode_home.join("opencode.db"))?;
        conn.execute_batch(
            r#"
            CREATE TABLE project(id TEXT PRIMARY KEY, worktree TEXT);
            CREATE TABLE session(id TEXT PRIMARY KEY, project_id TEXT);
            CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
            "#,
        )?;
        conn.execute(
            "INSERT INTO session(id, project_id) VALUES ('session-1', NULL)",
            [],
        )?;
        let message = serde_json::json!({
            "id": "msg-open",
            "role": "assistant",
            "modelID": "gpt-5",
            "tokens": {
                "input": 100,
                "output": 30,
                "reasoning": 7,
                "total": 250,
                "cache": {"read": 20, "write": 40}
            },
            "time": {"created": 1784077200000i64, "completed": 1784077200000i64}
        });
        conn.execute(
            "INSERT INTO message(id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                "msg-open",
                "session-1",
                1784077200000i64,
                message.to_string(),
            ),
        )?;
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for (key, value) in &self.saved_env {
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
}

fn claude_line(
    request_id: &str,
    is_sidechain: bool,
    input: i64,
    cache_read: i64,
    output: i64,
) -> String {
    serde_json::json!({
        "timestamp": "2026-07-15T02:00:00Z",
        "sessionId": "session-claude",
        "requestId": request_id,
        "isSidechain": is_sidechain,
        "message": {
            "id": "msg-claude",
            "model": "claude-sonnet-4",
            "usage": {
                "input_tokens": input,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": cache_read,
                "output_tokens": output
            }
        }
    })
    .to_string()
}
