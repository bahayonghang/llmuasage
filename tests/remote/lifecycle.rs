//! C4 remote lifecycle: unreachable hosts must not break local sync, missing
//! sweep, or lossy rebuild guards. No real SSH; shard streams are injected.

use std::{collections::BTreeMap, fs, path::PathBuf, process::Stdio};

use anyhow::{Context, Result};
use llmusage::{
    app::AppContext,
    commands::{self, source_status::host_lifecycle_status},
    models::{ParseIssues, SourceKind, UsageEvent, UsageTokens},
    parsers::{SourceSyncStats, SyncEvent},
    remote::{
        MemoryShardSource, SHARD_PROTOCOL_VERSION, ScriptedShardSource, ShardRecord, encode_record,
    },
    store::{FileCursor, Host, Store, SyncShard},
};
use rusqlite::Connection;
use tempfile::TempDir;
use tokio::sync::mpsc;

struct Fixture {
    _root: TempDir,
    home: PathBuf,
    env: crate::test_env::ScopedEnv,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = TempDir::new()?;
        let home = root.path().join("home");
        fs::create_dir_all(&home)?;
        let env = crate::test_env::ScopedEnv::capture(&[
            "HOME",
            "USERPROFILE",
            "CODEX_HOME",
            "CCR_ROOT",
            "OPENCODE_HOME",
            "OPENCODE_DB",
            "KIMI_CODE_HOME",
            "PI_AGENT_DIR",
            "GROK_HOME",
            "ZCODE_HOME",
            "GEMINI_CLI_HOME",
            "DSH_HOME",
        ]);
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("CODEX_HOME", home.join(".codex"));
            std::env::set_var("CCR_ROOT", home.join(".ccr"));
            std::env::set_var("OPENCODE_HOME", home.join(".opencode"));
            std::env::remove_var("OPENCODE_DB");
            std::env::remove_var("KIMI_CODE_HOME");
            std::env::remove_var("PI_AGENT_DIR");
            std::env::remove_var("GROK_HOME");
            std::env::remove_var("ZCODE_HOME");
            std::env::remove_var("GEMINI_CLI_HOME");
            std::env::remove_var("DSH_HOME");
        }
        fs::create_dir_all(home.join(".codex"))?;
        Ok(Self {
            _root: root,
            home,
            env,
        })
    }

    fn restore_env(&self) {
        self.env.restore();
    }

    fn app(&self) -> Result<AppContext> {
        AppContext::with_cli_home(Some(self.home.join(".llmusage")))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.restore_env();
    }
}

fn usage_event(key: &str, at: &str) -> UsageEvent {
    UsageEvent {
        event_key: key.to_string(),
        source: SourceKind::Codex,
        provider_label: String::new(),
        model: "gpt-5".to_string(),
        event_at: at.to_string(),
        hour_start: at.to_string(),
        tokens: UsageTokens {
            input_tokens: 1,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 1,
            reasoning_output_tokens: 0,
            total_tokens: 2,
        },
        project: None,
        session: None,
        source_cost: None,
    }
}

fn ssh_host(host_id: &str, label: &str) -> Host {
    Host {
        host_id: host_id.to_string(),
        label: label.to_string(),
        transport: "ssh".to_string(),
        ssh_target: Some(format!("me@{label}")),
        command: "llmusage".to_string(),
        added_at: "2026-08-20T00:00:00Z".to_string(),
        last_contacted_at: None,
        last_error: None,
        import_watermark: None,
    }
}

fn seed_remote_codex(store: &Store, host_id: &str, file_path: &str, key: &str) -> Result<()> {
    store.hosts().upsert(&ssh_host(host_id, host_id))?;
    let mut shard = SyncShard::new_for_host(SourceKind::Codex, host_id);
    shard.events.push(usage_event(key, "2026-08-20T00:00:00Z"));
    shard.seen_file_paths.push(file_path.to_string());
    shard.cursors.push(FileCursor {
        cursor_key: file_path.to_string(),
        file_path: file_path.to_string(),
        file_fingerprint: "fp".to_string(),
        file_size: 4,
        file_mtime_ns: 0,
        tail_signature: "tail".to_string(),
        offset: 4,
        last_total: None,
        last_model: None,
        updated_at: "2020-01-01T00:00:00Z".to_string(),
    });
    let mut writer = store.begin_sync_run()?;
    writer.commit_shard(shard)?;
    writer.finish_sync_run()?;
    let conn = Connection::open(&store.paths.db_path)?;
    conn.execute(
        "UPDATE source_file SET last_seen_at = '2020-01-01T00:00:00.000Z' WHERE host_id = ?1",
        [host_id],
    )?;
    Ok(())
}

