use std::process::Stdio;

use anyhow::{Context, Result};
use llmusage::{
    app::AppContext,
    commands::sync::{EmitShardOptions, emit_shards_to},
    models::{SourceKind, UsageEvent, UsageTokens},
    store::{FileCursor, HolderKind, Store, SyncShard, read_schema_version},
};
use rusqlite::Connection;
use tempfile::TempDir;

#[tokio::test]
async fn emit_shards_cli_leaves_user_db_counts_and_lock_unchanged() -> Result<()> {
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home)?;
    let app = AppContext::with_cli_home(Some(home.clone()))?;
    let store = Store::new(&app.paths)?;
    let lock =
        store.acquire_worker_lock_with(std::time::Duration::from_secs(5), HolderKind::Cli)?;
    let fenced = lock.fenced_store();
    fenced.bootstrap()?;
    let mut writer = fenced.begin_sync_run()?;
    let mut shard = SyncShard::new(SourceKind::Codex);
    shard.events.push(UsageEvent {
        event_key: "codex:path:seed".to_string(),
        source: SourceKind::Codex,
        provider_label: String::new(),
        model: "gpt-5".to_string(),
        event_at: "2026-08-20T00:00:00Z".to_string(),
        hour_start: "2026-08-20T00:00:00Z".to_string(),
        tokens: UsageTokens {
            input_tokens: 3,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 1,
            reasoning_output_tokens: 0,
            total_tokens: 4,
        },
        project: None,
        session: None,
        source_cost: None,
    });
    shard.seen_file_paths.push("/tmp/seed.jsonl".to_string());
    shard.cursors.push(FileCursor {
        cursor_key: "/tmp/seed.jsonl".to_string(),
        file_path: "/tmp/seed.jsonl".to_string(),
        file_fingerprint: "fp".to_string(),
        file_size: 4,
        file_mtime_ns: 0,
        tail_signature: "tail".to_string(),
        offset: 4,
        last_total: None,
        last_model: None,
        updated_at: "2026-08-20T00:00:00Z".to_string(),
    });
    writer.commit_shard(shard)?;
    writer.finish_sync_run()?;
    drop(lock);

    let conn = Connection::open(&app.paths.db_path)?;
    let schema_before = read_schema_version(&conn)?;
    let events_before: i64 =
        conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
    let files_before: i64 =
        conn.query_row("SELECT COUNT(*) FROM source_file", [], |row| row.get(0))?;
    let cursors_before: i64 =
        conn.query_row("SELECT COUNT(*) FROM source_cursor", [], |row| row.get(0))?;
    let lock_before = conn
        .query_row("SELECT COUNT(*) FROM worker_lock", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0);
    drop(conn);

    let zcode_home = temp.path().join("zcode-empty");
    std::fs::create_dir_all(&zcode_home)?;
    let output = crate::test_process::llmusage_command()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "--home",
            home.to_str().expect("utf8 home"),
            "sync",
            "--emit-shards",
            "--source",
            "zcode",
        ])
        .env("RUST_LOG", "off")
        .env("ZCODE_HOME", &zcode_home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("spawn llmusage emit-shards subprocess")?;
    assert!(
        output.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("\"kind\":\"header\""), "{stdout}");
    assert!(stdout.contains("\"kind\":\"trailer\""), "{stdout}");
    assert!(!stdout.contains("raw_records"), "{stdout}");

    emit_shards_to(
        &app,
        EmitShardOptions {
            source: Some("zcode".to_string()),
            ..EmitShardOptions::default()
        },
        std::io::sink(),
    )
    .await?;

    let conn = Connection::open(&app.paths.db_path)?;
    let schema_after = read_schema_version(&conn)?;
    let events_after: i64 =
        conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
    let files_after: i64 =
        conn.query_row("SELECT COUNT(*) FROM source_file", [], |row| row.get(0))?;
    let cursors_after: i64 =
        conn.query_row("SELECT COUNT(*) FROM source_cursor", [], |row| row.get(0))?;
    let lock_after = conn
        .query_row("SELECT COUNT(*) FROM worker_lock", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0);
    assert_eq!(schema_before, schema_after);
    assert_eq!(events_before, events_after);
    assert_eq!(files_before, files_after);
    assert_eq!(cursors_before, cursors_after);
    assert_eq!(lock_before, lock_after);
    assert!(events_before > 0);
    Ok(())
}
