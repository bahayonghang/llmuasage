use std::{
    collections::HashMap,
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    time::Instant,
};

use anyhow::Result;
use serde_json::Value;
use tokio::task;
use tokio_util::sync::CancellationToken;
use tracing::info;
use walkdir::WalkDir;

use crate::{
    models::{ProjectInfo, SessionInfo, SourceKind, UsageEvent, UsageTokens},
    parsers::{
        ProgressSink, SourceParser, SourceSyncStats, SyncEvent,
        file_state::{
            CandidateFile, FileReplayMode, decide_file_replay, finalize_cursor, should_rescan_file,
        },
    },
    project::ProjectResolver,
    store::{Store, SyncRunWriter, SyncShard},
    util::{bucket_start_from_rfc3339, hash_string, normalize_model, resolve_home_dir},
};

/// Parses Gemini CLI local chat session JSON files
/// (`~/.gemini/tmp/<projectHash>/chats/session-*.json`).
///
/// Gemini sessions are single JSON documents (not JSONL), so this parser always
/// fully reparses a file once its size or mtime changes. The cursor is still
/// persisted via [`FileCursor`] so the M2 `source_file` three-state machine and
/// `--rebuild --source gemini` reset paths work the same as for Codex / Claude.
pub struct GeminiParser;

impl SourceParser for GeminiParser {
    fn source(&self) -> SourceKind {
        SourceKind::Gemini
    }

    fn parse<'a>(
        &'a self,
        store: &'a Store,
        writer: &'a mut SyncRunWriter,
        parallelism: usize,
        cancel: &'a CancellationToken,
        progress: Option<ProgressSink<'a>>,
    ) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>> {
        Box::pin(sync_gemini(store, writer, parallelism, cancel, progress))
    }
}

#[derive(Debug, Clone)]
struct GeminiShardPlan {
    files: Vec<CandidateFile>,
    project_lookup: HashMap<String, PathBuf>,
}

#[derive(Debug)]
struct GeminiShardOutput {
    events: Vec<UsageEvent>,
    cursors: Vec<crate::store::FileCursor>,
    reset_path_hashes: Vec<String>,
    events_seen: usize,
    events_replayed: usize,
    bytes_scanned: u64,
    seen_file_paths: Vec<String>,
}

#[derive(Debug)]
struct GeminiParseResult {
    end_offset: u64,
    events: Vec<UsageEvent>,
}

