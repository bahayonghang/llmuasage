use super::decode::{normalize, read_file};
use super::*;
use crate::domain::source_diagnostics::{SourceIssue, SourceIssueCode, SourceIssues};
use base64::{Engine, engine::general_purpose::STANDARD};
use rusqlite::Connection;
use tempfile::TempDir;

fn varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = (value & 127) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 128;
        }
        bytes.push(byte);
        if value == 0 {
            return bytes;
        }
    }
}
fn number(field: u64, value: u64) -> Vec<u8> {
    [varint(field << 3), varint(value)].concat()
}
fn bytes(field: u64, value: &[u8]) -> Vec<u8> {
    [
        varint((field << 3) | 2),
        varint(value.len() as u64),
        value.to_vec(),
    ]
    .concat()
}
fn usage(id: &str, input: u64, visible: u64, thinking: u64, write: u64) -> Vec<u8> {
    [
        number(1, 1318),
        number(2, input),
        number(3, visible + thinking),
        number(4, write),
        number(5, 12),
        number(9, thinking),
        number(10, visible),
        bytes(7, b"shared-message"),
        bytes(11, id.as_bytes()),
    ]
    .concat()
}
fn generation(usage: &[u8], retries: &[Vec<u8>], timestamp: u64) -> Vec<u8> {
    let mut chat = [
        bytes(4, usage),
        bytes(9, &bytes(4, &number(1, timestamp))),
        bytes(19, b"gemini-3.8-flash"),
    ]
    .concat();
    for retry in retries {
        chat.extend(bytes(17, &bytes(2, retry)));
    }
    bytes(1, &chat)
}
fn database(dir: &TempDir, name: &str) -> (PathBuf, Connection) {
    let path = dir.path().join(name);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE gen_metadata(idx INTEGER PRIMARY KEY, data BLOB);
        CREATE TABLE steps(idx INTEGER PRIMARY KEY, metadata BLOB);
        CREATE TABLE trajectory_meta(source INTEGER);",
    )
    .unwrap();
    (path, conn)
}
fn events(path: &Path) -> Vec<crate::models::UsageEvent> {
    let file = read_file(path, SourceKind::Antigravity, &CancellationToken::new()).unwrap();
    normalize(file.observations, &mut HashMap::new()).unwrap()
}

fn preflight_listing(path: &Path, source: SourceKind) -> FamilyInputs {
    FamilyInputs {
        started: Instant::now(),
        listings: FAMILY.map(|kind| {
            (
                kind,
                source_files::SourceFileListing {
                    root: path.parent().unwrap().to_path_buf(),
                    paths: if kind == source {
                        vec![path.to_path_buf()]
                    } else {
                        Vec::new()
                    },
                    errors: Vec::new(),
                },
            )
        }),
        metadata: |path| std::fs::metadata(path),
    }
}

