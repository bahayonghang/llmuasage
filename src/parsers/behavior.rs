use serde_json::Value;

use crate::{
    models::{ActivityCategory, ToolKind, UsageEvent, UsageToolCall, UsageTurn},
    util::hash_string,
};

const SAFE_PREVIEW_CHARS: usize = 120;

/// Lightweight behavior evidence collected while parsing one source event.
///
/// Parsers use this as a privacy-preserving bridge from provider-specific JSON
/// to normalized turn/tool facts. It intentionally carries only tool names,
/// coarse kinds, and bounded previews/fingerprints.
#[derive(Debug, Clone)]
pub(crate) struct BehaviorToolEvidence {
    pub(crate) sequence: usize,
    pub(crate) tool_name: String,
    pub(crate) tool_kind: ToolKind,
    pub(crate) mcp_server: Option<String>,
    pub(crate) mcp_tool: Option<String>,
    pub(crate) input_fingerprint: Option<String>,
    pub(crate) safe_preview: Option<String>,
}

/// Builds a turn from an event and the tools observed around that event.
pub(crate) fn turn_from_tools(event: &UsageEvent, tools: &[BehaviorToolEvidence]) -> UsageTurn {
    let has_edits = tools.iter().any(|tool| tool.tool_kind == ToolKind::Edit);
    UsageTurn {
        category: classify_tools(tools),
        has_edits,
        one_shot: has_edits,
        ..UsageTurn::from_event(event, ActivityCategory::General)
    }
}

/// Converts provider-local behavior evidence into persisted `usage_tool_call`
/// facts parented to the conservative one-event turn.
pub(crate) fn tool_calls_from_evidence(
    event: &UsageEvent,
    tools: Vec<BehaviorToolEvidence>,
) -> Vec<UsageToolCall> {
    tools
        .into_iter()
        .map(|tool| UsageToolCall {
            tool_call_key: format!(
                "tool:{}:{}:{}",
                event.source.as_str(),
                event.event_key,
                tool.sequence
            ),
            turn_key: Some(format!("turn:{}", event.event_key)),
            event_key: Some(event.event_key.clone()),
            source: event.source,
            session_id: event
                .session
                .as_ref()
                .map(|session| session.session_id.clone()),
            source_path_hash: event
                .session
                .as_ref()
                .and_then(|session| session.source_path_hash.clone()),
            project_hash: event
                .project
                .as_ref()
                .map(|project| project.project_hash.clone()),
            model: Some(event.model.clone()),
            occurred_at: event.event_at.clone(),
            tool_name: tool.tool_name,
            tool_kind: tool.tool_kind,
            mcp_server: tool.mcp_server,
            mcp_tool: tool.mcp_tool,
            input_fingerprint: tool.input_fingerprint,
            safe_preview: tool.safe_preview,
        })
        .collect()
}

