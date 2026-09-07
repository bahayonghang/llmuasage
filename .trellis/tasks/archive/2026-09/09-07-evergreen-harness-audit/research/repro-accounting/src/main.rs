use std::{fs, path::Path};

use anyhow::{Context, Result, ensure};
use llmusage::{
    app::AppContext,
    commands,
    models::SourceKind,
    parsers::SyncEvent,
    store::Store,
};
use rusqlite::Connection;
use tokio_util::sync::CancellationToken;

const TABLES: &[&str] = &[
    "usage_event",
    "usage_bucket_30m",
    "usage_turn",
    "usage_tool_call",
    "source_cursor",
    "source_sync_status",
    "source_file",
];

#[tokio::main]
async fn main() -> Result<()> {
    let repro_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime");
    fs::create_dir_all(&repro_root)?;

    let parser_root = repro_root.join("parser-failure");
    let cancel_root = repro_root.join("cancellation");
    if parser_root.exists() || cancel_root.exists() {
        anyhow::bail!("runtime output already exists; use a clean repro directory");
    }

    reproduce_parser_failure(&parser_root).await?;
    reproduce_cancellation(&cancel_root).await?;
    Ok(())
}

async fn reproduce_parser_failure(root: &Path) -> Result<()> {
    let opencode_home = root.join("opencode-home");
    fs::create_dir_all(&opencode_home)?;
    unsafe { std::env::set_var("OPENCODE_HOME", &opencode_home) };
    seed_opencode(&opencode_home.join("opencode.db"))?;

    let app = AppContext::with_cli_home(Some(root.join("llmusage-home")))?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let options = commands::sync::SyncRunOptions {
        source: Some(SourceKind::Opencode),
        ..Default::default()
    };
    commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
    let before = counts(&store, SourceKind::Opencode)?;
    ensure!(before[0].1 > 0, "initial OpenCode sync produced no usage_event");

    store.clear_token_accounting_version(SourceKind::Opencode)?;
    Connection::open(opencode_home.join("opencode.db"))?.execute("DROP TABLE message", [])?;
    let error = commands::sync::run_once_with_options(&app, &store, 0, &options, None)
        .await
        .expect_err("broken source schema must fail automatic repair");
    let after = counts(&store, SourceKind::Opencode)?;

    println!("parser_failure.error={error:#}");
    print_counts("parser_failure.before", &before);
    print_counts("parser_failure.after", &after);
    println!(
        "parser_failure.marker_after={:?}",
        store.token_accounting_version(SourceKind::Opencode)?
    );
    Ok(())
}

async fn reproduce_cancellation(root: &Path) -> Result<()> {
    let codex_home = root.join("codex-home");
    unsafe { std::env::set_var("CODEX_HOME", &codex_home) };
    seed_codex(&codex_home)?;

    let app = AppContext::with_cli_home(Some(root.join("llmusage-home")))?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let options = commands::sync::SyncRunOptions {
        source: Some(SourceKind::Codex),
        ..Default::default()
    };
    commands::sync::run_once_with_options(&app, &store, 0, &options, None).await?;
    let before = counts(&store, SourceKind::Codex)?;
    ensure!(before[0].1 > 0, "initial Codex sync produced no usage_event");
    store.clear_token_accounting_version(SourceKind::Codex)?;

    let cancel = CancellationToken::new();
    let watcher_cancel = cancel.clone();
    let (mut tx, mut rx) = tokio::sync::mpsc::channel(1);
    tx.send(SyncEvent::BootstrapStarted).await?;
    let watcher = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if matches!(event, SyncEvent::TokenAccountingRepairStarted { .. }) {
                watcher_cancel.cancel();
            }
        }
    });
    commands::sync::run_once_with_cancel(&app, &store, 0, &options, Some(&mut tx), &cancel)
        .await?;
    drop(tx);
    watcher.await?;
    let after = counts(&store, SourceKind::Codex)?;

    print_counts("cancellation.before", &before);
    print_counts("cancellation.after", &after);
    println!(
        "cancellation.marker_after={:?}",
        store.token_accounting_version(SourceKind::Codex)?
    );
    Ok(())
}

fn seed_opencode(db_path: &Path) -> Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE project(id TEXT PRIMARY KEY, worktree TEXT);
        CREATE TABLE session(id TEXT PRIMARY KEY, project_id TEXT);
        CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
        INSERT INTO session(id, project_id) VALUES ('session-1', NULL);
        "#,
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
        "time": {"created": 1784077200000_i64, "completed": 1784077200000_i64}
    });
    conn.execute(
        "INSERT INTO message(id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
        ("msg-open", "session-1", 1784077200000_i64, message.to_string()),
    )?;
    Ok(())
}

fn seed_codex(codex_home: &Path) -> Result<()> {
    let dir = codex_home.join("sessions/2026/07/15");
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
    fs::write(dir.join("rollout-a.jsonl"), contents)?;
    Ok(())
}

fn counts(store: &Store, source: SourceKind) -> Result<Vec<(&'static str, i64)>> {
    let conn = store.open_connection()?;
    TABLES
        .iter()
        .map(|table| {
            let sql = format!("SELECT COUNT(*) FROM {table} WHERE source = ?1");
            let count = conn
                .query_row(&sql, [source.as_str()], |row| row.get(0))
                .with_context(|| format!("count {table}"))?;
            Ok((*table, count))
        })
        .collect()
}

fn print_counts(prefix: &str, counts: &[(&str, i64)]) {
    let body = counts
        .iter()
        .map(|(table, count)| format!("{table}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    println!("{prefix}={body}");
}
