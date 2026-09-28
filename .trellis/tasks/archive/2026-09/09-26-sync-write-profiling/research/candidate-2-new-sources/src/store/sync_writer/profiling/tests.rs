use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Result;
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::{AuditClock, Capture, Record, Variant, VariantGuard};
use crate::{
    models::{
        ActivityCategory, ProjectInfo, SessionInfo, SourceKind, ToolKind, UsageEvent, UsageTokens,
        UsageToolCall, UsageTurn,
    },
    paths::AppPaths,
    store::{FileCursor, HolderKind, Store, SyncShard},
};

const SEED: u64 = 0x20260928;
const AUDIT: &str = "2026-09-01T12:00:00.000Z";

struct Fixture {
    _root: TempDir,
    seed_path: PathBuf,
    seed_sha256: String,
    input: Vec<SyncShard>,
    manifest: serde_json::Value,
}

struct Sample {
    _dir: TempDir,
    db_path: PathBuf,
    total_ns: u128,
    begin_ns: u128,
    finish_ns: u128,
    write_ns: u128,
    records: Vec<Record>,
}

pub(crate) fn temp_root() -> Result<TempDir> {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/writer-benchmark");
    fs::create_dir_all(&parent)?;
    Ok(tempfile::Builder::new()
        .prefix("generated-")
        .tempdir_in(parent)?)
}

fn event(source: SourceKind, project: usize, file: usize, index: usize) -> UsageEvent {
    let n = SEED.wrapping_add((project * 10_000 + file * 1_000 + index) as u64);
    let path_hash = format!("fixture-{project:03}-{file:02}");
    let input = 100 + (n % 97) as i64;
    let cache_read = 20 + (n % 13) as i64;
    let cache_creation = if source == SourceKind::Codex {
        0
    } else {
        10 + (n % 7) as i64
    };
    let output = 40 + (n % 31) as i64;
    UsageEvent {
        event_key: format!("{}:{path_hash}:{index:05}", source.as_str()),
        source,
        provider_label: String::new(),
        model: if source == SourceKind::Claude {
            "claude-sonnet-4-5"
        } else {
            "gpt-5"
        }
        .into(),
        event_at: format!("2026-08-01T{:02}:10:00Z", index % 4),
        hour_start: format!("2026-08-01T{:02}:00:00Z", index % 4),
        tokens: UsageTokens {
            input_tokens: input,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            output_tokens: output,
            reasoning_output_tokens: (n % 11) as i64,
            total_tokens: input
                + cache_read
                + cache_creation
                + output
                + if matches!(source, SourceKind::Antigravity | SourceKind::AntigravityIde) {
                    (n % 11) as i64
                } else {
                    0
                },
        },
        project: Some(ProjectInfo {
            project_hash: format!("project-{project:03}"),
            project_label: format!("Generated project {project}"),
            project_ref: Some(format!("synthetic/project-{project}")),
            repo_root_hash: format!("repo-{project}"),
            path_hash: format!("project-path-{project}"),
        }),
        session: Some(SessionInfo {
            session_id: format!("session:{path_hash}"),
            session_label: Some(format!("Generated session {file}")),
            source_path_hash: Some(path_hash),
        }),
        source_cost: None,
    }
}

fn add_event(shard: &mut SyncShard, event: UsageEvent, behavior: bool) {
    if behavior {
        let turn = UsageTurn::from_event(&event, ActivityCategory::Coding);
        for (name, kind) in [
            ("Read", ToolKind::Read),
            ("Edit", ToolKind::Edit),
            ("Shell", ToolKind::Bash),
        ] {
            shard.tool_calls.push(UsageToolCall {
                tool_call_key: format!("tool:{}:{name}", event.event_key),
                turn_key: Some(turn.turn_key.clone()),
                event_key: Some(event.event_key.clone()),
                source: event.source,
                session_id: event.session.as_ref().map(|s| s.session_id.clone()),
                source_path_hash: event
                    .session
                    .as_ref()
                    .and_then(|s| s.source_path_hash.clone()),
                project_hash: event.project.as_ref().map(|p| p.project_hash.clone()),
                model: Some(event.model.clone()),
                occurred_at: event.event_at.clone(),
                tool_name: name.into(),
                tool_kind: kind,
                mcp_server: None,
                mcp_tool: None,
                input_fingerprint: Some(format!("generated-{name}")),
                safe_preview: Some("Generated fixture".into()),
            });
        }
        shard.turns.push(turn);
    }
    shard.events.push(event);
}

fn add_path(
    shard: &mut SyncShard,
    root: &Path,
    project: usize,
    file: usize,
    count: usize,
    reset: bool,
) {
    let path_hash = format!("fixture-{project:03}-{file:02}");
    let file_path = root
        .join("sources")
        .join(format!("project-{project}/{file}.jsonl"))
        .to_string_lossy()
        .into_owned();
    if reset {
        shard.reset_path_hashes.push(path_hash.clone());
    }
    shard.cursors.push(FileCursor {
        cursor_key: format!("cursor:{path_hash}"),
        file_path: file_path.clone(),
        file_fingerprint: format!("generated-{count}"),
        file_size: count as u64 * 128,
        file_mtime_ns: 1_700_000_000,
        tail_signature: format!("tail-{count}"),
        offset: count as u64 * 128,
        last_total: None,
        last_model: Some(
            if shard.source == SourceKind::Claude {
                "claude-sonnet-4-5"
            } else {
                "gpt-5"
            }
            .into(),
        ),
        updated_at: AUDIT.into(),
    });
    shard.seen_file_paths.push(file_path);
}

