use std::fmt::{Display, Formatter};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Supported local usage sources that `llmusage` can ingest.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// OpenAI Codex local rollout/session artifacts.
    Codex,
    /// Claude Code local project JSONL artifacts.
    Claude,
    /// OpenCode local SQLite usage database.
    Opencode,
}

impl SourceKind {
    /// Returns the stable lowercase identifier stored in SQLite and JSON payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Opencode => "opencode",
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
    /// Non-cached input/prompt tokens.
    pub input_tokens: i64,
    /// Cached or reused input tokens billed separately by some providers.
    pub cached_input_tokens: i64,
    /// Output/completion tokens excluding reasoning-only fields.
    pub output_tokens: i64,
    /// Extra reasoning tokens reported separately by providers that expose them.
    pub reasoning_output_tokens: i64,
    /// Total tokens for the event or bucket after source-specific normalization.
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
