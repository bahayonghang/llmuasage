//! Codex JSONL parser for extracting fine-grained usage events.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::models::CodexTracerEvent;

/// File parse state for incremental/resume parsing.
#[derive(Debug, Clone)]
pub struct FileParseState {
    /// Byte offset in the file where we left off
    pub byte_offset: u64,
    /// Line number where we left off
    pub line_number: i32,
    /// Last session ID encountered
    pub session_id: Option<String>,
    /// Last cumulative total to avoid duplicates
    pub last_cumulative_total: i64,
}

impl Default for FileParseState {
    fn default() -> Self {
        Self::new()
    }
}

impl FileParseState {
    /// Create a new initial parse state.
    pub fn new() -> Self {
        Self {
            byte_offset: 0,
            line_number: 0,
            session_id: None,
            last_cumulative_total: -1,
        }
    }
}

/// Parse a Codex JSONL file into tracer events.
pub fn parse_codex_jsonl_for_tracer(file_path: &Path) -> Result<Vec<CodexTracerEvent>> {
    parse_codex_jsonl_with_state(file_path, None).map(|(events, _state)| events)
}

/// Parse a Codex JSONL file with state tracking for incremental parsing.
///
/// Returns the parsed events and the final parse state for resuming.
pub fn parse_codex_jsonl_with_state(
    file_path: &Path,
    initial_state: Option<FileParseState>,
) -> Result<(Vec<CodexTracerEvent>, FileParseState)> {
    let file =
        File::open(file_path).with_context(|| format!("Failed to open {}", file_path.display()))?;
    let reader = BufReader::new(file);

    let mut events = Vec::new();
    let mut parser_state = ParserState::new();
    let source_file = file_path.to_string_lossy().to_string();

    // Load initial state if resuming
    let skip_lines = if let Some(ref state) = initial_state {
        parser_state.session_id = state.session_id.clone();
        parser_state.last_cumulative_total = state.last_cumulative_total;
        state.line_number as usize
    } else {
        0
    };

    let mut current_byte_offset = 0u64;

    for (line_number, line) in reader.lines().enumerate() {
        let line = line.context("Failed to read line")?;

        // Skip lines we've already processed
        if line_number < skip_lines {
            current_byte_offset += line.len() as u64 + 1; // +1 for newline
            continue;
        }

        let line_num = (line_number + 1) as i32;
        current_byte_offset += line.len() as u64 + 1; // +1 for newline

        let envelope: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue, // Skip invalid JSON
        };

        if let Some(event) = parse_envelope(
            &envelope,
            &mut parser_state,
            &source_file,
            line_num,
            file_path,
        )? {
            events.push(event);
        }
    }

    // Post-process: link previous/next and compute call indices
    link_previous_next_records(&mut events);

    // Create final parse state
    let final_state = FileParseState {
        byte_offset: current_byte_offset,
        line_number: events.last().map(|e| e.line_number).unwrap_or(0),
        session_id: parser_state.session_id.clone(),
        last_cumulative_total: parser_state.last_cumulative_total,
    };

    Ok((events, final_state))
}

/// Link previous/next records and compute thread call indices.
fn link_previous_next_records(events: &mut [CodexTracerEvent]) {
    // Group by thread_key
    let mut thread_groups: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, event) in events.iter().enumerate() {
        if let Some(thread_key) = &event.thread_key {
            thread_groups
                .entry(thread_key.clone())
                .or_default()
                .push(idx);
        }
    }

    // Process each thread
    for indices in thread_groups.values() {
        // Sort by event_timestamp within the thread
        let mut sorted_indices = indices.clone();
        sorted_indices.sort_by(|&a, &b| events[a].event_timestamp.cmp(&events[b].event_timestamp));

        // Link and assign call indices
        for (call_index, &idx) in sorted_indices.iter().enumerate() {
            events[idx].thread_call_index = Some(call_index as i32);

            // Link to previous
            if call_index > 0 {
                let prev_idx = sorted_indices[call_index - 1];
                events[idx].previous_record_id = Some(events[prev_idx].record_id.clone());
            }

            // Link to next
            if call_index < sorted_indices.len() - 1 {
                let next_idx = sorted_indices[call_index + 1];
                events[idx].next_record_id = Some(events[next_idx].record_id.clone());
            }
        }
    }
}

/// Parser state that persists across JSONL lines.
struct ParserState {
    session_id: Option<String>,
    session_meta: SessionMeta,
    current_turn: TurnContext,
    last_cumulative_total: i64,
}

