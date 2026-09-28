//! Generated source controls for the private writer A/B benchmark.

use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Result;
use serde_json::json;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::{ClaudeParser, CodexParser, SourceParser, SourceSyncStats};
use crate::{
    paths::AppPaths,
    store::{
        HolderKind, Store,
        writer_test_support::{
            AuditClock, Capture, Record, Variant, VariantGuard,
            tests::{
                assert_database_equal, checkpoint, digest_file, distribution, metadata, temp_root,
            },
        },
    },
};

struct Environment(Vec<(&'static str, Option<OsString>)>);

impl Environment {
    fn isolated(root: &Path) -> Self {
        let mut previous = Vec::new();
        for (key, value) in [
            ("HOME", root.to_path_buf()),
            ("USERPROFILE", root.to_path_buf()),
            ("CODEX_HOME", root.join(".codex")),
        ] {
            previous.push((key, std::env::var_os(key)));
            // The ignored benchmark runs alone with --test-threads=1.
            unsafe {
                std::env::set_var(key, value);
            }
        }
        Self(previous)
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        for (key, value) in &self.0 {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

struct ParserSample {
    _root: TempDir,
    db: PathBuf,
    total_ns: u128,
    write_ns: u128,
    begin_ns: u128,
    finish_ns: u128,
    stats: SourceSyncStats,
    records: Vec<Record>,
}

fn timestamp(index: usize) -> String {
    (chrono::DateTime::parse_from_rfc3339("2026-08-01T00:00:00Z").unwrap()
        + chrono::Duration::seconds(index as i64))
    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn codex_line(file: usize, index: usize) -> String {
    json!({"timestamp":timestamp(file * 1_000 + index), "type":"event_msg", "payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5,"total_tokens":130},"total_token_usage":{"input_tokens":(index+1)*100,"cached_input_tokens":(index+1)*20,"output_tokens":(index+1)*30,"reasoning_output_tokens":(index+1)*5,"total_tokens":(index+1)*130}}}}).to_string()
}

fn claude_line(project: usize, file: usize, index: usize) -> String {
    json!({"timestamp":timestamp(project*10_000+file*1_000+index), "type":"assistant", "sessionId":format!("session-{project}-{file}"), "requestId":format!("request-{project}-{file}-{index}"),"message":{"id":format!("message-{project}-{file}-{index}"),"model":"claude-sonnet-4-5","usage":{"input_tokens":100,"cache_creation_input_tokens":10,"cache_read_input_tokens":20,"output_tokens":30},"content":[{"type":"tool_use","id":format!("read-{project}-{file}-{index}"),"name":"Read","input":{"file_path":"generated.rs"}}]}}).to_string()
}

fn run_parser(
    runtime: &tokio::runtime::Runtime,
    parser: &dyn SourceParser,
    seed: &Path,
    variant: Variant,
) -> Result<ParserSample> {
    let root = temp_root()?;
    let paths = AppPaths::with_root(root.path().to_path_buf())?;
    fs::copy(seed, &paths.db_path)?;
    let store = Store::new(&paths)?;
    let lock = store.acquire_worker_lock_with(Duration::from_secs(1), HolderKind::Library)?;
    let heartbeat = lock.start_default_heartbeat();
    let fenced = lock.fenced_store();
    let _variant = VariantGuard::set(variant);
    let _clock = AuditClock::fixed();
    let capture = Capture::start(false);
    let started = Instant::now();
    let mut writer = fenced.begin_sync_run()?;
    let begin_ns = started.elapsed().as_nanos();
    let stats = runtime.block_on(parser.parse(
        &fenced,
        &mut writer,
        1,
        None,
        &CancellationToken::new(),
        None,
    ))?;
    let finish_started = Instant::now();
    writer.finish_sync_run()?;
    let finish_ns = finish_started.elapsed().as_nanos();
    let total_ns = started.elapsed().as_nanos();
    let records = capture.take();
    let write_ns = records.iter().map(|r| r.write_ns).sum::<u128>();
    let public_ns = stats.write_ms as u128 * 1_000_000;
    if records.is_empty() {
        assert_eq!(write_ns, 0);
        assert_eq!(public_ns, 0);
    } else {
        assert!(write_ns >= public_ns);
        assert!(write_ns - public_ns < records.len() as u128 * 1_000_000);
    }
    drop(capture);
    drop(heartbeat);
    drop(lock);
    checkpoint(&paths.db_path)?;
    Ok(ParserSample {
        _root: root,
        db: paths.db_path,
        total_ns,
        write_ns,
        begin_ns,
        finish_ns,
        stats,
        records,
    })
}

fn parser_control(name: &str) -> Result<bool> {
    let home = temp_root()?;
    let env = Environment::isolated(home.path());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let claude = name == "claude_project_replay";
    let parser: &dyn SourceParser = if claude { &ClaudeParser } else { &CodexParser };
    let mut files = Vec::new();
    if claude {
        for project in 0..3 {
            for file in 0..4 {
                let path = home.path().join(format!(
                    ".claude/projects/project-{project}/session-{file}.jsonl"
                ));
                fs::create_dir_all(path.parent().unwrap())?;
                let content = (0..250)
                    .map(|i| claude_line(project, file, i))
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n";
                fs::write(&path, content)?;
                files.push(path);
            }
        }
    } else {
        for file in 0..2 {
            let path = home
                .path()
                .join(format!(".codex/sessions/rollout-{file}.jsonl"));
            fs::create_dir_all(path.parent().unwrap())?;
            let header = json!({"type":"session_meta","payload":{"id":format!("generated-{file}"),"model":"gpt-5"}}).to_string();
            let content = header
                + "\n"
                + &(0..500)
                    .map(|i| codex_line(file, i))
                    .collect::<Vec<_>>()
                    .join("\n")
                + "\n";
            fs::write(&path, content)?;
            files.push(path);
        }
    }
    let empty = temp_root()?;
    let empty_paths = AppPaths::with_root(empty.path().to_path_buf())?;
    Store::new(&empty_paths)?.bootstrap()?;
    checkpoint(&empty_paths.db_path)?;
    let initial = run_parser(&runtime, parser, &empty_paths.db_path, Variant::Baseline)?;
    assert_eq!(
        initial.stats.events_inserted,
        if claude { 3_000 } else { 1_000 }
    );
    let mut append_bytes = 0u64;
    if name != "codex_hot" {
        let start = if claude { 250 } else { 500 };
        let content = (start..start + 25)
            .map(|i| {
                if claude {
                    claude_line(0, 0, i)
                } else {
                    codex_line(0, i)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        append_bytes = content.len() as u64;
        fs::OpenOptions::new()
            .append(true)
            .open(&files[0])?
            .write_all(content.as_bytes())?;
    }
    let expected_bytes = if claude {
        files[..4]
            .iter()
            .map(|p| fs::metadata(p).unwrap().len())
            .sum()
    } else {
        append_bytes
    };
    let seed_hash = digest_file(&initial.db)?;
    eprintln!(
        "{}",
        json!({"kind":"parser_manifest","workload":name,"initial_events":initial.stats.events_inserted,"files":files.len(),"changed_files":if claude {4} else if name == "codex_hot" {0} else {1},"expected_scanned_bytes":expected_bytes,"seed_sha256":seed_hash,"environment":metadata(&initial.db)?,"parallelism":1,"fine_profile":false,"measured_pairs":15,"temp_volume":"D:","seed_variant":"Baseline"})
    );
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut ratios = Vec::new();
    for round in 0..16 {
        let order = if round % 2 == 0 {
            [Variant::Baseline, Variant::Candidate]
        } else {
            [Variant::Candidate, Variant::Baseline]
        };
        let mut samples = Vec::new();
        for variant in order {
            assert_eq!(seed_hash, digest_file(&initial.db)?);
            let sample = run_parser(&runtime, parser, &initial.db, variant)?;
            if claude {
                let expected = if variant == Variant::Baseline {
                    "baseline"
                } else {
                    "adaptive"
                };
                assert!(
                    sample.records.iter().any(|r| r
                        .reset_algorithms
                        .get(expected)
                        .copied()
                        .unwrap_or_default()
                        > 0),
                    "actual reset function must match variant"
                );
            } else {
                assert!(sample.records.iter().all(|r| r.reset_algorithms.is_empty()));
            }
            assert_eq!(sample.stats.bytes_scanned, expected_bytes);
            assert_eq!(
                sample.stats.changed_files,
                if claude {
                    4
                } else if name == "codex_hot" {
                    0
                } else {
                    1
                }
            );
            assert_eq!(
                sample.stats.events_inserted,
                if claude {
                    1_025
                } else if name == "codex_hot" {
                    0
                } else {
                    25
                }
            );
            eprintln!(
                "{}",
                json!({"kind":"parser_sample","records":sample.records,"workload":name,"round":round,"warmup":round==0,"order":order,"variant":variant,"total_ns":sample.total_ns,"write_ns":sample.write_ns,"writer_begin_ns":sample.begin_ns,"writer_finish_ns":sample.finish_ns,"bytes_scanned":sample.stats.bytes_scanned,"changed_files":sample.stats.changed_files,"events_inserted":sample.stats.events_inserted})
            );
            samples.push(sample);
        }
        let (left, right) = if order[0] == Variant::Baseline {
            (&samples[0], &samples[1])
        } else {
            (&samples[1], &samples[0])
        };
        let equivalence = assert_database_equal(&left.db, &right.db)?;
        eprintln!(
            "{}",
            json!({"kind":"parser_equivalence","workload":name,"round":round,"equivalence":equivalence})
        );
        if round != 0 {
            a.push(left.total_ns);
            b.push(right.total_ns);
            ratios.push(right.total_ns as f64 / left.total_ns as f64);
        }
    }
    let da = distribution(a.into_iter());
    let db = distribution(b.into_iter());
    let ratio = db["median_ns"].as_f64().unwrap() / da["median_ns"].as_f64().unwrap();
    eprintln!(
        "{}",
        json!({"kind":"parser_summary","workload":name,"baseline_total":da,"candidate_total":db,"paired_total_ratios":ratios,"total_ratio":ratio,"passed":ratio<=1.10})
    );
    drop(runtime);
    drop(env);
    Ok(ratio <= 1.10)
}

#[test]
#[ignore = "single-thread release parser hot/append/project-replay A/B controls"]
fn writer_parser_ab_acceptance() -> Result<()> {
    if cfg!(debug_assertions) {
        anyhow::bail!("performance acceptance requires --release");
    }
    let mut passed = true;
    for name in ["codex_hot", "codex_append", "claude_project_replay"] {
        passed &= parser_control(name)?;
    }
    assert!(
        passed,
        "every parser total-time control must meet the ten percent bound"
    );
    Ok(())
}
