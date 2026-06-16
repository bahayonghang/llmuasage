//! Codex-specific extended usage event models for codex-tracer.

use serde::{Deserialize, Serialize};

/// Codex-specific extended usage event for detailed tracking.
///
/// This model includes fine-grained token accounting (cached/uncached split),
/// thread tracking, cumulative fields, and computed ratios that are specific
/// to Codex usage analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexTracerEvent {
    // === Primary Keys ===
    /// Unique identifier for this record (hash of source_file + line_number)
    pub record_id: String,

    // === Session/Thread Metadata ===
    /// Codex session UUID
    pub session_id: String,
    /// User-visible thread name (e.g., "Implement feature X")
    pub thread_name: Option<String>,
    /// Session last updated timestamp (RFC 3339)
    pub session_updated_at: Option<String>,
    /// Thread key for grouping related calls
    pub thread_key: Option<String>,
    /// Sequential index within the thread (0-based)
    pub thread_call_index: Option<i32>,
    /// Link to previous call in the same thread
    pub previous_record_id: Option<String>,
    /// Link to next call in the same thread
    pub next_record_id: Option<String>,
    /// Thread source classification (e.g., "user", "auto-review", "subagent")
    pub thread_source: Option<String>,

    // === Event Metadata ===
    /// Event timestamp (RFC 3339)
    pub event_timestamp: String,
    /// Source JSONL file path
    pub source_file: String,
    /// Line number in the source file
    pub line_number: i32,
    /// Turn ID (if available)
    pub turn_id: Option<String>,
    /// Turn timestamp (RFC 3339, if available)
    pub turn_timestamp: Option<String>,
    /// Working directory at the time of the call
    pub cwd: Option<String>,
    /// Current date as seen by the model
    pub current_date: Option<String>,
    /// Timezone
    pub timezone: Option<String>,
    /// Whether this session is archived
    pub is_archived: bool,

    // === Model Information ===
    /// Model name (e.g., "gpt-4", "o1-preview")
    pub model: Option<String>,
    /// Reasoning effort level (e.g., "low", "medium", "high")
    pub effort: Option<String>,
    /// Model context window size
    pub model_context_window: Option<i32>,

    // === Call Origin Classification ===
    /// Call initiator category (e.g., "user-turn", "auto-review", "subagent")
    pub call_initiator: Option<String>,
    /// Reason for the classification
    pub call_initiator_reason: Option<String>,
    /// Confidence level of the classification
    pub call_initiator_confidence: Option<String>,

    // === Subagent Information ===
    /// Subagent type (if this is a subagent call)
    pub subagent_type: Option<String>,
    /// Agent role
    pub agent_role: Option<String>,
    /// Agent nickname
    pub agent_nickname: Option<String>,
    /// Parent session ID (for subagent calls)
    pub parent_session_id: Option<String>,
    /// Parent thread name
    pub parent_thread_name: Option<String>,
    /// Parent session updated timestamp
    pub parent_session_updated_at: Option<String>,

    // === Token Accounting (Fine-grained) ===
    /// Total input/prompt tokens
    pub input_tokens: i64,
    /// Cached input tokens (read from cache)
    pub cached_input_tokens: i64,
    /// Uncached input tokens (computed: input - cached)
    pub uncached_input_tokens: i64,
    /// Output/completion tokens (excluding reasoning)
    pub output_tokens: i64,
    /// Reasoning output tokens
    pub reasoning_output_tokens: i64,
    /// Total tokens for this call
    pub total_tokens: i64,

    // === Cumulative Token Accounting ===
    /// Cumulative input tokens (session-wide)
    pub cumulative_input_tokens: i64,
    /// Cumulative cached input tokens
    pub cumulative_cached_input_tokens: i64,
    /// Cumulative output tokens
    pub cumulative_output_tokens: i64,
    /// Cumulative reasoning output tokens
    pub cumulative_reasoning_output_tokens: i64,
    /// Cumulative total tokens
    pub cumulative_total_tokens: i64,

    // === Computed Fields ===
    /// Cache hit ratio (cached / input)
    pub cache_ratio: f64,
    /// Reasoning output ratio (reasoning / output)
    pub reasoning_output_ratio: f64,
    /// Context window usage percentage
    pub context_window_percent: f64,
}