/// Extracts Claude Code `message.content[].type == "tool_use"` blocks.
pub(crate) fn extract_claude_tools(value: &Value) -> Vec<BehaviorToolEvidence> {
    let content = value
        .get("message")
        .and_then(|message| message.get("content"))
        .or_else(|| value.get("content"));
    let Some(items) = content.and_then(Value::as_array) else {
        return Vec::new();
    };

    items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|item| {
            let tool_name = item.get("name").and_then(Value::as_str)?.trim();
            if tool_name.is_empty() {
                return None;
            }
            let input = item.get("input");
            // The `Skill` tool carries the concrete skill in input.skill; resolve it
            // so behavior/zero-call analysis sees the real name, not literal "Skill".
            if tool_name.eq_ignore_ascii_case("skill") {
                let skill_name = input
                    .and_then(|input| input.get("skill"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(tool_name);
                Some(skill_evidence(skill_name, input))
            } else {
                Some(tool_evidence(tool_name, input, None))
            }
        })
        .enumerate()
        .map(|(index, mut tool)| {
            tool.sequence = index;
            tool
        })
        .collect()
}

/// Extracts OpenAI Codex response-item function calls from rollout JSONL rows.
///
/// The rollout format has evolved; this deliberately recognizes only stable
/// function-call shaped records and ignores free-form messages.
pub(crate) fn extract_codex_tools(value: &Value) -> Vec<BehaviorToolEvidence> {
    let mut tools = Vec::new();
    collect_codex_function_calls(value, &mut tools);
    for (index, tool) in tools.iter_mut().enumerate() {
        tool.sequence = index;
    }
    tools
}

fn collect_codex_function_calls(value: &Value, out: &mut Vec<BehaviorToolEvidence>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_codex_function_calls(item, out);
            }
        }
        Value::Object(map) => {
            let type_name = map.get("type").and_then(Value::as_str);
            let name = map
                .get("name")
                .or_else(|| map.get("recipient_name"))
                .or_else(|| map.get("tool_name"))
                .and_then(Value::as_str);
            let looks_like_call = matches!(
                type_name,
                Some("function_call" | "tool_call" | "custom_tool_call")
            ) || map.contains_key("recipient_name");

            if looks_like_call
                && let Some(tool_name) = name.map(str::trim).filter(|value| !value.is_empty())
            {
                let input = map
                    .get("arguments")
                    .or_else(|| map.get("input"))
                    .or_else(|| map.get("params"));
                out.push(tool_evidence(
                    tool_name,
                    input,
                    Some(type_name.unwrap_or("tool_call")),
                ));
                return;
            }

            for child in map.values() {
                collect_codex_function_calls(child, out);
            }
        }
        _ => {}
    }
}