fn preflight_snapshot(store: &Store) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    let connection = store.open_connection().unwrap();
    [
        "SELECT * FROM usage_event ORDER BY event_key",
        "SELECT * FROM usage_bucket_30m ORDER BY host_id,source,hour_start,model",
        "SELECT * FROM usage_event_raw ORDER BY event_key",
        "SELECT * FROM usage_turn ORDER BY turn_key",
        "SELECT * FROM usage_tool_call ORDER BY tool_call_key",
        "SELECT * FROM source_cursor ORDER BY host_id,source,cursor_key",
        "SELECT * FROM source_file ORDER BY host_id,source,file_path",
        "SELECT * FROM meta WHERE key LIKE 'token_accounting_version.%' ORDER BY key",
    ]
    .into_iter()
    .map(|sql| {
        let mut statement = connection.prepare(sql).unwrap();
        let count = statement.column_count();
        statement
            .query_map([], |row| {
                (0..count)
                    .map(|column| row.get(column))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    })
    .collect()
}

fn preflight_database(dir: &TempDir, name: &str, source: SourceKind) -> PathBuf {
    let (path, connection) = database(dir, name);
    connection
        .execute(
            "INSERT INTO trajectory_meta VALUES (?1)",
            [if source == FAMILY[0] { 17 } else { 1 }],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO gen_metadata VALUES (1, ?1)",
            [generation(&usage(name, 10, 3, 2, 1), &[], 1_800_000_000)],
        )
        .unwrap();
    drop(connection);
    path.canonicalize().unwrap()
}

fn preflight_store(dir: &TempDir) -> Store {
    let paths = crate::paths::AppPaths::with_root(dir.path().join("store")).unwrap();
    let store = Store::new(&paths).unwrap();
    store.bootstrap().unwrap();
    store
}

fn permission_probe(path: &Path) -> std::io::Result<std::fs::Metadata> {
    if path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("denied")
    {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    } else {
        std::fs::metadata(path)
    }
}

#[tokio::test]
async fn preflight_all_selected_blocked_never_calls_usage_decoder() {
    for selected in [vec![FAMILY[0]], vec![FAMILY[1]], FAMILY.to_vec()] {
        let dir = TempDir::new().unwrap();
        let (path, connection) = database(&dir, "remaining.db");
        connection
            .execute(
                "INSERT INTO trajectory_meta VALUES (?1)",
                [if selected[0] == FAMILY[0] { 17 } else { 1 }],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO gen_metadata VALUES (1, ?1)",
                [generation(
                    &usage("request", 10, 3, 2, 1),
                    &[],
                    1_800_000_000,
                )],
            )
            .unwrap();
        drop(connection);
        let path = path.canonicalize().unwrap();
        let paths = crate::paths::AppPaths::with_root(dir.path().join("store")).unwrap();
        let store = Store::new(&paths).unwrap();
        store.bootstrap().unwrap();
        let mut writer = store.begin_sync_run().unwrap();
        sync_family_with_inputs(
            &store,
            &mut writer,
            &selected,
            true,
            false,
            None,
            &CancellationToken::new(),
            None,
            preflight_listing(&path, selected[0]),
        )
        .await
        .unwrap();
        for source in &selected {
            let mut shard = SyncShard::new(*source);
            shard.seen_file_paths.push(
                dir.path()
                    .join(format!("missing-{source}.db"))
                    .to_string_lossy()
                    .into_owned(),
            );
            writer.commit_shard(shard).unwrap();
        }
        let before = preflight_snapshot(&store);
        assert_eq!(before[0].len(), 1);
        let mut inputs = preflight_listing(&path, selected[0]);
        inputs.started -= std::time::Duration::from_millis(25);
        let mut progress_events = Vec::new();
        let mut sink = |event| progress_events.push(event);
        let observed_before = Utc::now();
        decode::test_reads::reset(&path);
        let result = sync_family_with_inputs(
            &store,
            &mut writer,
            &selected,
            false,
            false,
            None,
            &CancellationToken::new(),
            Some(&mut sink),
            inputs,
        )
        .await
        .unwrap();
        assert_eq!(decode::test_reads::take(&path), 0, "selected={selected:?}");
        assert_eq!(preflight_snapshot(&store), before);
        assert_eq!(progress_events.len(), selected.len());
        for (event, source) in progress_events.iter().zip(&selected) {
            assert!(
                matches!(event, SyncEvent::SourceStarted { source: actual, .. } if actual == source)
            );
        }
        for stat in result.stats {
            let expected_files = if stat.source == selected[0] { 1 } else { 0 };
            assert_eq!(stat.files_processed, expected_files);
            assert_eq!(stat.changed_files, 0);
            assert_eq!(stat.events_seen, 0);
            assert_eq!(stat.events_replayed, 0);
            assert_eq!(stat.events_inserted, 0);
            assert_eq!(stat.bytes_scanned, 0);
            assert_eq!(stat.write_ms, 0);
            assert!(stat.parse_ms >= 25);
            assert!(stat.last_error.unwrap().contains("history preserved"));
            let issue = &result.source_issues[&stat.source][0];
            assert_eq!(issue.code, SourceIssueCode::TrackedMemberMissing);
            assert_eq!(issue.count, 1);
            assert!(issue.observed_at >= observed_before && issue.observed_at <= Utc::now());
        }
    }
}

#[tokio::test]
async fn preflight_uncovered_members_distinguish_missing_scope_and_access() {
    for (name, exists, expected) in [
        ("gone.db", false, SourceIssueCode::TrackedMemberMissing),
        (
            "legacy.json",
            true,
            SourceIssueCode::TrackedMemberOutOfScope,
        ),
        (
            "old-root.db",
            true,
            SourceIssueCode::TrackedMemberOutOfScope,
        ),
        ("denied.db", true, SourceIssueCode::TrackedMemberUnreadable),
    ] {
        for allow_loss in [false, true] {
            let dir = TempDir::new().unwrap();
            let path = preflight_database(&dir, "live.db", FAMILY[0]);
            let store = preflight_store(&dir);
            let mut writer = store.begin_sync_run().unwrap();
            sync_family_with_inputs(
                &store,
                &mut writer,
                &[FAMILY[0]],
                true,
                false,
                None,
                &CancellationToken::new(),
                None,
                preflight_listing(&path, FAMILY[0]),
            )
            .await
            .unwrap();
            let tracked = dir.path().join(name);
            if exists {
                std::fs::write(&tracked, b"old input").unwrap();
            }
            let mut shard = SyncShard::new(FAMILY[0]);
            shard
                .seen_file_paths
                .push(tracked.to_string_lossy().into_owned());
            writer.commit_shard(shard).unwrap();
            let before = preflight_snapshot(&store);
            let mut inputs = preflight_listing(&path, FAMILY[0]);
            inputs.metadata = permission_probe;
            decode::test_reads::reset(&path);
            let result = sync_family_with_inputs(
                &store,
                &mut writer,
                &[FAMILY[0]],
                true,
                allow_loss,
                None,
                &CancellationToken::new(),
                None,
                inputs,
            )
            .await
            .unwrap();
            if allow_loss && expected != SourceIssueCode::TrackedMemberUnreadable {
                assert_eq!(decode::test_reads::take(&path), 1, "{name}");
                assert!(result.source_issues.is_empty());
                assert!(result.stats[0].last_error.is_none());
            } else {
                assert_eq!(decode::test_reads::take(&path), 0, "{name}");
                assert_eq!(preflight_snapshot(&store), before, "{name}");
                let issue = &result.source_issues[&FAMILY[0]][0];
                assert_eq!(issue.code, expected, "{name}");
                assert_eq!(issue.count, 1);
                assert_eq!(result.stats[0].parse_issues.total(), 0);
            }
        }
    }
}

#[tokio::test]
async fn preflight_unselected_root_failure_blocks_unknown_product_copies() {
    for selected in FAMILY {
        for case in ["permission", "not-directory", "walk-error", "missing-root"] {
            let dir = TempDir::new().unwrap();
            let path = preflight_database(&dir, "live.db", selected);
            let store = preflight_store(&dir);
            let mut writer = store.begin_sync_run().unwrap();
            sync_family_with_inputs(
                &store,
                &mut writer,
                &[selected],
                true,
                false,
                None,
                &CancellationToken::new(),
                None,
                preflight_listing(&path, selected),
            )
            .await
            .unwrap();
            let before = preflight_snapshot(&store);
            let mut inputs = preflight_listing(&path, selected);
            let (_, sibling) = inputs
                .listings
                .iter_mut()
                .find(|(source, _)| *source != selected)
                .unwrap();
            match case {
                "permission" => {
                    sibling.root = dir.path().join("denied-root");
                    inputs.metadata = permission_probe;
                }
                "not-directory" => sibling.root = path.clone(),
                "walk-error" => sibling.errors.push("synthetic enumeration failure".into()),
                "missing-root" => sibling.root = dir.path().join("absent-root"),
                _ => unreachable!(),
            }
            decode::test_reads::reset(&path);
            let result = sync_family_with_inputs(
                &store,
                &mut writer,
                &[selected],
                true,
                true,
                None,
                &CancellationToken::new(),
                None,
                inputs,
            )
            .await
            .unwrap();
            if case == "missing-root" {
                assert_eq!(decode::test_reads::take(&path), 1);
                assert!(result.source_issues.is_empty());
            } else {
                assert_eq!(decode::test_reads::take(&path), 0, "{selected}: {case}");
                assert_eq!(preflight_snapshot(&store), before);
                let issue = &result.source_issues[&selected][0];
                assert_eq!(issue.code, SourceIssueCode::DiscoveryIncomplete);
                assert_eq!(issue.count, 1);
            }
        }
    }
}

#[tokio::test]
async fn preflight_partial_blocking_keeps_cross_root_decode_and_selected_scope() {
    for selected in [vec![FAMILY[0]], FAMILY.to_vec()] {
        let dir = TempDir::new().unwrap();
        let cli = preflight_database(&dir, "cli.db", FAMILY[0]);
        let ide = preflight_database(&dir, "ide.db", FAMILY[1]);
        let store = preflight_store(&dir);
        let mut writer = store.begin_sync_run().unwrap();
        let mut shard = SyncShard::new(FAMILY[1]);
        shard.seen_file_paths.push(
            dir.path()
                .join("missing-ide.db")
                .to_string_lossy()
                .into_owned(),
        );
        writer.commit_shard(shard).unwrap();
        let mut inputs = preflight_listing(&cli, FAMILY[0]);
        // Both native products are intentionally copied into the opposite roots.
        inputs.listings[0].1.paths = vec![ide.clone()];
        inputs.listings[1].1.paths = vec![cli.clone()];
        decode::test_reads::reset(&cli);
        decode::test_reads::reset(&ide);
        let result = sync_family_with_inputs(
            &store,
            &mut writer,
            &selected,
            false,
            false,
            None,
            &CancellationToken::new(),
            None,
            inputs,
        )
        .await
        .unwrap();
        assert_eq!(decode::test_reads::take(&cli), 1);
        assert_eq!(decode::test_reads::take(&ide), 1);
        assert_eq!(result.stats[0].source, FAMILY[0]);
        assert_eq!(result.stats[0].events_inserted, 1);
        assert!(result.stats[0].last_error.is_none());
        if selected.len() == 2 {
            assert_eq!(result.stats[1].events_inserted, 0);
            assert!(result.stats[1].last_error.is_some());
        }
    }
}

#[tokio::test]
async fn preflight_empty_selection_and_cancellation_do_not_decode_or_write() {
    for selected in [Vec::new(), vec![FAMILY[0]]] {
        let dir = TempDir::new().unwrap();
        let path = preflight_database(&dir, "live.db", FAMILY[0]);
        let store = preflight_store(&dir);
        let mut writer = store.begin_sync_run().unwrap();
        let before = preflight_snapshot(&store);
        let cancel = CancellationToken::new();
        if !selected.is_empty() {
            cancel.cancel();
        }
        decode::test_reads::reset(&path);
        let result = sync_family_with_inputs(
            &store,
            &mut writer,
            &selected,
            false,
            false,
            None,
            &cancel,
            None,
            preflight_listing(&path, FAMILY[0]),
        )
        .await
        .unwrap();
        assert_eq!(decode::test_reads::take(&path), 0);
        assert_eq!(result.stats.len(), selected.len());
        assert_eq!(preflight_snapshot(&store), before);
    }
}

#[tokio::test]
async fn preflight_fingerprint_failure_does_not_accept_lossy_rebuild() {
    let dir = TempDir::new().unwrap();
    let path = preflight_database(&dir, "live.db", FAMILY[0]);
    let store = preflight_store(&dir);
    let mut writer = store.begin_sync_run().unwrap();
    sync_family_with_inputs(
        &store,
        &mut writer,
        &[FAMILY[0]],
        true,
        false,
        None,
        &CancellationToken::new(),
        None,
        preflight_listing(&path, FAMILY[0]),
    )
    .await
    .unwrap();
    let before = preflight_snapshot(&store);
    let mut inputs = preflight_listing(&path, FAMILY[0]);
    inputs.listings[1]
        .1
        .paths
        .push(dir.path().join("disappeared-after-discovery.db"));
    decode::test_reads::reset(&path);
    let result = sync_family_with_inputs(
        &store,
        &mut writer,
        &[FAMILY[0]],
        true,
        true,
        None,
        &CancellationToken::new(),
        None,
        inputs,
    )
    .await
    .unwrap();
    assert_eq!(decode::test_reads::take(&path), 0);
    assert_eq!(preflight_snapshot(&store), before);
    assert_eq!(
        result.source_issues[&FAMILY[0]][0].code,
        SourceIssueCode::FingerprintUnavailable
    );
}

#[tokio::test]
async fn preflight_remote_membership_never_probes_local_filesystem() {
    let dir = TempDir::new().unwrap();
    let path = preflight_database(&dir, "live.db", FAMILY[0]);
    let store = preflight_store(&dir);
    let mut writer = store.begin_sync_run().unwrap();
    let mut shard = SyncShard::new_for_host(FAMILY[0], "remote");
    shard
        .seen_file_paths
        .push("/remote-only/private/database.db".into());
    writer.commit_shard(shard).unwrap();
    let mut inputs = preflight_listing(&path, FAMILY[0]);
    inputs.metadata = |path| {
        assert!(
            !path.to_string_lossy().contains("remote-only"),
            "remote membership must not be probed locally"
        );
        std::fs::metadata(path)
    };
    let result = sync_family_with_inputs(
        &store,
        &mut writer,
        &[FAMILY[0]],
        false,
        false,
        None,
        &CancellationToken::new(),
        None,
        inputs,
    )
    .await
    .unwrap();
    assert!(result.source_issues.is_empty());
    assert_eq!(result.stats[0].events_inserted, 1);
    assert_eq!(
        store
            .source_files()
            .tracked_paths(FAMILY[0], "remote")
            .unwrap(),
        ["/remote-only/private/database.db"]
    );
}

#[test]
fn sanitized_native_samples_match_independent_six_channel_oracle() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/antigravity-native-usage.json"
    ))
    .unwrap();
    for sample in fixture["samples"].as_array().unwrap() {
        let dir = TempDir::new().unwrap();
        let (path, connection) = database(&dir, "native.db");
        connection
            .execute(
                "INSERT INTO trajectory_meta VALUES (?1)",
                [sample["native_source_enum"].as_i64().unwrap()],
            )
            .unwrap();
        for row in sample["gen_metadata"].as_array().unwrap() {
            connection
                .execute(
                    "INSERT INTO gen_metadata VALUES (?1, ?2)",
                    rusqlite::params![
                        row["idx"].as_i64().unwrap(),
                        STANDARD
                            .decode(row["data_base64"].as_str().unwrap())
                            .unwrap()
                    ],
                )
                .unwrap();
        }
        for row in sample["steps"].as_array().unwrap() {
            connection
                .execute(
                    "INSERT INTO steps VALUES (?1, ?2)",
                    rusqlite::params![
                        row["idx"].as_i64().unwrap(),
                        STANDARD
                            .decode(row["metadata_base64"].as_str().unwrap())
                            .unwrap()
                    ],
                )
                .unwrap();
        }
        drop(connection);
        let parsed = events(&path);
        let empty = Vec::new();
        let records = sample["oracle_usage_records"].as_array().unwrap_or(&empty);
        let has_attempts = records.iter().any(|row| {
            row["lane"].as_str().unwrap().starts_with("retry_")
                && row["total_tokens"].as_u64().unwrap() > 0
        });
        let expected: Vec<_> = records
            .iter()
            .filter(|row| {
                (row["lane"].as_str().unwrap().starts_with("retry_") == has_attempts)
                    && row["total_tokens"].as_u64().unwrap() > 0
            })
            .collect();
        assert_eq!(
            parsed.len(),
            expected.len(),
            "{} {}",
            sample["family"],
            sample["category"]
        );
        let sums = |field: &str| {
            expected
                .iter()
                .map(|row| row[field].as_i64().unwrap())
                .sum::<i64>()
        };
        assert_eq!(
            parsed.iter().map(|e| e.tokens.input_tokens).sum::<i64>(),
            sums("input")
        );
        assert_eq!(
            parsed.iter().map(|e| e.tokens.output_tokens).sum::<i64>(),
            sums("output_visible")
        );
        assert_eq!(
            parsed
                .iter()
                .map(|e| e.tokens.reasoning_output_tokens)
                .sum::<i64>(),
            sums("reasoning")
        );
        assert_eq!(
            parsed
                .iter()
                .map(|e| e.tokens.cache_creation_tokens)
                .sum::<i64>(),
            sums("cache_write")
        );
        assert_eq!(
            parsed
                .iter()
                .map(|e| e.tokens.cache_read_tokens)
                .sum::<i64>(),
            sums("cache_read")
        );
        assert_eq!(
            parsed.iter().map(|e| e.tokens.total_tokens).sum::<i64>(),
            sums("total_tokens")
        );
        let source = if sample["native_source_enum"] == 17 {
            SourceKind::Antigravity
        } else {
            SourceKind::AntigravityIde
        };
        assert!(parsed.iter().all(|e| e.source == source));
    }
}