fn shard_stream(host_id: &str, key: &str) -> Result<String> {
    let mut shard = SyncShard::new(SourceKind::Codex);
    shard.events.push(usage_event(key, "2026-08-20T01:00:00Z"));
    shard
        .seen_file_paths
        .push(format!("/{host_id}/codex.jsonl"));
    let mut stdout = String::new();
    for record in [
        ShardRecord::Header {
            shard_protocol: SHARD_PROTOCOL_VERSION,
            llmusage_version: "1.2.0".to_string(),
            schema_version: 23,
            emitted_at: "2026-08-20T02:00:00Z".to_string(),
        },
        ShardRecord::Shard { shard },
        ShardRecord::Trailer {
            sources: vec![SourceSyncStats {
                source: SourceKind::Codex,
                events_seen: 1,
                events_inserted: 1,
                ..SourceSyncStats::default()
            }],
            parse_issues: ParseIssues::default(),
        },
    ] {
        stdout.push_str(&encode_record(&record)?);
        stdout.push('\n');
    }
    Ok(stdout)
}

fn unreachable_source() -> ScriptedShardSource {
    ScriptedShardSource {
        hosts: BTreeMap::new(),
        default_error: "ssh timed out".to_string(),
    }
}

#[test]
fn unreachable_remote_does_not_fail_sync_or_sweep_source_files() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = fixture.app()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        seed_remote_codex(&store, "devbox", "/missing/remote-codex.jsonl", "codex:r:1")?;
        let before = store.source_files().counts(SourceKind::Codex, "devbox")?;
        assert_eq!(before.live, 1);
        assert_eq!(before.missing, 0);

        let (mut tx, mut rx) = mpsc::channel(32);
        let result = commands::sync::run_store_once_with_remote_source(
            &store,
            &commands::sync::SyncRunOptions::default(),
            &unreachable_source(),
            Some(&mut tx),
        )
        .await;
        drop(tx);
        assert!(result.is_ok(), "{result:?}");

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        assert!(
            events.iter().any(|event| matches!(
                event,
                SyncEvent::RemoteHostSkipped { label, reason, .. }
                    if label == "devbox" && reason.contains("ssh timed out")
            )),
            "{events:?}"
        );
        let after = store.source_files().counts(SourceKind::Codex, "devbox")?;
        assert_eq!(
            after.live, 1,
            "unreachable host must not be swept to missing"
        );
        assert_eq!(after.missing, 0);
        let host = store.hosts().get_by_label("devbox")?.expect("devbox");
        assert!(
            host.last_error
                .as_deref()
                .is_some_and(|error| error.contains("ssh timed out")),
            "{:?}",
            host.last_error
        );
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

#[test]
fn unreachable_remote_missing_files_do_not_block_rebuild() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = fixture.app()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        seed_remote_codex(&store, "devbox", "/missing/remote-codex.jsonl", "codex:r:1")?;

        let result = commands::sync::run_store_once_with_remote_source(
            &store,
            &commands::sync::SyncRunOptions {
                rebuild: true,
                ..Default::default()
            },
            &unreachable_source(),
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "rebuild must ignore uncontacted remote missing files: {result:?}"
        );
        let conn = store.open_connection()?;
        let remote_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE host_id = 'devbox'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(remote_events, 1, "rebuild resets local only");
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