/// Builds tool evidence from one OpenCode `part` row whose `type == "tool"`.
///
/// OpenCode names tools differently from Claude/Codex: built-ins are bare words
/// (`read`, `bash`, `glob`), skills surface as `tool == "skill"` with the name in
/// `state.input.name`, and MCP tools use a single-underscore `<server>_<tool>`
/// shape (no `mcp__` prefix). Returns `None` for non-tool or unnamed parts.
pub(crate) fn opencode_tool_evidence(part: &Value) -> Option<BehaviorToolEvidence> {
    if part.get("type").and_then(Value::as_str) != Some("tool") {
        return None;
    }
    let tool = part
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let input = part.get("state").and_then(|state| state.get("input"));

    let (display_name, tool_kind, mcp_server, mcp_tool) = if tool.eq_ignore_ascii_case("skill") {
        // Skill name lives in state.input.name; fall back to the literal tool.
        let skill_name = input
            .and_then(|input| input.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(tool);
        (skill_name.to_string(), ToolKind::Skill, None, None)
    } else if is_opencode_builtin(tool) {
        // Built-ins map cleanly through the shared classifier (read->Read,
        // bash->Bash, glob/grep->Search, todowrite->Planning, task->Agent, ...).
        (
            tool.to_string(),
            classify_tool(tool, None, None),
            None,
            None,
        )
    } else if let Some((server, mcp_tool)) = split_opencode_mcp(tool) {
        // Heuristic: a non-builtin name containing `_` is an MCP `<server>_<tool>`.
        // Known-server longest-prefix matching (needs opencode.json) is left to the
        // inventory work; first-underscore split can mis-split a server whose own
        // name contains `_` (documented known limitation).
        (
            tool.to_string(),
            ToolKind::Mcp,
            Some(server),
            Some(mcp_tool),
        )
    } else {
        (tool.to_string(), ToolKind::Core, None, None)
    };

    Some(BehaviorToolEvidence {
        sequence: 0,
        tool_name: normalize_tool_name(&display_name),
        tool_kind,
        mcp_server,
        mcp_tool,
        input_fingerprint: input.map(input_fingerprint),
        safe_preview: safe_tool_preview(tool, input),
    })
}

/// OpenCode built-in tool names (bare words, no server prefix).
fn is_opencode_builtin(tool: &str) -> bool {
    const BUILTINS: &[&str] = &[
        "read",
        "write",
        "edit",
        "multiedit",
        "bash",
        "glob",
        "grep",
        "list",
        "webfetch",
        "patch",
        "task",
        "question",
        "todowrite",
        "todoread",
        "invalid",
    ];
    BUILTINS.contains(&tool.to_ascii_lowercase().as_str())
}

/// Splits an OpenCode MCP tool name `<server>_<tool>` at the first underscore.
fn split_opencode_mcp(tool: &str) -> Option<(String, String)> {
    let (server, rest) = tool.split_once('_')?;
    let server = normalize_tool_name(server);
    let mcp_tool = normalize_tool_name(rest);
    (!server.is_empty() && !mcp_tool.is_empty()).then_some((server, mcp_tool))
}

/// Builds evidence for a skill call with the concrete skill name and a forced
/// `Skill` kind. Used by Claude (`input.skill`) and OpenCode (`state.input.name`).
fn skill_evidence(skill_name: &str, input: Option<&Value>) -> BehaviorToolEvidence {
    BehaviorToolEvidence {
        sequence: 0,
        tool_name: normalize_tool_name(skill_name),
        tool_kind: ToolKind::Skill,
        mcp_server: None,
        mcp_tool: None,
        input_fingerprint: input.map(input_fingerprint),
        safe_preview: safe_tool_preview("skill", input),
    }
}

fn tool_evidence(
    tool_name: &str,
    input: Option<&Value>,
    source_hint: Option<&str>,
) -> BehaviorToolEvidence {
    let (mcp_server, mcp_tool) = split_mcp_tool(tool_name);
    let tool_kind = classify_tool(tool_name, mcp_server.as_deref(), source_hint);
    let preview = safe_tool_preview(tool_name, input);
    let fingerprint = input.map(input_fingerprint);
    BehaviorToolEvidence {
        sequence: 0,
        tool_name: normalize_tool_name(tool_name),
        tool_kind,
        mcp_server,
        mcp_tool,
        input_fingerprint: fingerprint,
        safe_preview: preview,
    }
}

fn classify_tools(tools: &[BehaviorToolEvidence]) -> ActivityCategory {
    if tools.iter().any(|tool| tool.tool_kind == ToolKind::Agent) {
        return ActivityCategory::Delegation;
    }
    if tools
        .iter()
        .any(|tool| tool.tool_kind == ToolKind::Planning)
    {
        return ActivityCategory::Planning;
    }
    if tools.iter().any(|tool| {
        tool.tool_kind == ToolKind::Edit || matches_tool_name(&tool.tool_name, &["write", "edit"])
    }) {
        return ActivityCategory::Coding;
    }
    if tools.iter().any(|tool| {
        tool.tool_kind == ToolKind::Bash
            && matches_tool_name(
                &tool.safe_preview.clone().unwrap_or_default(),
                &[
                    "test",
                    "cargo test",
                    "pytest",
                    "npm test",
                    "pnpm test",
                    "bun test",
                ],
            )
    }) {
        return ActivityCategory::Testing;
    }
    if tools
        .iter()
        .any(|tool| tool.tool_kind == ToolKind::Search || tool.tool_kind == ToolKind::Read)
    {
        return ActivityCategory::Exploration;
    }
    ActivityCategory::General
}

fn classify_tool(tool_name: &str, mcp_server: Option<&str>, source_hint: Option<&str>) -> ToolKind {
    let normalized = tool_name.to_ascii_lowercase();
    if mcp_server.is_some() || normalized.starts_with("mcp__") {
        return ToolKind::Mcp;
    }
    if normalized == "bash"
        || normalized.ends_with(".shell_command")
        || normalized.contains("shell")
    {
        return ToolKind::Bash;
    }
    if matches_tool_name(&normalized, &["edit", "multiedit", "write", "apply_patch"]) {
        return ToolKind::Edit;
    }
    if matches_tool_name(&normalized, &["read", "view_image", "open"]) {
        return ToolKind::Read;
    }
    if matches_tool_name(&normalized, &["grep", "glob", "find", "search"]) {
        return ToolKind::Search;
    }
    if matches_tool_name(&normalized, &["todowrite", "todo", "plan", "update_plan"]) {
        return ToolKind::Planning;
    }
    if matches_tool_name(&normalized, &["task", "spawn_agent", "agent"]) {
        return ToolKind::Agent;
    }
    if normalized.contains("skill") {
        return ToolKind::Skill;
    }
    if source_hint == Some("custom_tool_call") {
        return ToolKind::Core;
    }
    ToolKind::Core
}

fn matches_tool_name(value: &str, needles: &[&str]) -> bool {
    let value = value.to_ascii_lowercase();
    needles.iter().any(|needle| value.contains(needle))
}

fn split_mcp_tool(tool_name: &str) -> (Option<String>, Option<String>) {
    let normalized = tool_name.trim();
    let Some(rest) = normalized.strip_prefix("mcp__") else {
        return (None, None);
    };
    let mut parts = rest.splitn(2, "__");
    let server = parts
        .next()
        .map(normalize_tool_name)
        .filter(|value| !value.is_empty());
    let tool = parts
        .next()
        .map(normalize_tool_name)
        .filter(|value| !value.is_empty());
    (server, tool)
}

fn normalize_tool_name(value: &str) -> String {
    value.trim().chars().take(96).collect()
}

fn input_fingerprint(value: &Value) -> String {
    hash_string(&serde_json::to_string(value).unwrap_or_else(|_| value.to_string()))
}

fn safe_tool_preview(tool_name: &str, input: Option<&Value>) -> Option<String> {
    let input = input?;
    let mut parts = Vec::new();
    let normalized = tool_name.to_ascii_lowercase();
    if normalized == "bash" {
        push_string_field(&mut parts, "cmd", input, "command");
    } else {
        for key in [
            "file_path",
            "path",
            "pattern",
            "query",
            "cmd",
            "command",
            "description",
        ] {
            push_string_field(&mut parts, key, input, key);
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(truncate_preview(&parts.join(" · ")))
    }
}

fn push_string_field(parts: &mut Vec<String>, label: &str, input: &Value, key: &str) {
    if let Some(value) = input.get(key).and_then(Value::as_str) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            parts.push(format!("{label}: {}", truncate_preview(trimmed)));
        }
    }
}