#[test]
fn cache_write_and_model_enum_have_distinct_semantics() {
    let dir = TempDir::new().unwrap();
    let (path, connection) = database(&dir, "channels.db");
    let blob = generation(&usage("one", 100, 20, 30, 50), &[], 1_800_000_000);
    connection
        .execute("INSERT INTO gen_metadata VALUES (7, ?1)", [&blob])
        .unwrap();
    let parsed = events(&path);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].tokens.input_tokens, 100);
    assert_eq!(parsed[0].tokens.cache_creation_tokens, 50);
    assert_eq!(parsed[0].tokens.output_tokens, 20);
    assert_eq!(parsed[0].tokens.reasoning_output_tokens, 30);
    assert_eq!(parsed[0].tokens.total_tokens, 212);
}

#[test]
fn constructed_descriptor_fixture_preserves_one_sided_output_channels() {
    // Descriptor-based constructed cases; the native corpus does not establish
    // observed positive thinking-only or visible-only usage.
    for (visible, thinking) in [(0, 37), (37, 0)] {
        let dir = TempDir::new().unwrap();
        let (path, connection) = database(&dir, "one-sided.db");
        connection
            .execute(
                "INSERT INTO gen_metadata VALUES (1, ?1)",
                [generation(
                    &usage("one-sided", 100, visible, thinking, 0),
                    &[],
                    1_800_000_000,
                )],
            )
            .unwrap();
        let parsed = events(&path);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].tokens.output_tokens, visible as i64);
        assert_eq!(parsed[0].tokens.reasoning_output_tokens, thinking as i64);
        assert_eq!(parsed[0].tokens.input_tokens, 100);
        assert_eq!(parsed[0].tokens.cache_read_tokens, 12);
        assert_eq!(parsed[0].tokens.total_tokens, 149);
    }
}