async fn sync_gemini(
    store: &Store,
    writer: &mut SyncRunWriter,
    parallelism: usize,
    cancel: &CancellationToken,
    mut progress: Option<ProgressSink<'_>>,
) -> Result<SourceSyncStats> {
    /*
     * ========================================================================
     * 步骤1：按 projectHash 目录分片并行解析 Gemini chat 会话真源
     * ========================================================================
     * 目标：
     * 1) 读取 ~/.gemini/tmp/<projectHash>/chats/session-*.json 文件
     * 2) 用 ~/.gemini/projects.json 反查 projectHash → cwd
     * 3) 返回 event / cursor / reset 指令给单 writer 统一落库
     */
    info!("开始同步 Gemini chat 会话真源");

    // 1.1 构建按 projectHash 分片的候选文件计划
    let parse_started = Instant::now();
    let home_dir = resolve_home_dir();
    let gemini_home = home_dir.join(".gemini");
    let tmp_dir = gemini_home.join("tmp");
    let project_lookup = load_projects_lookup(&gemini_home.join("projects.json"));
    let files = list_session_files(&tmp_dir);
    let total_files = files.len();
    emit_progress(
        &mut progress,
        SyncEvent::SourceStarted {
            source: SourceKind::Gemini,
            files_total: total_files as u64,
        },
    );
    let cursor_map = store.cursors().load_file_cursors(SourceKind::Gemini)?;

    let mut shards = std::collections::HashMap::<PathBuf, Vec<CandidateFile>>::new();
    let mut changed_files = 0usize;
    for file_path in files {
        let key = file_path
            .parent()
            .and_then(|chats| chats.parent())
            .unwrap_or(&tmp_dir)
            .to_path_buf();
        let existing = file_path
            .to_str()
            .and_then(|raw| cursor_map.get(raw).cloned());
        if should_rescan_file(&file_path, existing.as_ref())? {
            changed_files += 1;
            shards.entry(key).or_default().push(CandidateFile {
                path: file_path,
                existing,
            });
        }
    }

    // 1.2 控制并发度并行解析分片
    let mut events_seen = 0usize;
    let mut events_replayed = 0usize;
    let mut bytes_scanned = 0u64;
    let mut inserted = 0usize;
    let mut write_ms = 0u64;
    let mut files_scanned = 0usize;
    let mut plans = shards
        .into_values()
        .map(|files| GeminiShardPlan {
            files,
            project_lookup: project_lookup.clone(),
        })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.files.first().map(|file| file.path.clone()));

    let width = parallelism.max(1);
    for batch in plans.chunks(width) {
        if cancel.is_cancelled() {
            break;
        }
        let mut tasks = Vec::new();
        for plan in batch {
            let plan = plan.clone();
            tasks.push(task::spawn_blocking(move || parse_gemini_shard(plan)));
        }

        for task in tasks {
            if cancel.is_cancelled() {
                break;
            }
            let shard = task.await??;
            events_seen += shard.events_seen;
            events_replayed += shard.events_replayed;
            bytes_scanned += shard.bytes_scanned;

            // 1.3 把 reset / event / cursor 协议交给单写入端原子提交
            let commit = writer.commit_shard(SyncShard {
                source: SourceKind::Gemini,
                reset_path_hashes: shard.reset_path_hashes,
                events: shard.events,
                cursors: shard.cursors,
                seen_file_paths: shard.seen_file_paths,
                raw_records: Vec::new(),
            })?;
            files_scanned += commit.files_seen;
            inserted += commit.events_inserted;
            write_ms += commit.write_ms;
            emit_progress(
                &mut progress,
                SyncEvent::Progress {
                    source: SourceKind::Gemini,
                    files_scanned: files_scanned as u64,
                    records_imported: inserted as u64,
                    current_file: None,
                },
            );
        }
    }

    let mut stats = SourceSyncStats {
        source: SourceKind::Gemini,
        files_processed: total_files,
        changed_files,
        bytes_scanned,
        events_seen,
        events_replayed,
        events_inserted: inserted,
        write_ms,
        ..SourceSyncStats::default()
    };
    let total_elapsed = parse_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    stats.parse_ms = total_elapsed.saturating_sub(write_ms);

    info!(
        files_processed = stats.files_processed,
        changed_files = stats.changed_files,
        events_seen = stats.events_seen,
        bytes_scanned = stats.bytes_scanned,
        "完成 Gemini chat 会话真源解析"
    );
    Ok(stats)
}

fn emit_progress(sink: &mut Option<ProgressSink<'_>>, event: SyncEvent) {
    if let Some(sink) = sink.as_mut() {
        sink(event);
    }
}

