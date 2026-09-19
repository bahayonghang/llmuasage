use super::decode::{normalize, read_file};
use super::*;
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