#[test]
fn independent_retries_share_message_but_not_response_identity() {
    let dir = TempDir::new().unwrap();
    let (path, connection) = database(&dir, "retry.db");
    let attempt_a = usage("a", 100, 20, 30, 0);
    let attempt_b = usage("b", 10, 2, 3, 0);
    let blob = generation(
        &usage("a", 110, 22, 33, 0),
        &[attempt_a, attempt_b],
        1_800_000_000,
    );
    connection
        .execute("INSERT INTO gen_metadata VALUES (22, ?1)", [&blob])
        .unwrap();
    let parsed = events(&path);
    assert_eq!(parsed.len(), 2);
    assert_eq!(
        parsed.iter().map(|e| e.tokens.input_tokens).sum::<i64>(),
        110
    );
}

#[test]
fn copied_databases_keep_native_product_and_count_once() {
    let dir = TempDir::new().unwrap();
    let (path, connection) = database(&dir, "original.db");
    connection
        .execute("INSERT INTO trajectory_meta VALUES (17)", [])
        .unwrap();
    connection
        .execute(
            "INSERT INTO gen_metadata VALUES (1, ?1)",
            [generation(&usage("copy", 5, 3, 2, 0), &[], 1_800_000_000)],
        )
        .unwrap();
    drop(connection);
    let copy = dir.path().join("ide-copy.db");
    std::fs::copy(&path, &copy).unwrap();
    let mut original = read_file(&path, SourceKind::Antigravity, &CancellationToken::new())
        .unwrap()
        .observations;
    let copied = read_file(&copy, SourceKind::AntigravityIde, &CancellationToken::new()).unwrap();
    assert_eq!(copied.source, SourceKind::Antigravity);
    original.extend(copied.observations);
    let parsed = normalize(original, &mut HashMap::new()).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].source, SourceKind::Antigravity);
}