pub(crate) fn checkpoint(path: &Path) -> Result<()> {
    let conn = Connection::open(path)?;
    let (busy, _, _): (i64, i64, i64) =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    assert_eq!(busy, 0);
    conn.close().map_err(|(_, e)| e)?;
    Ok(())
}

pub(crate) fn digest_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut block = [0u8; 65_536];
    loop {
        let read = file.read(&mut block)?;
        if read == 0 {
            break;
        }
        hash.update(&block[..read]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn replay_fixture(projects: usize) -> Result<Fixture> {
    sized_fixture(projects, 10, 500, 4, 25, SourceKind::Claude, true)
}

fn sized_fixture(
    projects: usize,
    files: usize,
    per_file: usize,
    changed_projects: usize,
    extra: usize,
    source: SourceKind,
    behavior: bool,
) -> Result<Fixture> {
    let root = temp_root()?;
    let paths = AppPaths::with_root(root.path().join("seed"))?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    let _clock = AuditClock::fixed();
    let _baseline = VariantGuard::set(Variant::Baseline);
    {
        let lock = store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Library)?;
        let heartbeat = lock.start_default_heartbeat();
        let fenced = lock.fenced_store();
        let mut writer = fenced.begin_sync_run()?;
        for project in 0..projects {
            let mut shard = SyncShard::new(source);
            for file in 0..files {
                add_path(&mut shard, root.path(), project, file, per_file, false);
                for index in 0..per_file {
                    add_event(&mut shard, event(source, project, file, index), behavior);
                }
            }
            writer.commit_shard(shard)?;
        }
        writer.finish_sync_run()?;
        fenced.write_transaction(|tx| {
            tx.execute("INSERT INTO run_log(command,status,summary,started_at,finished_at,duration_ms) VALUES ('generated-sentinel','success','retain',?1,?1,1)", [AUDIT])?;
            tx.execute("INSERT INTO source_sync_status(host_id,source,files_processed,changed_files,bytes_scanned,events_seen,events_replayed,events_inserted,stored_events,parse_ms,write_ms,lock_wait_ms,updated_at) VALUES ('local','grok',1,1,128,1,0,1,1,1,1,0,?1)", [AUDIT])?;
            tx.execute("INSERT OR REPLACE INTO meta(key,value) VALUES ('generated_untouched_marker','retain')", [])?;
            Ok(())
        })?;
        drop(heartbeat);
    }
    checkpoint(&paths.db_path)?;
    let mut replay = SyncShard::new(source);
    for project in 0..changed_projects {
        for file in 0..files {
            add_path(
                &mut replay,
                root.path(),
                project,
                file,
                per_file + extra,
                true,
            );
            for index in 0..per_file + extra {
                add_event(&mut replay, event(source, project, file, index), behavior);
            }
        }
    }
    let initial = projects * files * per_file;
    let input_count = changed_projects * files * (per_file + extra);
    let manifest = json!({"seed": SEED, "workload": if projects == 10 { "claude_replay_primary" } else { "claude_replay_history" }, "source": source.as_str(), "initial_events": initial, "initial_turns": if behavior {initial} else {0}, "initial_tools": if behavior {initial * 3} else {0}, "input_events": input_count, "input_turns": replay.turns.len(), "input_tools": replay.tool_calls.len(), "reset_paths": replay.reset_path_hashes.len(), "temp_volume": root.path().components().next().map(|v| format!("{v:?}")), "raw": false, "provider_index": false, "parallel_writers": 1, "seed_variant": "Baseline", "input_retained_events": input_count, "seed_max_retained_events": files * per_file, "event_batch_size": 1000, "cache_or_batch_changed": false});
    let hash = digest_file(&paths.db_path)?;
    Ok(Fixture {
        _root: root,
        seed_path: paths.db_path,
        seed_sha256: hash,
        input: vec![replay],
        manifest,
    })
}

fn measure(fixture: &Fixture, detailed: bool) -> Result<Sample> {
    measure_variant(fixture, detailed, Variant::Baseline)
}

fn measure_variant(fixture: &Fixture, detailed: bool, variant: Variant) -> Result<Sample> {
    let _variant = VariantGuard::set(variant);
    assert_eq!(fixture.seed_sha256, digest_file(&fixture.seed_path)?);
    let dir = temp_root()?;
    let paths = AppPaths::with_root(dir.path().to_path_buf())?;
    fs::copy(&fixture.seed_path, &paths.db_path)?;
    let store = Store::new(&paths)?;
    let lock =
        store.acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Library)?;
    let heartbeat = lock.start_default_heartbeat();
    let input = fixture.input.clone();
    let _clock = AuditClock::fixed();
    let capture = Capture::start(detailed);
    let started = Instant::now();
    let mut writer = lock.fenced_store().begin_sync_run()?;
    let begin_ns = started.elapsed().as_nanos();
    let mut public_ms = 0u64;
    for shard in input {
        public_ms += writer.commit_shard(shard)?.write_ms;
    }
    let finish_started = Instant::now();
    writer.finish_sync_run()?;
    let finish_ns = finish_started.elapsed().as_nanos();
    let total_ns = started.elapsed().as_nanos();
    let records = capture.take();
    let write_ns = records.iter().map(|r| r.write_ns).sum::<u128>();
    assert!(write_ns >= public_ms as u128 * 1_000_000);
    assert!(write_ns - (public_ms as u128 * 1_000_000) < records.len() as u128 * 1_000_000);
    drop(capture);
    drop(heartbeat);
    drop(lock);
    checkpoint(&paths.db_path)?;
    Ok(Sample {
        _dir: dir,
        db_path: paths.db_path,
        total_ns,
        begin_ns,
        finish_ns,
        write_ns,
        records,
    })
}