impl CodexTracerEvent {
    /// Create a new event with computed fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        record_id: String,
        session_id: String,
        event_timestamp: String,
        source_file: String,
        line_number: i32,
        input_tokens: i64,
        cached_input_tokens: i64,
        output_tokens: i64,
        reasoning_output_tokens: i64,
    ) -> Self {
        let uncached_input_tokens = input_tokens.saturating_sub(cached_input_tokens).max(0);
        let total_tokens = input_tokens + output_tokens;

        let cache_ratio = if input_tokens > 0 {
            (cached_input_tokens as f64) / (input_tokens as f64)
        } else {
            0.0
        };

        let reasoning_output_ratio = if output_tokens > 0 {
            (reasoning_output_tokens as f64) / (output_tokens as f64)
        } else {
            0.0
        };

        Self {
            record_id,
            session_id,
            thread_name: None,
            session_updated_at: None,
            thread_key: None,
            thread_call_index: None,
            previous_record_id: None,
            next_record_id: None,
            thread_source: None,
            event_timestamp,
            source_file,
            line_number,
            turn_id: None,
            turn_timestamp: None,
            cwd: None,
            current_date: None,
            timezone: None,
            is_archived: false,
            model: None,
            effort: None,
            model_context_window: None,
            call_initiator: None,
            call_initiator_reason: None,
            call_initiator_confidence: None,
            subagent_type: None,
            agent_role: None,
            agent_nickname: None,
            parent_session_id: None,
            parent_thread_name: None,
            parent_session_updated_at: None,
            input_tokens,
            cached_input_tokens,
            uncached_input_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
            cumulative_input_tokens: input_tokens,
            cumulative_cached_input_tokens: cached_input_tokens,
            cumulative_output_tokens: output_tokens,
            cumulative_reasoning_output_tokens: reasoning_output_tokens,
            cumulative_total_tokens: total_tokens,
            cache_ratio,
            reasoning_output_ratio,
            context_window_percent: 0.0,
        }
    }

    /// Recompute derived fields (uncached, ratios, context %).
    pub fn recompute_derived_fields(&mut self) {
        self.uncached_input_tokens = self
            .input_tokens
            .saturating_sub(self.cached_input_tokens)
            .max(0);

        self.cache_ratio = if self.input_tokens > 0 {
            (self.cached_input_tokens as f64) / (self.input_tokens as f64)
        } else {
            0.0
        };

        self.reasoning_output_ratio = if self.output_tokens > 0 {
            (self.reasoning_output_tokens as f64) / (self.output_tokens as f64)
        } else {
            0.0
        };

        self.context_window_percent = if let Some(window) = self.model_context_window {
            if window > 0 {
                (self.input_tokens as f64) / (window as f64)
            } else {
                0.0
            }
        } else {
            0.0
        };
    }
}

/// Thread summary for aggregating calls within a thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    /// Thread key
    pub thread_key: String,
    /// First record ID in the thread
    pub first_record_id: Option<String>,
    /// Last record ID in the thread
    pub last_record_id: Option<String>,
    /// Number of calls in the thread
    pub call_count: i32,
    /// Total tokens across all calls
    pub total_tokens_sum: i64,
    /// Estimated cost sum (if pricing is available)
    pub estimated_cost_sum: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codex_tracer_event_computed_fields() {
        let event = CodexTracerEvent::new(
            "test-record-1".to_string(),
            "session-123".to_string(),
            "2026-06-16T10:00:00Z".to_string(),
            "/path/to/file.jsonl".to_string(),
            1,
            1000, // input
            600,  // cached
            200,  // output
            50,   // reasoning
        );

        assert_eq!(event.uncached_input_tokens, 400); // 1000 - 600
        assert_eq!(event.total_tokens, 1200); // 1000 + 200
        assert!((event.cache_ratio - 0.6).abs() < 0.001); // 600 / 1000
        assert!((event.reasoning_output_ratio - 0.25).abs() < 0.001); // 50 / 200
    }

    #[test]
    fn test_recompute_derived_fields() {
        let mut event = CodexTracerEvent::new(
            "test-record-2".to_string(),
            "session-456".to_string(),
            "2026-06-16T11:00:00Z".to_string(),
            "/path/to/file.jsonl".to_string(),
            2,
            2000, // input
            1500, // cached
            300,  // output
            100,  // reasoning
        );

        // Modify token values
        event.input_tokens = 3000;
        event.cached_input_tokens = 2000;
        event.model_context_window = Some(128000);

        // Recompute
        event.recompute_derived_fields();

        assert_eq!(event.uncached_input_tokens, 1000); // 3000 - 2000
        assert!((event.cache_ratio - 0.6667).abs() < 0.01); // 2000 / 3000
        assert!((event.context_window_percent - 0.0234).abs() < 0.01); // 3000 / 128000
    }

    #[test]
    fn test_zero_token_edge_cases() {
        let event = CodexTracerEvent::new(
            "test-record-3".to_string(),
            "session-789".to_string(),
            "2026-06-16T12:00:00Z".to_string(),
            "/path/to/file.jsonl".to_string(),
            3,
            0, // input
            0, // cached
            0, // output
            0, // reasoning
        );

        assert_eq!(event.uncached_input_tokens, 0);
        assert_eq!(event.cache_ratio, 0.0);
        assert_eq!(event.reasoning_output_ratio, 0.0);
        assert_eq!(event.context_window_percent, 0.0);
    }
}