#[test]
fn same_time_and_tokens_without_identity_remain_separate() {
    let dir = TempDir::new().unwrap();
    let (path, connection) = database(&dir, "idless.db");
    let raw_usage = [number(1, 1318), number(2, 100), number(3, 2), number(10, 2)].concat();
    for idx in [3, 77] {
        connection
            .execute(
                "INSERT INTO gen_metadata VALUES (?1, ?2)",
                rusqlite::params![idx, generation(&raw_usage, &[], 1_800_000_000)],
            )
            .unwrap();
    }
    let parsed = events(&path);
    assert_eq!(parsed.len(), 2);
    assert_ne!(parsed[0].event_key, parsed[1].event_key);
}

#[test]
fn cache_metadata_is_never_used_as_timestamp() {
    let dir = TempDir::new().unwrap();
    let (path, connection) = database(&dir, "no-time.db");
    let blob = bytes(
        1,
        &[
            bytes(4, &usage("none", 5, 3, 2, 0)),
            bytes(9, &bytes(10, &number(1, 1_800_000_000))),
        ]
        .concat(),
    );
    connection
        .execute("INSERT INTO gen_metadata VALUES (1, ?1)", [&blob])
        .unwrap();
    let file = read_file(&path, SourceKind::Antigravity, &CancellationToken::new()).unwrap();
    assert!(normalize(file.observations, &mut HashMap::new()).is_err());
}