#[test]
fn unreachable_remote_missing_files_do_not_block_automatic_repair() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = fixture.app()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        seed_remote_codex(&store, "devbox", "/missing/remote-codex.jsonl", "codex:r:1")?;
        store.clear_token_accounting_version(SourceKind::Codex)?;
        assert!(store.has_legacy_token_accounting(SourceKind::Codex)?);

        let result = commands::sync::run_store_once_with_remote_source(
            &store,
            &commands::sync::SyncRunOptions::default(),
            &unreachable_source(),
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "automatic repair must ignore uncontacted remote missing files: {result:?}"
        );
        let conn = store.open_connection()?;
        let remote_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE host_id = 'devbox'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(remote_events, 1);
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

#[test]
fn json_events_emit_remote_host_started_finished_and_skipped() -> Result<()> {
    let fixture = Fixture::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let app = fixture.app()?;
        let store = Store::new(&app.paths)?;
        store.bootstrap()?;
        store.hosts().upsert(&ssh_host("livebox", "livebox"))?;
        store.hosts().upsert(&ssh_host("deadbox", "deadbox"))?;

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "livebox".to_string(),
            Ok(MemoryShardSource {
                stdout: shard_stream("livebox", "codex:live:1")?,
                stderr: String::new(),
                status: 0,
            }),
        );
        hosts.insert("deadbox".to_string(), Err("connection refused".to_string()));
        let source = ScriptedShardSource {
            hosts,
            default_error: "unreachable".to_string(),
        };

        let (mut tx, mut rx) = mpsc::channel(64);
        commands::sync::run_store_once_with_remote_source(
            &store,
            &commands::sync::SyncRunOptions::default(),
            &source,
            Some(&mut tx),
        )
        .await?;
        drop(tx);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        assert!(
            events.iter().any(|event| matches!(
                event,
                SyncEvent::RemoteHostStarted { label, .. } if label == "livebox"
            )),
            "{events:?}"
        );
        assert!(
            events.iter().any(|event| matches!(
                event,
                SyncEvent::RemoteHostFinished { label, .. } if label == "livebox"
            )),
            "{events:?}"
        );
        assert!(
            events.iter().any(|event| matches!(
                event,
                SyncEvent::RemoteHostSkipped { label, reason, .. }
                    if label == "deadbox" && reason.contains("connection refused")
            )),
            "{events:?}"
        );
        let live = store.hosts().get_by_label("livebox")?.expect("livebox");
        assert!(live.last_error.is_none());
        assert!(live.last_contacted_at.is_some());
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

#[test]
fn source_status_reports_three_host_states_and_omits_live() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = fixture.app()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    store.hosts().upsert(&ssh_host("fresh", "fresh"))?;
    store.hosts().upsert(&Host {
        last_contacted_at: Some("2026-08-20T01:00:00Z".to_string()),
        last_error: Some("ssh timed out".to_string()),
        ..ssh_host("down", "down")
    })?;
    store.hosts().upsert(&Host {
        last_contacted_at: Some("2026-08-20T01:00:00Z".to_string()),
        last_error: None,
        ..ssh_host("ok", "ok")
    })?;

    let local = store.hosts().get_by_label("local")?.expect("local");
    assert_eq!(host_lifecycle_status(&local), "never_contacted");
    assert_eq!(
        host_lifecycle_status(&store.hosts().get_by_label("fresh")?.expect("fresh")),
        "never_contacted"
    );
    assert_eq!(
        host_lifecycle_status(&store.hosts().get_by_label("down")?.expect("down")),
        "unreachable"
    );
    assert_eq!(
        host_lifecycle_status(&store.hosts().get_by_label("ok")?.expect("ok")),
        "idle"
    );

    let output = crate::test_process::llmusage_command()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "--home",
            app.paths.root_dir.to_str().expect("utf8 home"),
            "source-status",
        ])
        .env("RUST_LOG", "off")
        .env("LLMUSAGE_LOG", "off")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("spawn llmusage sync for remote lifecycle events")?;
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("status=never_contacted"), "{stdout}");
    assert!(stdout.contains("status=unreachable"), "{stdout}");
    assert!(stdout.contains("status=idle"), "{stdout}");
    assert!(
        !stdout.contains("status=live"),
        "source-status must not print live: {stdout}"
    );
    Ok(())
}