pub(crate) fn metadata(path: &Path) -> Result<serde_json::Value> {
    let paths = AppPaths::with_root(path.parent().unwrap().to_path_buf())?;
    let conn = Store::new(&paths)?.open_connection()?;
    let mut pragmas = serde_json::Map::new();
    for name in [
        "journal_mode",
        "synchronous",
        "foreign_keys",
        "temp_store",
        "busy_timeout",
        "page_size",
        "cache_size",
        "wal_autocheckpoint",
        "user_version",
    ] {
        let value: rusqlite::types::Value =
            conn.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))?;
        pragmas.insert(name.into(), json!(format!("{value:?}")));
    }
    let minimum_observed_clock_interval_ns = (0..10_000)
        .map(|_| Instant::now().elapsed().as_nanos())
        .filter(|v| *v > 0)
        .min();
    let schema_version: String = conn.query_row(
        "SELECT value FROM meta WHERE key='schema_version'",
        [],
        |r| r.get(0),
    )?;
    Ok(
        json!({"schema_version": schema_version, "minimum_observed_clock_interval_ns":minimum_observed_clock_interval_ns, "sqlite": rusqlite::version(), "pragmas": pragmas, "package": env!("CARGO_PKG_VERSION"), "profile": if cfg!(debug_assertions) { "debug" } else { "release" }, "clock": "Instant Duration nanoseconds; resolution measured separately", "cache_state": "warm OS cache; database copy and checkpoint outside timing"}),
    )
}

#[test]
#[ignore = "explicit single-thread original writer stage profile"]
fn sync_writer_replay_profile() -> Result<()> {
    let fixture = replay_fixture(10)?;
    eprintln!(
        "{}",
        json!({"kind": "manifest", "fixture": fixture.manifest, "seed_sha256": fixture.seed_sha256, "environment": metadata(&fixture.seed_path)?})
    );
    let _ = measure(&fixture, false)?;
    for round in 0..3 {
        for detailed in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let sample = measure(&fixture, detailed)?;
            eprintln!(
                "{}",
                json!({"kind": "baseline_profile", "round": round, "detailed": detailed, "total_ns": sample.total_ns, "write_ns": sample.write_ns, "records": sample.records})
            );
            let conn = Connection::open(&sample.db_path)?;
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM usage_event", [], |r| r.get(0))?;
            assert_eq!(count, 51_000);
        }
    }
    Ok(())
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