#[test]
fn committed_wal_only_changes_update_fingerprint_and_usage() {
    let dir = TempDir::new().unwrap();
    let (path, connection) = database(&dir, "wal.db");
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    let before = snapshot(&path).unwrap();
    let main_before = std::fs::read(&path).unwrap();
    connection
        .execute(
            "INSERT INTO gen_metadata VALUES (1, ?1)",
            [generation(&usage("wal", 10, 3, 2, 0), &[], 1_800_000_000)],
        )
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), main_before);
    let after = snapshot(&path).unwrap();
    assert_ne!(before.file_fingerprint, after.file_fingerprint);
    assert_eq!(events(&path).len(), 1);
}

#[test]
fn absent_and_empty_wal_have_the_same_usage_fingerprint() {
    let dir = TempDir::new().unwrap();
    let (path, connection) = database(&dir, "empty-wal.db");
    drop(connection);
    let before = snapshot(&path).unwrap();
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    std::fs::write(&wal, []).unwrap();
    assert_eq!(
        before.file_fingerprint,
        snapshot(&path).unwrap().file_fingerprint
    );
}

#[test]
fn malformed_sqlite_and_protobuf_fail_without_partial_events() {
    let dir = TempDir::new().unwrap();
    let (path, connection) = database(&dir, "bad-wire.db");
    connection
        .execute(
            "INSERT INTO gen_metadata VALUES (1, ?1)",
            [vec![0x0a, 0x80]],
        )
        .unwrap();
    assert!(read_file(&path, SourceKind::Antigravity, &CancellationToken::new()).is_err());
    drop(connection);
    std::fs::write(&path, b"not sqlite").unwrap();
    assert!(read_file(&path, SourceKind::Antigravity, &CancellationToken::new()).is_err());
}

