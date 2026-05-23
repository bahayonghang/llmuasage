use std::fmt::{Display, Formatter};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Supported local usage sources that `llmusage` can ingest.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// OpenAI Codex local rollout/session artifacts.
    Codex,
    /// Claude Code local project JSONL artifacts.
    Claude,
    /// OpenCode local SQLite usage database.
    Opencode,
    /// Google Antigravity / Gemini CLI local usage artifacts.
    #[value(alias = "antigravity")]
    Gemini,
}

impl SourceKind {
    /// Returns the stable lowercase identifier stored in SQLite and JSON payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Opencode => "opencode",
            Self::Gemini => "gemini",
        }
    }

    /// Parses the lowercase identifier produced by [`Self::as_str`].
    pub fn parse_id(value: &str) -> Option<Self> {
        match value {
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            "opencode" => Some(Self::Opencode),
            "gemini" | "antigravity" => Some(Self::Gemini),
            _ => None,
        }
    }
}

impl Display for SourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Token counters stored on usage events and aggregated buckets.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageTokens {
    /// Non-cached prompt/input tokens.
    #[serde(default)]
    pub input_tokens: i64,
    /// Cache-read prompt tokens, formerly persisted as `cached_input_tokens`.
    #[serde(default, alias = "cached_input_tokens")]
    pub cache_read_tokens: i64,
    /// Cache-creation prompt tokens. Non-Anthropic sources normally keep this at 0.
    #[serde(default)]
    pub cache_creation_tokens: i64,
    /// Output/completion tokens excluding reasoning-only fields.
    #[serde(default)]
    pub output_tokens: i64,
    /// Extra reasoning tokens reported separately by providers that expose them.
    #[serde(default)]
    pub reasoning_output_tokens: i64,
    /// Total tokens for the event or bucket after source-specific normalization.
    #[serde(default)]
    pub total_tokens: i64,
}

/// Optional conversation/session metadata attached to one normalized usage event.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionInfo {
    /// Stable session identifier from the source when available.
    pub session_id: String,
    /// Optional human-readable label, usually a transcript/file stem.
    pub session_label: Option<String>,
    /// Privacy-preserving hash of the source transcript or local DB session path.
    pub source_path_hash: Option<String>,
}

/// Stable, privacy-preserving project dimension values derived from a local path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Hash key used as the durable project identifier in SQLite tables.
    pub project_hash: String,
    /// Human-readable project label shown in dashboards and exports.
    pub project_label: String,
    /// Optional repo or project reference such as a remote URL.
    pub project_ref: Option<String>,
    /// Hash of the detected repository root, used for grouping sibling worktrees.
    pub repo_root_hash: String,
    /// Hash of the original local path to avoid storing raw filesystem locations.
    pub path_hash: String,
}

/// Canonical normalized usage event written to `usage_event`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    /// Stable idempotency key combining source-specific identity and event position.
    pub event_key: String,
    /// Source that produced the event.
    pub source: SourceKind,
    /// Normalized model name used for grouping and cost estimation.
    pub model: String,
    /// Source event timestamp in RFC 3339 format.
    pub event_at: String,
    /// 30-minute bucket start in RFC 3339 format.
    pub hour_start: String,
    /// Token usage associated with this event.
    pub tokens: UsageTokens,
    /// Optional project metadata resolved from the local working directory.
    pub project: Option<ProjectInfo>,
    /// Optional session metadata used by report-first commands.
    pub session: Option<SessionInfo>,
}

/// Deterministic activity bucket used by behavior-oriented dashboard views.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ActivityCategory {
    /// Code-writing or implementation turns.
    Coding,
    /// Failure diagnosis and debugging turns.
    Debugging,
    /// Feature discovery or product-shaping turns.
    Feature,
    /// Refactoring/cleanup turns.
    Refactoring,
    /// Test writing or verification turns.
    Testing,
    /// Read/search/navigation-heavy exploration turns.
    Exploration,
    /// Planning or task-organization turns.
    Planning,
    /// Sub-agent or delegation turns.
    Delegation,
    /// Git/history/release-control turns.
    Git,
    /// Build/deploy/packaging turns.
    BuildDeploy,
    /// Conversational turns without a stronger tool signal.
    Conversation,
    /// Brainstorming/ideation turns.
    Brainstorming,
    /// Fallback category when the source lacks enough behavior evidence.
    General,
}

impl ActivityCategory {
    /// Stable lowercase identifier stored in SQLite and JSON payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Debugging => "debugging",
            Self::Feature => "feature",
            Self::Refactoring => "refactoring",
            Self::Testing => "testing",
            Self::Exploration => "exploration",
            Self::Planning => "planning",
            Self::Delegation => "delegation",
            Self::Git => "git",
            Self::BuildDeploy => "build_deploy",
            Self::Conversation => "conversation",
            Self::Brainstorming => "brainstorming",
            Self::General => "general",
        }
    }

    /// Parses the lowercase identifier produced by [`Self::as_str`].
    pub fn parse_id(value: &str) -> Option<Self> {
        match value {
            "coding" => Some(Self::Coding),
            "debugging" => Some(Self::Debugging),
            "feature" => Some(Self::Feature),
            "refactoring" => Some(Self::Refactoring),
            "testing" => Some(Self::Testing),
            "exploration" => Some(Self::Exploration),
            "planning" => Some(Self::Planning),
            "delegation" => Some(Self::Delegation),
            "git" => Some(Self::Git),
            "build_deploy" => Some(Self::BuildDeploy),
            "conversation" => Some(Self::Conversation),
            "brainstorming" => Some(Self::Brainstorming),
            "general" => Some(Self::General),
            _ => None,
        }
    }
}