fn list_session_files(tmp_dir: &Path) -> Vec<PathBuf> {
    let mut files = WalkDir::new(tmp_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            // session-*.json under .../<projectHash>/chats/
            let parent_is_chats = path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                == Some("chats");
            let is_session_json = path
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("session-") && value.ends_with(".json"))
                .unwrap_or(false);
            parent_is_chats && is_session_json
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// Reads `~/.gemini/projects.json` and returns `projectHash → cwd`.
///
/// Gemini CLI maps each project working directory to a privacy-preserving
/// hash that becomes the directory name under `~/.gemini/tmp/`. The hash
/// itself is opaque; the projects.json file is the only stable way to recover
/// the original cwd. Falls back to an empty map when the file is missing or
/// malformed (P4 — Gemini may never have run on this machine).
pub(crate) fn load_projects_lookup(projects_path: &Path) -> HashMap<String, PathBuf> {
    let raw = match fs::read_to_string(projects_path) {
        Ok(value) => value,
        Err(_) => return HashMap::new(),
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return HashMap::new();
    };

    let Some(object) = value.as_object() else {
        return HashMap::new();
    };

    let mut map = HashMap::new();
    for (key, entry) in object {
        if let Some(cwd) = extract_project_cwd(entry) {
            map.insert(key.clone(), PathBuf::from(cwd));
        }
    }
    map
}

fn extract_project_cwd(entry: &Value) -> Option<String> {
    if let Some(value) = entry.as_str() {
        return Some(value.to_string());
    }
    let object = entry.as_object()?;
    object
        .get("path")
        .or_else(|| object.get("cwd"))
        .or_else(|| object.get("workingDirectory"))
        .or_else(|| object.get("working_directory"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn parse_gemini_shard(plan: GeminiShardPlan) -> Result<GeminiShardOutput> {
    let mut resolver = ProjectResolver::default();
    let mut output = GeminiShardOutput {
        events: Vec::new(),
        cursors: Vec::new(),
        reset_path_hashes: Vec::new(),
        events_seen: 0,
        events_replayed: 0,
        bytes_scanned: 0,
        seen_file_paths: Vec::new(),
    };

    for candidate in plan.files {
        let existing = candidate.existing.clone();
        let decision = decide_file_replay(candidate)?;
        let path_text = decision.snapshot.path.to_string_lossy().to_string();
        output.seen_file_paths.push(path_text.clone());
        let path_hash = hash_string(&path_text);
        let project_hash_dir = decision
            .snapshot
            .path
            .parent()
            .and_then(|chats| chats.parent())
            .and_then(|dir| dir.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string);

        let project = resolve_project_from_hash(
            project_hash_dir.as_deref(),
            &plan.project_lookup,
            &mut resolver,
        )?;

        let parsed = parse_session_file(
            &decision.snapshot.path,
            &path_hash,
            &decision.snapshot.file_fingerprint,
            project,
        )?;

        output.bytes_scanned += decision.snapshot.file_size;
        output.events_seen += parsed.events.len();

        // Gemini sessions are single JSON documents — every refresh fully
        // replaces the prior event set for this path. Treat as Reparse so
        // commit_shard wipes the old rows before inserting the new ones.
        if existing.is_some() && decision.replay_mode == FileReplayMode::Reparse {
            output.events_replayed += parsed.events.len();
        }
        if existing.is_some() {
            output.reset_path_hashes.push(path_hash);
        }

        output.events.extend(parsed.events);
        output.cursors.push(finalize_cursor(
            &decision.snapshot.path,
            &decision.snapshot,
            parsed.end_offset,
            None,
            None,
        ));
    }

    Ok(output)
}

fn resolve_project_from_hash(
    project_hash_dir: Option<&str>,
    project_lookup: &HashMap<String, PathBuf>,
    resolver: &mut ProjectResolver,
) -> Result<Option<ProjectInfo>> {
    let Some(project_hash) = project_hash_dir else {
        return Ok(None);
    };

    if let Some(cwd) = project_lookup.get(project_hash) {
        if let Some(info) = resolver.resolve(cwd)? {
            return Ok(Some(info));
        }
        // Cwd known but not a git repo: synthesize a stable label so downstream
        // dashboards still show something meaningful.
        return Ok(Some(synthesize_project_info(project_hash, Some(cwd))));
    }

    // Fallback (P4): projects.json missing or out of date — still emit a stable
    // project so events do not collapse into the global `unknown-project` bucket.
    Ok(Some(synthesize_project_info(project_hash, None)))
}

fn synthesize_project_info(project_hash: &str, cwd: Option<&Path>) -> ProjectInfo {
    let project_label = match cwd {
        Some(path) => path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("<gemini:{project_hash}>")),
        None => format!("<gemini:{project_hash}>"),
    };
    let path_text = cwd
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| project_hash.to_string());
    let project_hash_value = hash_string(&path_text);
    ProjectInfo {
        project_hash: project_hash_value.clone(),
        project_label,
        project_ref: None,
        repo_root_hash: project_hash_value,
        path_hash: hash_string(&path_text),
    }
}

fn parse_session_file(
    file_path: &Path,
    path_hash: &str,
    file_fingerprint: &str,
    project: Option<ProjectInfo>,
) -> Result<GeminiParseResult> {
    let raw = fs::read(file_path)?;
    let end_offset = raw.len() as u64;
    let session_label = file_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_string);
    let fallback_session_id = session_label
        .clone()
        .unwrap_or_else(|| path_hash.to_string());

    let Ok(value) = serde_json::from_slice::<Value>(&raw) else {
        return Ok(GeminiParseResult {
            end_offset,
            events: Vec::new(),
        });
    };

    let messages = collect_messages(&value);
    let session_id = extract_session_id(&value).unwrap_or(fallback_session_id);
    let mut events = Vec::new();

    for (index, message) in messages.iter().enumerate() {
        let Some(timestamp) = extract_timestamp(message) else {
            continue;
        };
        let Some(hour_start) = bucket_start_from_rfc3339(&timestamp) else {
            continue;
        };
        let Some(usage) = extract_usage(message) else {
            continue;
        };
        let tokens = normalize_gemini_usage(usage);
        if tokens.total_tokens == 0
            && tokens.input_tokens == 0
            && tokens.output_tokens == 0
            && tokens.cache_read_tokens == 0
            && tokens.reasoning_output_tokens == 0
        {
            continue;
        }

        let model = normalize_model(extract_model(message, &value).as_deref());

        events.push(UsageEvent {
            event_key: format!("gemini:{path_hash}:{file_fingerprint}:{index}"),
            source: SourceKind::Gemini,
            model,
            event_at: timestamp,
            hour_start,
            tokens,
            project: project.clone(),
            session: Some(SessionInfo {
                session_id: session_id.clone(),
                session_label: session_label.clone(),
                source_path_hash: Some(path_hash.to_string()),
            }),
        });
    }

    Ok(GeminiParseResult { end_offset, events })
}

/// Walks the loaded session JSON and returns the message array.
///
/// Tolerates the two shapes documented in PRD §F1.1: a top-level array, or a
/// top-level object with a `messages` array. Other container keys are not
/// supported — if Gemini CLI changes layout, update PRD and this dispatcher.
fn collect_messages(value: &Value) -> Vec<Value> {
    if let Some(array) = value.as_array() {
        return array.clone();
    }
    value
        .as_object()
        .and_then(|object| object.get("messages"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn extract_session_id(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    object
        .get("sessionId")
        .or_else(|| object.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn extract_timestamp(message: &Value) -> Option<String> {
    let object = message.as_object()?;
    object
        .get("timestamp")
        .or_else(|| object.get("createdAt"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn extract_model(message: &Value, root: &Value) -> Option<String> {
    let from_message = message
        .as_object()
        .and_then(|object| object.get("model"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if from_message.is_some() {
        return from_message;
    }
    root.as_object()
        .and_then(|object| object.get("model"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Returns the usage payload sub-object.
///
/// PRD §F1.1 lists three accepted keys: `usageMetadata` (Google SDK shape),
/// `usage_metadata` (snake_case mirror), and `tokens` (Gemini CLI shape).
fn extract_usage(message: &Value) -> Option<&Value> {
    let object = message.as_object()?;
    object
        .get("usageMetadata")
        .or_else(|| object.get("usage_metadata"))
        .or_else(|| object.get("tokens"))
}

/// Normalizes Gemini token-count payloads into the shared [`UsageTokens`].
///
/// Two PRD-documented shapes are accepted:
/// 1. Google SDK / Gemini API: `promptTokenCount` / `candidatesTokenCount` /
///    `cachedContentTokenCount` / `thoughtsTokenCount` / `totalTokenCount`
/// 2. Gemini CLI: `{ input, output, cached, thoughts }`
///
/// Both keep promptTokenCount/input as the gross prompt total — Gemini's
/// promptTokenCount already includes cachedContentTokenCount per Google API
/// semantics, so this routine subtracts cached from input to keep the
/// dashboard `cache_efficiency = cache_read / (input + cache_read)` formula
/// consistent across sources (D8).
pub(crate) fn normalize_gemini_usage(value: &Value) -> UsageTokens {
    let prompt = value
        .get("promptTokenCount")
        .or_else(|| value.get("input"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let candidates = value
        .get("candidatesTokenCount")
        .or_else(|| value.get("output"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let cached = value
        .get("cachedContentTokenCount")
        .or_else(|| value.get("cached"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let reasoning = value
        .get("thoughtsTokenCount")
        .or_else(|| value.get("thoughts"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let total = value
        .get("totalTokenCount")
        .or_else(|| value.get("total"))
        .and_then(Value::as_i64)
        .unwrap_or(prompt + candidates + reasoning);

    let input_tokens = (prompt - cached).max(0);

    UsageTokens {
        input_tokens,
        cache_read_tokens: cached,
        cache_creation_tokens: 0,
        output_tokens: candidates,
        reasoning_output_tokens: reasoning,
        total_tokens: total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::TempDir;

    /// Validates F1.1: Gemini parser extracts `usageMetadata` plus the
    /// optional `thoughtsTokenCount` field, mapping cached content to
    /// `cache_read_tokens` and thoughts to `reasoning_output_tokens`.
    #[test]
    fn gemini_parser_extracts_tokens_and_thoughts() {
        let usage = json!({
            "promptTokenCount": 1000,
            "candidatesTokenCount": 200,
            "cachedContentTokenCount": 300,
            "thoughtsTokenCount": 50,
            "totalTokenCount": 1550,
        });
        let tokens = normalize_gemini_usage(&usage);
        // Input excludes cache so cache_efficiency math matches Anthropic semantics.
        assert_eq!(tokens.input_tokens, 700);
        assert_eq!(tokens.cache_read_tokens, 300);
        assert_eq!(tokens.cache_creation_tokens, 0);
        assert_eq!(tokens.output_tokens, 200);
        assert_eq!(tokens.reasoning_output_tokens, 50);
        assert_eq!(tokens.total_tokens, 1550);
    }

    /// Validates the Gemini CLI shape (`tokens.{input, output, cached,
    /// thoughts}`) per PRD §F1.1.
    #[test]
    fn gemini_parser_accepts_cli_payload() {
        let usage = json!({
            "input": 80,
            "output": 40,
            "cached": 20,
            "thoughts": 10,
        });
        let tokens = normalize_gemini_usage(&usage);
        assert_eq!(tokens.input_tokens, 60); // 80 - 20 cached
        assert_eq!(tokens.cache_read_tokens, 20);
        assert_eq!(tokens.output_tokens, 40);
        assert_eq!(tokens.reasoning_output_tokens, 10);
        // total fallback = prompt + candidates + reasoning (cached already in prompt)
        assert_eq!(tokens.total_tokens, 80 + 40 + 10);
    }

    /// Validates F1.1: `~/.gemini/projects.json` reverse-lookup turns a
    /// projectHash directory name into the original cwd, which is then handed
    /// to [`ProjectResolver`] (or the synthetic fallback for non-git cwds).
    #[test]
    fn gemini_project_hash_resolves_to_cwd() -> Result<()> {
        let temp = TempDir::new()?;
        let projects_path = temp.path().join("projects.json");
        let cwd_dir = temp.path().join("workspace").join("my-project");
        fs::create_dir_all(&cwd_dir)?;

        let mut file = fs::File::create(&projects_path)?;
        writeln!(
            file,
            r#"{{"deadbeef":{{"path":"{}"}},"cafef00d":"{}/other"}}"#,
            cwd_dir.display().to_string().replace('\\', "/"),
            temp.path().display().to_string().replace('\\', "/")
        )?;
        drop(file);

        let lookup = load_projects_lookup(&projects_path);
        assert_eq!(lookup.len(), 2);
        assert_eq!(
            lookup.get("deadbeef").map(PathBuf::as_path),
            Some(&*cwd_dir)
        );
        assert!(lookup.contains_key("cafef00d"));

        // Synthesized info is stable when projects.json maps to a non-git cwd.
        let mut resolver = ProjectResolver::default();
        let info =
            resolve_project_from_hash(Some("deadbeef"), &lookup, &mut resolver)?.expect("project");
        assert_eq!(info.project_label, "my-project");
        assert!(info.project_ref.is_none());

        // Missing projectHash falls back to the deterministic <gemini:hash> label.
        let info =
            resolve_project_from_hash(Some("missinghash"), &lookup, &mut resolver)?.expect("info");
        assert_eq!(info.project_label, "<gemini:missinghash>");
        Ok(())
    }

    /// Validates F1.1 end-to-end: a synthetic Gemini chat session JSON drives
    /// the shard parser through `parse_gemini_shard`, producing one event with
    /// the cached/output/thoughts split applied and the project resolved via
    /// the projects.json lookup. Pinned to the in-process API to avoid touching
    /// HOME / USERPROFILE during integration.
    #[test]
    fn gemini_shard_emits_event_with_resolved_project() -> Result<()> {
        let temp = TempDir::new()?;
        let project_hash = "feedface";
        let session_dir = temp.path().join("tmp").join(project_hash).join("chats");
        fs::create_dir_all(&session_dir)?;
        let session_path = session_dir.join("session-001.json");
        let session_body = json!({
            "sessionId": "abc-123",
            "model": "gemini-2.5-pro",
            "messages": [
                {
                    "role": "model",
                    "timestamp": "2026-05-08T10:00:00Z",
                    "usageMetadata": {
                        "promptTokenCount": 1000,
                        "candidatesTokenCount": 200,
                        "cachedContentTokenCount": 300,
                        "thoughtsTokenCount": 50,
                        "totalTokenCount": 1550,
                    }
                }
            ]
        });
        fs::write(&session_path, serde_json::to_vec_pretty(&session_body)?)?;

        let cwd_dir = temp.path().join("workspace").join("my-app");
        fs::create_dir_all(&cwd_dir)?;
        let mut lookup = HashMap::new();
        lookup.insert(project_hash.to_string(), cwd_dir.clone());

        let plan = GeminiShardPlan {
            files: vec![CandidateFile {
                path: session_path.clone(),
                existing: None,
            }],
            project_lookup: lookup,
        };
        let output = parse_gemini_shard(plan)?;

        assert_eq!(output.events.len(), 1);
        assert_eq!(output.seen_file_paths.len(), 1);
        let event = &output.events[0];
        assert_eq!(event.source, SourceKind::Gemini);
        assert_eq!(event.model, "gemini-2.5-pro");
        assert_eq!(event.tokens.input_tokens, 700);
        assert_eq!(event.tokens.cache_read_tokens, 300);
        assert_eq!(event.tokens.output_tokens, 200);
        assert_eq!(event.tokens.reasoning_output_tokens, 50);
        assert_eq!(event.tokens.total_tokens, 1550);
        let project = event.project.as_ref().expect("project resolved");
        assert_eq!(project.project_label, "my-app");
        let session = event.session.as_ref().expect("session attached");
        assert_eq!(session.session_id, "abc-123");
        Ok(())
    }
}