#[test]
fn unrelated_sqlite_schema_is_not_a_successful_empty_snapshot() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("unrelated.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE unrelated(value TEXT)")
        .unwrap();
    assert!(read_file(&path, SourceKind::Antigravity, &CancellationToken::new()).is_err());
}

#[test]
fn wrong_usage_wire_type_cannot_reset_a_snapshot_to_zero() {
    let dir = TempDir::new().unwrap();
    let (path, connection) = database(&dir, "wrong-type.db");
    let malformed_usage = bytes(2, b"not-a-number");
    connection
        .execute(
            "INSERT INTO gen_metadata VALUES (1, ?1)",
            [generation(&malformed_usage, &[], 1_800_000_000)],
        )
        .unwrap();
    assert!(read_file(&path, SourceKind::Antigravity, &CancellationToken::new()).is_err());
}

#[test]
fn conflicting_identity_counts_fail_in_both_file_orders() {
    let dir = TempDir::new().unwrap();
    let (path_a, connection_a) = database(&dir, "a.db");
    let (path_b, connection_b) = database(&dir, "b.db");
    connection_a
        .execute(
            "INSERT INTO gen_metadata VALUES (1, ?1)",
            [generation(
                &usage("same", 100, 20, 3, 0),
                &[],
                1_800_000_000,
            )],
        )
        .unwrap();
    connection_b
        .execute(
            "INSERT INTO gen_metadata VALUES (1, ?1)",
            [generation(
                &usage("same", 200, 20, 3, 0),
                &[],
                1_800_000_000,
            )],
        )
        .unwrap();
    for paths in [[&path_a, &path_b], [&path_b, &path_a]] {
        let mut observations = Vec::new();
        for path in paths {
            observations.extend(
                read_file(path, SourceKind::Antigravity, &CancellationToken::new())
                    .unwrap()
                    .observations,
            );
        }
        assert!(normalize(observations, &mut HashMap::new()).is_err());
    }
}

#[test]
fn native_product_overrides_a_root_only_copy() {
    let dir = TempDir::new().unwrap();
    let (native, native_conn) = database(&dir, "native-ide.db");
    let (copy, copy_conn) = database(&dir, "root-only-cli.db");
    native_conn
        .execute("INSERT INTO trajectory_meta VALUES (1)", [])
        .unwrap();
    let blob = generation(&usage("same", 100, 20, 3, 0), &[], 1_800_000_000);
    native_conn
        .execute("INSERT INTO gen_metadata VALUES (1, ?1)", [&blob])
        .unwrap();
    copy_conn
        .execute("INSERT INTO gen_metadata VALUES (1, ?1)", [&blob])
        .unwrap();
    let mut observations = read_file(
        &native,
        SourceKind::AntigravityIde,
        &CancellationToken::new(),
    )
    .unwrap()
    .observations;
    observations.extend(
        read_file(&copy, SourceKind::Antigravity, &CancellationToken::new())
            .unwrap()
            .observations,
    );
    let parsed = normalize(observations, &mut HashMap::new()).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].source, SourceKind::AntigravityIde);
}