fn truncate_preview(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars().take(SAFE_PREVIEW_CHARS) {
        if ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        extract_claude_tools, extract_codex_tools, opencode_tool_evidence, turn_from_tools,
    };
    use crate::models::{ActivityCategory, SourceKind, ToolKind, UsageEvent, UsageTokens};

    fn event() -> UsageEvent {
        UsageEvent {
            event_key: "codex:path:offset".to_string(),
            source: SourceKind::Codex,
            provider_label: String::new(),
            model: "gpt-5".to_string(),
            event_at: "2026-05-01T00:00:00Z".to_string(),
            hour_start: "2026-05-01T00:00:00Z".to_string(),
            tokens: UsageTokens {
                total_tokens: 1,
                ..UsageTokens::default()
            },
            project: None,
            session: None,
        }
    }

    #[test]
    fn claude_tool_extraction_keeps_safe_metadata_only() {
        let value = json!({
            "message": {
                "content": [
                    {"type":"text","text":"ignored"},
                    {"type":"tool_use","name":"Read","input":{"file_path":"src/lib.rs"}},
                    {"type":"tool_use","name":"Bash","input":{"command":"cargo test -- --test-threads=1"}},
                    {"type":"tool_use","name":"mcp__context7__resolve-library-id","input":{"libraryName":"rusqlite"}}
                ]
            }
        });

        let tools = extract_claude_tools(&value);

        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].tool_name, "Read");
        assert_eq!(tools[0].tool_kind, ToolKind::Read);
        assert!(
            tools[0]
                .safe_preview
                .as_deref()
                .unwrap()
                .contains("src/lib.rs")
        );
        assert_eq!(tools[1].tool_kind, ToolKind::Bash);
        assert_eq!(tools[2].tool_kind, ToolKind::Mcp);
        assert_eq!(tools[2].mcp_server.as_deref(), Some("context7"));
        assert_eq!(tools[2].mcp_tool.as_deref(), Some("resolve-library-id"));
        assert!(tools.iter().all(|tool| tool.input_fingerprint.is_some()));
    }

    #[test]
    fn claude_skill_resolves_concrete_name_from_input() {
        let tools = extract_claude_tools(&json!({
            "message": { "content": [
                {"type":"tool_use","name":"Skill","input":{"skill":"smart-search","args":"x"}}
            ]}
        }));
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_kind, ToolKind::Skill);
        // Concrete skill name resolved from input.skill, not literal "Skill".
        assert_eq!(tools[0].tool_name, "smart-search");
    }

    #[test]
    fn codex_tool_extraction_recognizes_response_item_function_call() {
        let value = json!({
            "type": "response_item",
            "payload": {
                "item": {
                    "type": "function_call",
                    "name": "functions.shell_command",
                    "arguments": {"command":"cargo check"}
                }
            }
        });

        let tools = extract_codex_tools(&value);

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_name, "functions.shell_command");
        assert_eq!(tools[0].tool_kind, ToolKind::Bash);
        assert!(
            tools[0]
                .safe_preview
                .as_deref()
                .unwrap()
                .contains("cargo check")
        );
    }

    #[test]
    fn turn_classification_is_tool_first() {
        let tools = extract_claude_tools(&json!({
            "message": {
                "content": [
                    {"type":"tool_use","name":"Edit","input":{"file_path":"src/lib.rs"}},
                    {"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}
                ]
            }
        }));

        let turn = turn_from_tools(&event(), &tools);

        assert_eq!(turn.category, ActivityCategory::Coding);
        assert!(turn.has_edits);
        assert!(turn.one_shot);
    }

    #[test]
    fn opencode_tool_evidence_classifies_builtin_skill_and_mcp() {
        let read = opencode_tool_evidence(&json!({
            "type": "tool",
            "tool": "read",
            "state": { "status": "completed", "input": { "file_path": "src/lib.rs" } }
        }))
        .expect("read evidence");
        assert_eq!(read.tool_name, "read");
        assert_eq!(read.tool_kind, ToolKind::Read);
        assert!(read.safe_preview.as_deref().unwrap().contains("src/lib.rs"));

        let skill = opencode_tool_evidence(&json!({
            "type": "tool",
            "tool": "skill",
            "state": { "input": { "name": "smart-search" } }
        }))
        .expect("skill evidence");
        assert_eq!(skill.tool_kind, ToolKind::Skill);
        assert_eq!(skill.tool_name, "smart-search");

        let mcp = opencode_tool_evidence(&json!({
            "type": "tool",
            "tool": "context7_query-docs",
            "state": { "status": "completed" }
        }))
        .expect("mcp evidence");
        assert_eq!(mcp.tool_kind, ToolKind::Mcp);
        assert_eq!(mcp.mcp_server.as_deref(), Some("context7"));
        assert_eq!(mcp.mcp_tool.as_deref(), Some("query-docs"));

        // Non-tool parts and unnamed tools are ignored.
        assert!(opencode_tool_evidence(&json!({ "type": "text", "text": "hi" })).is_none());
        assert!(opencode_tool_evidence(&json!({ "type": "tool", "tool": "" })).is_none());
    }
}