impl ParserState {
    fn new() -> Self {
        Self {
            session_id: None,
            session_meta: SessionMeta::default(),
            current_turn: TurnContext::default(),
            last_cumulative_total: -1,
        }
    }
}

#[derive(Default)]
struct SessionMeta {
    thread_source: Option<String>,
    subagent_type: Option<String>,
    agent_role: Option<String>,
    agent_nickname: Option<String>,
    parent_session_id: Option<String>,
    parent_thread_name: Option<String>,
    parent_session_updated_at: Option<String>,
}

#[derive(Default)]
struct TurnContext {
    turn_id: Option<String>,
    turn_timestamp: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    current_date: Option<String>,
    timezone: Option<String>,
}

/// Parse a single JSONL envelope.
fn parse_envelope(
    envelope: &Value,
    state: &mut ParserState,
    source_file: &str,
    line_number: i32,
    file_path: &Path,
) -> Result<Option<CodexTracerEvent>> {
    let entry_type = envelope.get("type").and_then(|v| v.as_str());
    let timestamp = envelope
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let payload = match envelope.get("payload") {
        Some(p) if p.is_object() => p,
        _ => return Ok(None),
    };

    match entry_type {
        Some("session_meta") => {
            update_session_meta(payload, state);
            if state.session_id.is_none() {
                state.session_id = payload.get("id").and_then(|v| v.as_str()).map(String::from);
            }
            Ok(None)
        }
        Some("turn_context") => {
            update_turn_context(payload, state, timestamp);
            Ok(None)
        }
        Some("event_msg") => {
            let payload_type = payload.get("type").and_then(|v| v.as_str());
            if payload_type == Some("token_count") {
                parse_token_count(
                    envelope,
                    payload,
                    state,
                    source_file,
                    line_number,
                    file_path,
                )
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

/// Update session metadata from a session_meta event.
fn update_session_meta(payload: &Value, state: &mut ParserState) {
    state.session_meta.thread_source = payload
        .get("thread_source")
        .and_then(|v| v.as_str())
        .map(String::from);

    if let Some(subagent) = payload
        .get("source")
        .and_then(|v| v.as_object())
        .and_then(|source| source.get("subagent"))
        .and_then(|v| v.as_object())
    {
        // Check for "other" type
        if let Some(other) = subagent.get("other").and_then(|v| v.as_str()) {
            state.session_meta.subagent_type = Some(other.to_string());
        }

        // Check for thread_spawn
        if let Some(thread_spawn) = subagent.get("thread_spawn").and_then(|v| v.as_object()) {
            state.session_meta.subagent_type = Some("thread_spawn".to_string());
            state.session_meta.agent_role = thread_spawn
                .get("agent_role")
                .and_then(|v| v.as_str())
                .map(String::from);
            state.session_meta.agent_nickname = thread_spawn
                .get("agent_nickname")
                .and_then(|v| v.as_str())
                .map(String::from);
            state.session_meta.parent_session_id = thread_spawn
                .get("parent_thread_id")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
    }
}

/// Update turn context from a turn_context event.
fn update_turn_context(payload: &Value, state: &mut ParserState, timestamp: &str) {
    state.current_turn.turn_id = payload
        .get("turn_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    state.current_turn.turn_timestamp = Some(timestamp.to_string());
    state.current_turn.cwd = payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(String::from);
    state.current_turn.model = payload
        .get("model")
        .and_then(|v| v.as_str())
        .map(String::from);
    state.current_turn.effort = payload
        .get("effort")
        .and_then(|v| v.as_str())
        .map(String::from);
    state.current_turn.current_date = payload
        .get("current_date")
        .and_then(|v| v.as_str())
        .map(String::from);
    state.current_turn.timezone = payload
        .get("timezone")
        .and_then(|v| v.as_str())
        .map(String::from);
}

/// Parse a token_count event into a CodexTracerEvent.
fn parse_token_count(
    envelope: &Value,
    payload: &Value,
    state: &mut ParserState,
    source_file: &str,
    line_number: i32,
    file_path: &Path,
) -> Result<Option<CodexTracerEvent>> {
    let timestamp = envelope
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let info = match payload.get("info").and_then(|v| v.as_object()) {
        Some(i) => i,
        None => return Ok(None),
    };

    // Extract last_token_usage (current call)
    let last_usage = match info.get("last_token_usage").and_then(|v| v.as_object()) {
        Some(u) => u,
        None => return Ok(None),
    };

    // Extract total_token_usage (cumulative)
    let total_usage = match info.get("total_token_usage").and_then(|v| v.as_object()) {
        Some(u) => u,
        None => return Ok(None),
    };

    let input_tokens = get_i64(last_usage, "input_tokens")?;
    let cached_input_tokens = get_i64(last_usage, "cached_input_tokens")?;
    let output_tokens = get_i64(last_usage, "output_tokens")?;
    let reasoning_output_tokens = get_i64(last_usage, "reasoning_output_tokens")?;
    let total_tokens = get_i64(last_usage, "total_tokens")?;

    let cumulative_input_tokens = get_i64(total_usage, "input_tokens")?;
    let cumulative_cached_input_tokens = get_i64(total_usage, "cached_input_tokens")?;
    let cumulative_output_tokens = get_i64(total_usage, "output_tokens")?;
    let cumulative_reasoning_output_tokens = get_i64(total_usage, "reasoning_output_tokens")?;
    let cumulative_total_tokens = get_i64(total_usage, "total_tokens")?;

    // Skip duplicates (cumulative_total should increase)
    if cumulative_total_tokens <= state.last_cumulative_total {
        return Ok(None);
    }
    state.last_cumulative_total = cumulative_total_tokens;

    let session_id = state.session_id.clone().unwrap_or_else(|| {
        extract_session_id_from_path(file_path).unwrap_or_else(|| "unknown".to_string())
    });

    // Generate record_id
    let record_id = generate_record_id(
        &session_id,
        state.current_turn.turn_id.as_deref(),
        timestamp,
        cumulative_total_tokens,
        total_tokens,
    );

    // Create event
    let mut event = CodexTracerEvent::new(
        record_id,
        session_id.clone(),
        timestamp.to_string(),
        source_file.to_string(),
        line_number,
        input_tokens,
        cached_input_tokens,
        output_tokens,
        reasoning_output_tokens,
    );

    // Populate cumulative fields
    event.cumulative_input_tokens = cumulative_input_tokens;
    event.cumulative_cached_input_tokens = cumulative_cached_input_tokens;
    event.cumulative_output_tokens = cumulative_output_tokens;
    event.cumulative_reasoning_output_tokens = cumulative_reasoning_output_tokens;
    event.cumulative_total_tokens = cumulative_total_tokens;

    // Populate metadata from state
    event.turn_id = state.current_turn.turn_id.clone();
    event.turn_timestamp = state.current_turn.turn_timestamp.clone();
    event.cwd = state.current_turn.cwd.clone();
    event.model = state.current_turn.model.clone();
    event.effort = state.current_turn.effort.clone();
    event.current_date = state.current_turn.current_date.clone();
    event.timezone = state.current_turn.timezone.clone();

    // Session metadata
    event.thread_source = state.session_meta.thread_source.clone();
    event.subagent_type = state.session_meta.subagent_type.clone();
    event.agent_role = state.session_meta.agent_role.clone();
    event.agent_nickname = state.session_meta.agent_nickname.clone();
    event.parent_session_id = state.session_meta.parent_session_id.clone();
    event.parent_thread_name = state.session_meta.parent_thread_name.clone();
    event.parent_session_updated_at = state.session_meta.parent_session_updated_at.clone();

    // Check if archived
    event.is_archived = is_archived_path(file_path);

    // Model context window
    event.model_context_window = info
        .get("model_context_window")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);

    // Compute thread_key (simplified - just use session_id for now)
    event.thread_key = Some(compute_thread_key(&session_id, None));

    // Recompute derived fields
    event.recompute_derived_fields();

    Ok(Some(event))
}

/// Extract i64 from JSON object.
fn get_i64(obj: &serde_json::Map<String, Value>, key: &str) -> Result<i64> {
    obj.get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid field: {}", key))
}

/// Generate a record ID (SHA256 hash of key fields).
fn generate_record_id(
    session_id: &str,
    turn_id: Option<&str>,
    timestamp: &str,
    cumulative_total: i64,
    total: i64,
) -> String {
    let hash_input = format!(
        "{}:{}:{}:{}:{}",
        session_id,
        turn_id.unwrap_or(""),
        timestamp,
        cumulative_total,
        total
    );

    let mut hasher = Sha256::new();
    hasher.update(hash_input.as_bytes());
    let result = hasher.finalize();

    // Convert to hex and take first 16 characters
    let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
    hex[..16].to_string()
}

/// Compute thread key from session_id and optional thread_name.
fn compute_thread_key(session_id: &str, thread_name: Option<&str>) -> String {
    if let Some(name) = thread_name {
        let hash_input = format!("{}:{}", session_id, name);
        let mut hasher = Sha256::new();
        hasher.update(hash_input.as_bytes());
        let result = hasher.finalize();
        let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
        hex[..16].to_string()
    } else {
        session_id.to_string()
    }
}

/// Extract session ID from file path (e.g., rollout-...-{session-id}.jsonl).
fn extract_session_id_from_path(path: &Path) -> Option<String> {
    let filename = path.file_name()?.to_str()?;

    // Remove .jsonl extension first
    let name_without_ext = filename.strip_suffix(".jsonl")?;

    // Pattern: rollout-xyz-{8-4-4-4-12}.jsonl
    // The UUID is the last 36 characters before .jsonl
    if name_without_ext.len() >= 36 {
        let potential_uuid = &name_without_ext[name_without_ext.len() - 36..];

        // Validate UUID format (8-4-4-4-12)
        let parts: Vec<&str> = potential_uuid.split('-').collect();
        if parts.len() == 5
            && parts[0].len() == 8
            && parts[1].len() == 4
            && parts[2].len() == 4
            && parts[3].len() == 4
            && parts[4].len() == 12
            && parts[0].chars().all(|c| c.is_ascii_hexdigit())
            && parts[1].chars().all(|c| c.is_ascii_hexdigit())
            && parts[2].chars().all(|c| c.is_ascii_hexdigit())
            && parts[3].chars().all(|c| c.is_ascii_hexdigit())
            && parts[4].chars().all(|c| c.is_ascii_hexdigit())
        {
            return Some(potential_uuid.to_string());
        }
    }

    None
}

/// Check if path is in archived_sessions directory.
fn is_archived_path(path: &Path) -> bool {
    path.to_string_lossy().contains("archived_sessions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_record_id() {
        let id = generate_record_id(
            "session-123",
            Some("turn-456"),
            "2026-06-16T10:00:00Z",
            1000,
            200,
        );
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn test_compute_thread_key() {
        let key1 = compute_thread_key("session-123", None);
        assert_eq!(key1, "session-123");

        let key2 = compute_thread_key("session-123", Some("My Thread"));
        assert_eq!(key2.len(), 16);
        assert_ne!(key2, "session-123");
    }

    #[test]
    fn test_extract_session_id_from_path() {
        let path = Path::new("/path/rollout-xyz-12345678-1234-1234-1234-123456789abc.jsonl");
        let id = extract_session_id_from_path(path);
        assert_eq!(id, Some("12345678-1234-1234-1234-123456789abc".to_string()));
    }

    #[test]
    fn test_is_archived_path() {
        assert!(is_archived_path(Path::new(
            "/path/archived_sessions/file.jsonl"
        )));
        assert!(!is_archived_path(Path::new("/path/sessions/file.jsonl")));
    }

    #[test]
    fn test_link_previous_next_records() {
        let mut events = vec![
            CodexTracerEvent {
                record_id: "rec1".to_string(),
                session_id: "session1".to_string(),
                thread_key: Some("thread1".to_string()),
                event_timestamp: "2026-06-16T10:00:00Z".to_string(),
                thread_call_index: None,
                previous_record_id: None,
                next_record_id: None,
                ..CodexTracerEvent::new(
                    "rec1".to_string(),
                    "session1".to_string(),
                    "2026-06-16T10:00:00Z".to_string(),
                    "file.jsonl".to_string(),
                    1,
                    1000,
                    600,
                    200,
                    50,
                )
            },
            CodexTracerEvent {
                record_id: "rec2".to_string(),
                session_id: "session1".to_string(),
                thread_key: Some("thread1".to_string()),
                event_timestamp: "2026-06-16T11:00:00Z".to_string(),
                thread_call_index: None,
                previous_record_id: None,
                next_record_id: None,
                ..CodexTracerEvent::new(
                    "rec2".to_string(),
                    "session1".to_string(),
                    "2026-06-16T11:00:00Z".to_string(),
                    "file.jsonl".to_string(),
                    2,
                    2000,
                    1200,
                    300,
                    100,
                )
            },
        ];

        link_previous_next_records(&mut events);

        // Verify call indices
        assert_eq!(events[0].thread_call_index, Some(0));
        assert_eq!(events[1].thread_call_index, Some(1));

        // Verify previous/next links
        assert_eq!(events[0].previous_record_id, None);
        assert_eq!(events[0].next_record_id, Some("rec2".to_string()));

        assert_eq!(events[1].previous_record_id, Some("rec1".to_string()));
        assert_eq!(events[1].next_record_id, None);
    }

    #[test]
    fn test_file_parse_state() {
        let state = FileParseState::new();
        assert_eq!(state.byte_offset, 0);
        assert_eq!(state.line_number, 0);
        assert_eq!(state.session_id, None);
        assert_eq!(state.last_cumulative_total, -1);
    }
}