#[tokio::test]
async fn coverage_without_prior_diagnostics_reports_actual_disk_gaps() {
    let dir = TempDir::new().unwrap();
    let live_path = preflight_database(&dir, "live.db", FAMILY[0]);
    let store = preflight_store(&dir);
    let mut writer = store.begin_sync_run().unwrap();

    // Seed live.db as tracked
    sync_family_with_inputs(
        &store,
        &mut writer,
        &[FAMILY[0]],
        true,
        false,
        None,
        &CancellationToken::new(),
        None,
        preflight_listing(&live_path, FAMILY[0]),
    )
    .await
    .unwrap();

    // Add a missing tracked path and an out-of-scope tracked path
    let missing_path = dir.path().join("missing.db");
    let out_of_scope_path = dir.path().join("out_of_scope.json");
    std::fs::write(&out_of_scope_path, b"out of scope file content").unwrap();

    let mut shard = SyncShard::new(FAMILY[0]);
    shard
        .seen_file_paths
        .push(missing_path.to_string_lossy().into_owned());
    shard
        .seen_file_paths
        .push(out_of_scope_path.to_string_lossy().into_owned());
    writer.commit_shard(shard).unwrap();

    // Snapshot before coverage check
    let before = preflight_snapshot(&store);

    // Run read-only coverage with inputs containing only live.db
    let coverage = super::check_antigravity_coverage_with_inputs(
        &store,
        FAMILY.map(|kind| {
            (
                kind,
                source_files::SourceFileListing {
                    root: dir.path().to_path_buf(),
                    paths: if kind == FAMILY[0] {
                        vec![live_path.clone()]
                    } else {
                        Vec::new()
                    },
                    errors: Vec::new(),
                },
            )
        }),
        |path| std::fs::metadata(path),
    )
    .unwrap();

    let cli_cov = &coverage[&FAMILY[0]];
    assert_eq!(cli_cov.discovered_count, 1);
    assert_eq!(cli_cov.missing_count, 1);
    assert_eq!(cli_cov.out_of_scope_count, 1);
    assert_eq!(cli_cov.unreadable_count, 0);
    assert!(!cli_cov.discovery_incomplete);
    assert!(cli_cov.can_prompt_for_loss());

    // No changes were written to store, cursors, inventory, or marker
    assert_eq!(preflight_snapshot(&store), before);
}

#[tokio::test]
async fn coverage_ignores_stale_diagnostics_and_derives_from_current_disk() {
    let dir = TempDir::new().unwrap();
    let live_path = preflight_database(&dir, "live.db", FAMILY[0]);
    let store = preflight_store(&dir);
    let mut writer = store.begin_sync_run().unwrap();

    sync_family_with_inputs(
        &store,
        &mut writer,
        &[FAMILY[0]],
        true,
        false,
        None,
        &CancellationToken::new(),
        None,
        preflight_listing(&live_path, FAMILY[0]),
    )
    .await
    .unwrap();

    // Track 1 missing path on disk
    let missing_path = dir.path().join("missing.db");
    let mut shard = SyncShard::new(FAMILY[0]);
    shard
        .seen_file_paths
        .push(missing_path.to_string_lossy().into_owned());
    writer.commit_shard(shard).unwrap();

    // Deliberately write STALE source_issues into sync_status with missing = 999
    let mut stale_issues = SourceIssues::new();
    SourceIssue::record(
        stale_issues.entry(FAMILY[0]).or_default(),
        SourceIssueCode::TrackedMemberMissing,
        999,
    );
    let dummy_status = crate::store::SourceSyncStatus {
        source: FAMILY[0].as_str().to_string(),
        files_processed: 10,
        changed_files: 5,
        bytes_scanned: 1000,
        events_seen: 50,
        events_replayed: 0,
        events_inserted: 20,
        stored_events: 100,
        token_accounting_version: Some(2),
        legacy_token_accounting: true,
        token_accounting_warning: None,
        parse_ms: 10,
        write_ms: 5,
        lock_wait_ms: 0,
        parse_issues: Default::default(),
        updated_at: crate::util::now_utc(),
    };
    store
        .sync_status()
        .save_source_sync_statuses_with_issues("local", &[dummy_status], &stale_issues)
        .unwrap();

    // Check coverage: it must ignore the stale 999 and reflect disk (1 missing)
    let coverage = super::check_antigravity_coverage_with_inputs(
        &store,
        FAMILY.map(|kind| {
            (
                kind,
                source_files::SourceFileListing {
                    root: dir.path().to_path_buf(),
                    paths: if kind == FAMILY[0] {
                        vec![live_path.clone()]
                    } else {
                        Vec::new()
                    },
                    errors: Vec::new(),
                },
            )
        }),
        |path| std::fs::metadata(path),
    )
    .unwrap();

    let cli_cov = &coverage[&FAMILY[0]];
    assert_eq!(
        cli_cov.missing_count, 1,
        "must be 1 from disk, not 999 from stale json"
    );
}