impl Display for ActivityCategory {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Coarse tool/action family persisted for behavior analytics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Built-in model/tool call such as Read, Edit, Grep, or Bash.
    Core,
    /// MCP server tool call.
    Mcp,
    /// Shell command execution.
    Bash,
    /// Skill invocation or skill-backed action.
    Skill,
    /// Sub-agent spawn or delegation action.
    Agent,
    /// Planning/task-list action.
    Planning,
    /// Read-like file/navigation action.
    Read,
    /// Write/edit-like action.
    Edit,
    /// Search-like action.
    Search,
    /// Fallback for source-specific tools that do not map cleanly yet.
    Other,
}

impl ToolKind {
    /// Stable lowercase identifier stored in SQLite and JSON payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Mcp => "mcp",
            Self::Bash => "bash",
            Self::Skill => "skill",
            Self::Agent => "agent",
            Self::Planning => "planning",
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Search => "search",
            Self::Other => "other",
        }
    }

    /// Parses the lowercase identifier produced by [`Self::as_str`].
    pub fn parse_id(value: &str) -> Option<Self> {
        match value {
            "core" => Some(Self::Core),
            "mcp" => Some(Self::Mcp),
            "bash" => Some(Self::Bash),
            "skill" => Some(Self::Skill),
            "agent" => Some(Self::Agent),
            "planning" => Some(Self::Planning),
            "read" => Some(Self::Read),
            "edit" => Some(Self::Edit),
            "search" => Some(Self::Search),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

impl Display for ToolKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Normalized turn-level behavior fact.
///
/// A turn aggregates one or more usage events without storing full prompt or
/// assistant text. Parsers may start with one event per turn and later merge
/// source-specific message groups as more evidence becomes available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageTurn {
    /// Stable idempotency key for the turn.
    pub turn_key: String,
    /// Source that produced the turn.
    pub source: SourceKind,
    /// Optional source session identifier.
    pub session_id: Option<String>,
    /// Privacy-preserving hash of the source transcript/file path.
    pub source_path_hash: Option<String>,
    /// Optional project hash dimension.
    pub project_hash: Option<String>,
    /// Primary model observed for this turn.
    pub primary_model: String,
    /// Turn start timestamp in RFC 3339 format.
    pub started_at: String,
    /// Deterministic activity category.
    pub category: ActivityCategory,
    /// Whether this turn performed an edit/write action.
    pub has_edits: bool,
    /// Deterministic retry count estimate for this turn.
    pub retries: i64,
    /// Whether this edit turn completed without a detected retry.
    pub one_shot: bool,
    /// Number of API calls/events represented by this turn.
    pub call_count: i64,
    /// Token usage attributed to this turn.
    pub tokens: UsageTokens,
}

impl UsageTurn {
    /// Creates a conservative one-event turn from an existing normalized event.
    pub fn from_event(event: &UsageEvent, category: ActivityCategory) -> Self {
        Self {
            turn_key: format!("turn:{}", event.event_key),
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
            primary_model: event.model.clone(),
            started_at: event.event_at.clone(),
            category,
            has_edits: false,
            retries: 0,
            one_shot: false,
            call_count: 1,
            tokens: event.tokens.clone(),
        }
    }
}

/// Normalized tool/action fact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageToolCall {
    /// Stable idempotency key for this action.
    pub tool_call_key: String,
    /// Optional parent turn key when known.
    pub turn_key: Option<String>,
    /// Optional parent usage event key when known.
    pub event_key: Option<String>,
    /// Source that produced the action.
    pub source: SourceKind,
    /// Optional source session identifier.
    pub session_id: Option<String>,
    /// Privacy-preserving hash of the source transcript/file path.
    pub source_path_hash: Option<String>,
    /// Optional project hash dimension.
    pub project_hash: Option<String>,
    /// Optional model associated with the action.
    pub model: Option<String>,
    /// Action timestamp in RFC 3339 format.
    pub occurred_at: String,
    /// Source tool name, e.g. `Read`, `Edit`, `Bash`, or an MCP tool name.
    pub tool_name: String,
    /// Coarse tool/action family.
    pub tool_kind: ToolKind,
    /// MCP server name when this is an MCP action.
    pub mcp_server: Option<String>,
    /// MCP tool name when this is an MCP action.
    pub mcp_tool: Option<String>,
    /// Privacy-preserving hash/fingerprint of the input when available.
    pub input_fingerprint: Option<String>,
    /// Short safe preview that must not contain full prompts or full file text.
    pub safe_preview: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_kind_antigravity_alias_keeps_stable_id() {
        let source = SourceKind::parse_id("antigravity").expect("alias should parse");
        assert_eq!(source, SourceKind::Gemini);
        assert_eq!(source.as_str(), "gemini");
    }
}