// Compare rows as a stream. Large histories must not become an in-memory oracle.
pub(crate) fn assert_database_equal(a: &Path, b: &Path) -> Result<serde_json::Value> {
    use rusqlite::types::ValueRef;
    let a = Connection::open(a)?;
    let b = Connection::open(b)?;
    let schema = |conn: &Connection| -> Result<Vec<(String, String)>> {
        Ok(conn.prepare("SELECT name, COALESCE(sql, '') FROM sqlite_master WHERE type='table' ORDER BY name")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
    };
    let complete_schema = |conn: &Connection| -> Result<Vec<(String, String, String, String)>> {
        Ok(conn
            .prepare(
                "SELECT type,name,tbl_name,COALESCE(sql,'') FROM sqlite_schema ORDER BY type,name",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    };
    assert_eq!(
        complete_schema(&a)?,
        complete_schema(&b)?,
        "indexes, triggers, views and tables"
    );
    let tables = schema(&a)?;
    assert_eq!(tables, schema(&b)?, "complete table schema");
    let mut summary = serde_json::Map::new();
    let mut digest_a = Sha256::new();
    let mut digest_b = Sha256::new();
    let mut max_cost_error = 0.0_f64;
    for (table, _) in tables {
        let columns = a
            .prepare(&format!("PRAGMA table_info({})", quote_identifier(&table)))?
            .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut keys = columns
            .iter()
            .filter(|(_, pk)| *pk != 0)
            .collect::<Vec<_>>();
        keys.sort_by_key(|(_, pk)| *pk);
        let order = if keys.is_empty() {
            columns
                .iter()
                .map(|(name, _)| quote_identifier(name))
                .collect::<Vec<_>>()
        } else {
            keys.iter()
                .map(|(name, _)| quote_identifier(name))
                .collect()
        }
        .join(",");
        let sql = format!(
            "SELECT * FROM {} ORDER BY {order}",
            quote_identifier(&table)
        );
        let mut sa = a.prepare(&sql)?;
        let mut sb = b.prepare(&sql)?;
        let mut ra = sa.query([])?;
        let mut rb = sb.query([])?;
        let mut count = 0usize;
        loop {
            let (left, right) = (ra.next()?, rb.next()?);
            match (left, right) {
                (None, None) => break,
                (Some(left), Some(right)) => {
                    for (index, (column, _)) in columns.iter().enumerate() {
                        let (va, vb) = (left.get_ref(index)?, right.get_ref(index)?);
                        match (va, vb) {
                            (ValueRef::Real(x), ValueRef::Real(y))
                                if column.starts_with("cost_") =>
                            {
                                let error = (x - y).abs();
                                max_cost_error = max_cost_error.max(error);
                                assert!(error <= 1e-9, "{table}.{column}, row {count}: {x} != {y}");
                            }
                            _ => assert_eq!(va, vb, "{table}.{column}, row {count}"),
                        }
                        digest_a.update(format!("{table}:{column}:{va:?}\n"));
                        digest_b.update(format!("{table}:{column}:{vb:?}\n"));
                    }
                    count += 1;
                }
                _ => panic!("{table} row count differs after row {count}"),
            }
        }
        summary.insert(table, json!(count));
    }
    Ok(
        json!({"rows": summary, "baseline_digest": digest_a.finalize().iter().map(|v| format!("{v:02x}")).collect::<String>(), "candidate_digest": digest_b.finalize().iter().map(|v| format!("{v:02x}")).collect::<String>(), "maximum_cost_error": max_cost_error}),
    )
}

pub(crate) fn distribution(values: impl Iterator<Item = u128>) -> serde_json::Value {
    let mut values = values.collect::<Vec<_>>();
    values.sort_unstable();
    let median = |sorted: &[f64]| {
        if sorted.len().is_multiple_of(2) {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
        } else {
            sorted[sorted.len() / 2]
        }
    };
    let middle = median(&values.iter().map(|v| *v as f64).collect::<Vec<_>>());
    let mut deviations = values
        .iter()
        .map(|value| (*value as f64 - middle).abs())
        .collect::<Vec<_>>();
    deviations.sort_by(f64::total_cmp);
    json!({"median_ns": middle, "min_ns": values[0], "max_ns": values[values.len()-1], "iqr_ns": values[values.len()*3/4] - values[values.len()/4], "mad_ns": median(&deviations), "quartile_method": "sorted indices n/4 and 3n/4"})
}

fn reset_query_plans(path: &Path) -> Result<serde_json::Value> {
    let conn = Connection::open(path)?;
    let mut plans = serde_json::Map::new();
    for (name, sql) in [
        (
            "aggregate",
            "SELECT COALESCE(provider_label, ''), model, hour_start, COALESCE(project_hash, ''), SUM(input_tokens), SUM(cache_read_tokens), SUM(cache_creation_tokens), SUM(output_tokens), SUM(reasoning_output_tokens), SUM(total_tokens), SUM(cost_with_cache_usd), SUM(cost_without_cache_usd), COUNT(*) FROM usage_event WHERE source = ?1 AND host_id = ?2 AND source_path_hash = ?3 GROUP BY COALESCE(provider_label, ''), model, hour_start, COALESCE(project_hash, '')",
        ),
        (
            "delete",
            "DELETE FROM usage_event WHERE source = ?1 AND host_id = ?2 AND source_path_hash = ?3",
        ),
    ] {
        let rows = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
            .query_map(["claude", "local", "fixture-000-00"], |r| {
                r.get::<_, String>(3)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        plans.insert(name.into(), json!(rows));
    }
    for (name, sql) in [
        (
            "candidate_path_aggregate",
            super::super::RESET_EVENT_AGGREGATE_SQL,
        ),
        (
            "candidate_path_delete",
            super::super::RESET_EVENT_DELETE_SQL,
        ),
        (
            "candidate_default_aggregate",
            super::super::RESET_EVENT_AGGREGATE_DEFAULT_SQL,
        ),
        (
            "candidate_default_delete",
            super::super::RESET_EVENT_DELETE_DEFAULT_SQL,
        ),
    ] {
        let rows = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
            .query_map(["claude", "local", "fixture-000-00"], |r| {
                r.get::<_, String>(3)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        plans.insert(name.into(), json!(rows));
    }
    let host_plan = conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            super::super::RESET_HOST_COUNT_SQL
        ))?
        .query_map(["local", "claude"], |r| r.get::<_, String>(3))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let path_plan = conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            super::super::RESET_PATH_COUNT_SQL
        ))?
        .query_map(rusqlite::params!["claude", "fixture-000-00", 500], |r| {
            r.get::<_, String>(3)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    plans.insert("host_count_covering".into(), json!(host_plan));
    plans.insert("bounded_path_count_covering".into(), json!(path_plan));
    Ok(json!(plans))
}

#[test]
#[ignore = "explicit single-thread original reset SQL profile"]
fn sync_writer_reset_profile() -> Result<()> {
    let fixture = replay_fixture(10)?;
    eprintln!(
        "{}",
        json!({"kind": "reset_manifest", "fixture": fixture.manifest, "seed_sha256": fixture.seed_sha256, "plans": reset_query_plans(&fixture.seed_path)?})
    );
    let baseline = measure(&fixture, false)?;
    let sample = measure(&fixture, true)?;
    let comparison = assert_database_equal(&baseline.db_path, &sample.db_path)?;
    eprintln!(
        "{}",
        json!({"kind": "reset_profile", "write_ns": sample.write_ns, "total_ns": sample.total_ns, "records": sample.records, "equivalence": comparison, "profile_pair": distribution([baseline.total_ns, sample.total_ns].into_iter())})
    );
    Ok(())
}

fn control_fixture(name: &str) -> Result<Fixture> {
    let mut fixture = match name {
        "insertion" => sized_fixture(0, 0, 0, 0, 0, SourceKind::Codex, false)?,
        "codex_append" => sized_fixture(50, 10, 500, 0, 0, SourceKind::Codex, true)?,
        "duplicate_behavior" => replay_fixture(10)?,
        "shared_bucket_reset" => sized_fixture(1, 2, 2_000, 0, 0, SourceKind::Claude, true)?,
        _ => unreachable!(),
    };
    let mut input = Vec::new();
    if matches!(name, "insertion" | "codex_append") {
        for file in 0..8 {
            let mut shard = SyncShard::new(SourceKind::Codex);
            add_path(
                &mut shard,
                fixture._root.path(),
                0,
                file,
                if name == "insertion" { 500 } else { 1_000 },
                false,
            );
            for i in 0..500 {
                add_event(
                    &mut shard,
                    event(
                        SourceKind::Codex,
                        0,
                        file,
                        i + if name == "insertion" { 0 } else { 500 },
                    ),
                    name != "insertion",
                );
            }
            input.push(shard);
        }
    } else if name == "duplicate_behavior" {
        let mut shard = SyncShard::new(SourceKind::Claude);
        for i in 0..20_000 {
            add_event(
                &mut shard,
                event(
                    SourceKind::Claude,
                    i / 5_000,
                    i / 500 % 10,
                    i % 500 + if i < 18_000 { 0 } else { 500 },
                ),
                true,
            );
        }
        shard.turns.extend_from_within(..5_000);
        shard.tool_calls.extend_from_within(..15_000);
        input.push(shard);
    } else {
        let mut shard = SyncShard::new(SourceKind::Claude);
        add_path(&mut shard, fixture._root.path(), 0, 0, 2_050, true);
        for i in 0..2_050 {
            add_event(&mut shard, event(SourceKind::Claude, 0, 0, i), true);
        }
        input.push(shard);
    }
    fixture.input = input;
    fixture.manifest["workload"] = json!(name);
    for (key, value) in [
        (
            "input_events",
            fixture.input.iter().map(|s| s.events.len()).sum::<usize>(),
        ),
        (
            "input_turns",
            fixture.input.iter().map(|s| s.turns.len()).sum(),
        ),
        (
            "input_tools",
            fixture.input.iter().map(|s| s.tool_calls.len()).sum(),
        ),
        (
            "reset_paths",
            fixture
                .input
                .iter()
                .map(|s| s.reset_path_hashes.len())
                .sum(),
        ),
    ] {
        fixture.manifest[key] = json!(value);
    }
    fixture.manifest["input_retained_events"] = fixture.manifest["input_events"].clone();
    Ok(fixture)
}

fn assert_idempotent(fixture: &Fixture, sample: &Sample) -> Result<()> {
    let repeated = Fixture {
        _root: temp_root()?,
        seed_path: sample.db_path.clone(),
        seed_sha256: digest_file(&sample.db_path)?,
        input: fixture.input.clone(),
        manifest: fixture.manifest.clone(),
    };
    let second = measure_variant(&repeated, false, Variant::Candidate)?;
    assert_database_equal(&sample.db_path, &second.db_path)?;
    Ok(())
}

fn run_acceptance(fixture: Fixture, primary: bool) -> Result<bool> {
    let name = fixture.manifest["workload"].as_str().unwrap();
    let rounds = if name == "host_shared_path_skew" {
        15
    } else {
        7
    };
    eprintln!(
        "{}",
        json!({"kind": "ab_manifest", "fixture": fixture.manifest, "query_plans":reset_query_plans(&fixture.seed_path)?, "seed_sha256": fixture.seed_sha256, "environment": metadata(&fixture.seed_path)?, "warmup_per_variant":1, "rounds":rounds, "fine_profile":false})
    );
    let warm_a = measure_variant(&fixture, false, Variant::Baseline)?;
    let warm_b = measure_variant(&fixture, false, Variant::Candidate)?;
    let equivalent = assert_database_equal(&warm_a.db_path, &warm_b.db_path)?;
    assert_idempotent(&fixture, &warm_b)?;
    eprintln!(
        "{}",
        json!({"kind": "warmup_equivalence", "workload": name, "equivalence":equivalent, "idempotent":true})
    );
    drop((warm_a, warm_b));
    let mut baseline_write = Vec::new();
    let mut candidate_write = Vec::new();
    let mut baseline_total = Vec::new();
    let mut candidate_total = Vec::new();
    let mut paired_write = Vec::new();
    let mut paired_total = Vec::new();
    for round in 0..rounds {
        let order = if round % 2 == 0 {
            [Variant::Baseline, Variant::Candidate]
        } else {
            [Variant::Candidate, Variant::Baseline]
        };
        let mut samples = Vec::new();
        for variant in order {
            let sample = measure_variant(&fixture, false, variant)?;
            eprintln!(
                "{}",
                json!({"kind": "ab_sample", "workload":name, "round":round, "order":order, "variant":variant, "write_ns":sample.write_ns, "total_ns":sample.total_ns, "writer_begin_ns":sample.begin_ns, "writer_finish_ns":sample.finish_ns, "records":sample.records})
            );
            samples.push(sample);
        }
        let (a, b) = if round % 2 == 0 {
            (&samples[0], &samples[1])
        } else {
            (&samples[1], &samples[0])
        };
        let equivalent = assert_database_equal(&a.db_path, &b.db_path)?;
        eprintln!(
            "{}",
            json!({"kind": "ab_equivalence", "workload":name, "round":round, "equivalence":equivalent})
        );
        baseline_write.push(a.write_ns);
        candidate_write.push(b.write_ns);
        baseline_total.push(a.total_ns);
        candidate_total.push(b.total_ns);
        paired_write.push(b.write_ns as f64 / a.write_ns as f64);
        paired_total.push(b.total_ns as f64 / a.total_ns as f64);
    }
    let aw = distribution(baseline_write.into_iter());
    let bw = distribution(candidate_write.into_iter());
    let at = distribution(baseline_total.into_iter());
    let bt = distribution(candidate_total.into_iter());
    let write_ratio = bw["median_ns"].as_f64().unwrap() / aw["median_ns"].as_f64().unwrap();
    let total_ratio = bt["median_ns"].as_f64().unwrap() / at["median_ns"].as_f64().unwrap();
    let passed = if primary {
        write_ratio <= 0.80
    } else {
        total_ratio <= 1.10
    };
    eprintln!(
        "{}",
        json!({"kind": "ab_summary", "workload":name, "primary":primary, "baseline_write":aw, "candidate_write":bw, "baseline_total":at, "candidate_total":bt, "write_ratio":write_ratio, "total_ratio":total_ratio, "paired_write_ratios":paired_write, "paired_total_ratios":paired_total, "passed":passed})
    );
    Ok(passed)
}

#[test]
#[ignore = "release-only seven alternating pairs with full database equivalence"]
fn sync_writer_replay_ab_acceptance() -> Result<()> {
    if cfg!(debug_assertions) {
        anyhow::bail!("performance acceptance requires --release");
    }
    let mut passed = run_acceptance(replay_fixture(10)?, true)?;
    passed &= run_acceptance(replay_fixture(50)?, false)?;
    passed &= run_acceptance(host_skew_fixture(50_000)?, false)?;
    for name in [
        "insertion",
        "codex_append",
        "duplicate_behavior",
        "shared_bucket_reset",
    ] {
        passed &= run_acceptance(control_fixture(name)?, false)?;
    }
    assert!(
        passed,
        "candidate must be rejected when any fixed threshold fails"
    );
    Ok(())
}

#[test]
fn reset_path_index_is_selected_for_both_statements() -> Result<()> {
    let fixture = sized_fixture(2, 2, 5, 1, 1, SourceKind::Claude, true)?;
    let conn = Connection::open(&fixture.seed_path)?;
    for sql in [
        super::super::RESET_EVENT_AGGREGATE_SQL,
        super::super::RESET_EVENT_DELETE_SQL,
    ] {
        let plan = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
            .query_map(["claude", "local", "fixture-000-00"], |r| {
                r.get::<_, String>(3)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .join("; ");
        assert!(plan.contains("idx_usage_event_source_path_hash"), "{plan}");
        assert!(plan.contains("source_path_hash=?"), "{plan}");
        assert!(
            !plan.contains("idx_usage_event_host_source_event_at"),
            "{plan}"
        );
    }
    Ok(())
}

#[test]
fn reset_candidate_matches_complete_state_and_is_idempotent() -> Result<()> {
    let fixture = sized_fixture(3, 2, 5, 2, 1, SourceKind::Claude, true)?;
    let baseline = measure_variant(&fixture, false, Variant::Baseline)?;
    let candidate = measure_variant(&fixture, true, Variant::Candidate)?;
    assert_database_equal(&baseline.db_path, &candidate.db_path)?;
    assert_idempotent(&fixture, &candidate)?;
    let record = &candidate.records[0];
    let classified: u128 = record
        .stages_ns
        .iter()
        .filter(|(name, _)| !name.starts_with("pre_"))
        .map(|(_, value)| value)
        .sum();
    assert_eq!(record.write_ns, classified + record.unclassified_ns);
    assert_eq!(record.counts["events_deleted"], 20);
    Ok(())
}

fn host_skew_fixture(remote_events: usize) -> Result<Fixture> {
    let mut fixture = sized_fixture(1, 1, 500, 1, 25, SourceKind::Claude, true)?;
    let paths = AppPaths::with_root(fixture.seed_path.parent().unwrap().to_path_buf())?;
    let store = Store::new(&paths)?;
    let _clock = AuditClock::fixed();
    let _baseline = VariantGuard::set(Variant::Baseline);
    {
        let lock = store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Library)?;
        let fenced = lock.fenced_store();
        fenced.hosts().upsert(&crate::store::Host {
            host_id: "generated-remote".into(),
            label: "Generated remote".into(),
            transport: "ssh".into(),
            ssh_target: Some("fixture.invalid".into()),
            command: "llmusage".into(),
            added_at: AUDIT.into(),
            last_contacted_at: Some(AUDIT.into()),
            last_error: None,
            import_watermark: Some(AUDIT.into()),
        })?;
        let mut writer = fenced.begin_sync_run()?;
        for start in (0..remote_events).step_by(5_000) {
            let mut shard = SyncShard::new_for_host(SourceKind::Claude, "generated-remote");
            for i in start..(start + 5_000).min(remote_events) {
                add_event(&mut shard, event(SourceKind::Claude, 0, 0, i), false);
            }
            writer.commit_shard(shard)?;
        }
        writer.finish_sync_run()?;
    }
    checkpoint(&fixture.seed_path)?;
    fixture.seed_sha256 = digest_file(&fixture.seed_path)?;
    fixture.manifest["workload"] = json!("host_shared_path_skew");
    fixture.manifest["initial_events"] = json!(500 + remote_events);
    fixture.manifest["selected_host_events"] = json!(500);
    fixture.manifest["seed_max_retained_events"] = json!(5_000);
    fixture.manifest["other_host_same_path_events"] = json!(remote_events);
    Ok(fixture)
}

#[test]
#[ignore = "release host-skew acceptance for the reset candidate"]
fn sync_writer_host_skew_ab_acceptance() -> Result<()> {
    if cfg!(debug_assertions) {
        anyhow::bail!("performance acceptance requires --release");
    }
    assert!(
        run_acceptance(host_skew_fixture(50_000)?, false)?,
        "host-skew total regression exceeds ten percent"
    );
    Ok(())
}

#[test]
fn distribution_uses_average_of_middle_pair() {
    let stats = distribution([14_870_723_600, 15_107_858_200].into_iter());
    assert_eq!(stats["median_ns"], json!(14_989_290_900.0));
    assert_eq!(stats["mad_ns"], json!(118_567_300.0));
}

fn complete_surface_fixture() -> Result<Fixture> {
    let mut fixture = sized_fixture(2, 2, 5, 1, 2, SourceKind::Claude, true)?;
    let paths = AppPaths::with_root(fixture.seed_path.parent().unwrap().to_path_buf())?;
    let store = Store::new(&paths)?;
    let _clock = AuditClock::fixed();
    let _variant = VariantGuard::set(Variant::Baseline);
    {
        let lock = store
            .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Library)?;
        let fenced = lock.fenced_store();
        fenced.set_raw_archive(true)?;
        fenced.hosts().upsert(&crate::store::Host {
            host_id: "protected-remote".into(),
            label: "Protected generated remote".into(),
            transport: "ssh".into(),
            ssh_target: Some("fixture.invalid".into()),
            command: "llmusage".into(),
            added_at: AUDIT.into(),
            last_contacted_at: Some(AUDIT.into()),
            last_error: None,
            import_watermark: Some(AUDIT.into()),
        })?;
        let mut writer = fenced.begin_sync_run()?;
        for (source, host) in [
            (SourceKind::Claude, "protected-remote"),
            (SourceKind::Grok, "local"),
        ] {
            let mut shard = SyncShard::new_for_host(source, host);
            add_path(&mut shard, fixture._root.path(), 0, 0, 1, false);
            let event = event(source, 0, 0, 0);
            shard.raw_records.push(crate::store::RawRecord {
                event_key: event.event_key.clone(),
                raw_json: r#"{"generated":"protected"}"#.into(),
            });
            add_event(&mut shard, event, true);
            writer.commit_shard(shard)?;
        }
        writer.finish_sync_run()?;
        fenced.set_meta_value("token_accounting_version.protected-remote.claude", "2")?;
    }
    let shard = &mut fixture.input[0];
    shard.events[0].source_cost = Some(crate::models::SourceCost {
        total: 0.25,
        input: Some(0.1),
        output: Some(0.1),
        cache_read: Some(0.025),
        cache_write: Some(0.025),
    });
    shard.events[1].model = "generated-unknown-model".into();
    shard.events[2].tokens.input_tokens = 300_000;
    let tokens = &mut shard.events[2].tokens;
    tokens.total_tokens = tokens.input_tokens
        + tokens.cache_read_tokens
        + tokens.cache_creation_tokens
        + tokens.output_tokens;
    for event in &shard.events {
        shard.raw_records.push(crate::store::RawRecord {
            event_key: event.event_key.clone(),
            raw_json: r#"{"generated":"replacement"}"#.into(),
        });
    }
    shard
        .reset_path_hashes
        .push(shard.reset_path_hashes[0].clone());
    checkpoint(&fixture.seed_path)?;
    fixture.seed_sha256 = digest_file(&fixture.seed_path)?;
    Ok(fixture)
}

#[test]
fn complete_state_covers_raw_pricing_hosts_and_all_rollback_stages() -> Result<()> {
    use super::super::ShardCommitFailpoint;
    let fixture = complete_surface_fixture()?;
    let baseline = measure_variant(&fixture, false, Variant::Baseline)?;
    let candidate = measure_variant(&fixture, true, Variant::Candidate)?;
    assert_database_equal(&baseline.db_path, &candidate.db_path)?;
    assert_idempotent(&fixture, &candidate)?;
    for variant in [Variant::Baseline, Variant::Candidate] {
        for point in [
            ShardCommitFailpoint::Reset,
            ShardCommitFailpoint::Events,
            ShardCommitFailpoint::Cursor,
            ShardCommitFailpoint::SourceFile,
            ShardCommitFailpoint::Raw,
            ShardCommitFailpoint::BehaviorReset,
            ShardCommitFailpoint::Turns,
            ShardCommitFailpoint::ToolCalls,
        ] {
            let dir = temp_root()?;
            let paths = AppPaths::with_root(dir.path().to_path_buf())?;
            fs::copy(&fixture.seed_path, &paths.db_path)?;
            let store = Store::new(&paths)?;
            {
                let lock = store.acquire_worker_lock_with(
                    std::time::Duration::from_secs(1),
                    HolderKind::Library,
                )?;
                let _clock = AuditClock::fixed();
                let _variant = VariantGuard::set(variant);
                let mut writer = lock.fenced_store().begin_sync_run()?;
                assert!(
                    writer
                        .commit_shard_with_failpoint(fixture.input[0].clone(), point)
                        .is_err()
                );
                writer.finish_sync_run()?;
            }
            checkpoint(&paths.db_path)?;
            assert_database_equal(&fixture.seed_path, &paths.db_path)?;
        }
    }
    Ok(())
}

#[test]
fn antigravity_profile_keeps_transaction_and_source_apply_boundaries() -> Result<()> {
    let fixture = sized_fixture(1, 2, 5, 1, 1, SourceKind::Antigravity, true)?;
    let mut completed = Vec::new();
    for variant in [Variant::Baseline, Variant::Candidate] {
        let dir = temp_root()?;
        let paths = AppPaths::with_root(dir.path().to_path_buf())?;
        fs::copy(&fixture.seed_path, &paths.db_path)?;
        let store = Store::new(&paths)?;
        {
            let lock = store
                .acquire_worker_lock_with(std::time::Duration::from_secs(1), HolderKind::Library)?;
            let _clock = AuditClock::fixed();
            let _variant = VariantGuard::set(variant);
            let capture = Capture::start(true);
            let mut writer = lock.fenced_store().begin_sync_run()?;
            let stats = writer.commit_antigravity_snapshot(fixture.input.clone(), false)?;
            writer.finish_sync_run()?;
            let records = capture.take();
            assert_eq!(records.len(), 1);
            let record = &records[0];
            assert_eq!(record.kind, "antigravity_transaction");
            assert_eq!(record.source_apply_ns.len(), stats.len());
            assert!(record.write_ns > record.source_apply_ns.iter().sum::<u128>());
            assert_eq!(
                stats[0].write_ms as u128,
                record.source_apply_ns[0] / 1_000_000
            );
            assert!(record.stages_ns.contains_key("group_reset"));
            assert!(record.stages_ns.contains_key("commit"));
        }
        checkpoint(&paths.db_path)?;
        completed.push((dir, paths.db_path));
    }
    assert_database_equal(&completed[0].1, &completed[1].1)?;
    Ok(())
}

#[test]
fn host_skew_selects_the_smaller_host_range_and_preserves_other_host() -> Result<()> {
    let fixture = host_skew_fixture(1_000)?;
    let baseline = measure_variant(&fixture, false, Variant::Baseline)?;
    let candidate = measure_variant(&fixture, true, Variant::Candidate)?;
    assert_database_equal(&baseline.db_path, &candidate.db_path)?;
    assert_eq!(candidate.records[0].counts["reset_default_plan_paths"], 1);
    let conn = Connection::open(&candidate.db_path)?;
    let remote: i64 = conn.query_row(
        "SELECT COUNT(*) FROM usage_event WHERE host_id='generated-remote'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(remote, 1_000);
    Ok(())
}
