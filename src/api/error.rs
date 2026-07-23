use thiserror::Error;

/// Public error contract for llmusage library APIs.
///
/// The enum is intentionally coarse-grained so downstream adapters such as
/// ccr-ui can branch on stable failure families without depending on internal
/// implementation details. It is non-exhaustive to allow new 0.5.x variants
/// without forcing downstream matches to change.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum LlmusageError {
    /// Filesystem access failed while reading/writing local-only runtime data.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// SQLite returned an error while opening, migrating, or querying the store.
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    /// The runtime store has not been initialized yet.
    #[error("not initialized — run `llmusage init`")]
    NotInitialized,
    /// A schema migration failed and its transaction was rolled back.
    #[error("migration {version} ({name}) failed: {source}")]
    MigrationFailed {
        /// Migration version that failed.
        version: u32,
        /// Human-readable migration name.
        name: &'static str,
        /// Original failure cause.
        #[source]
        source: anyhow::Error,
    },
    /// A textual payload (JSON/TOML/JSONL/etc.) failed to parse.
    ///
    /// Carries the structural source so downstream adapters can branch on the
    /// underlying serde error type, plus a `context` snippet identifying which
    /// document or field bucket failed.
    #[error("parse {context}: {source}")]
    Parse {
        /// Short human-facing identifier for what was being parsed
        /// (e.g. `"pricing snapshot"`, `"antigravity hook payload"`).
        context: &'static str,
        /// Original parse failure cause.
        #[source]
        source: serde_json::Error,
    },
    /// The global sync worker lock could not be acquired before the requested
    /// timeout (5.4 / D13). Surfaces enough metadata for a caller to render
    /// "another worker is holding the lock since X" without re-querying the
    /// store.
    #[error("worker lock busy: {holder}")]
    LockBusy {
        /// Stable identifier for the current holder, typically
        /// `kind:pid@acquired_at`. Empty when the holder row has been evicted
        /// between the busy decision and the response.
        holder: String,
    },
    /// User-supplied configuration (CLI flag combination, settings JSON,
    /// integration target file, …) was syntactically valid but semantically
    /// rejected before any side effect ran. Always recoverable by adjusting
    /// the input.
    #[error("invalid config: {detail}")]
    ConfigInvalid {
        /// Human-readable explanation of what was rejected and how to fix it.
        detail: String,
    },
    /// Work was intentionally stopped because its caller timed out or went away.
    #[error("cancelled: {operation}")]
    Cancelled {
        /// Stable operation label for diagnostics and structured logs.
        operation: &'static str,
    },
    /// Pricing data is unavailable for `(source, model)` while a caller has
    /// asked for a strict cost calculation. Soft callers (dashboard fallback)
    /// continue to use [`crate::query::PricingStatus::Unpriced`]; this variant
    /// is for callers that want to bail out rather than silently zero-fill.
    #[error("pricing missing for {source_id}:{model}")]
    PricingMissing {
        /// Stable source identifier such as `codex`, `kimi_code`, or `pi`.
        ///
        /// Named `source_id` rather than `source` because thiserror treats a
        /// field literally named `source` as the error chain link.
        source_id: String,
        /// Normalized model name as written to `usage_event.model`.
        model: String,
    },
}

/// Convenient result alias for public llmusage APIs.
pub type Result<T, E = LlmusageError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_variant_from_std_io_error() {
        let err = std::fs::read_to_string("definitely-missing-llmusage-file")
            .map_err(LlmusageError::from)
            .expect_err("missing file should produce io error");
        assert!(matches!(err, LlmusageError::Io(_)));
    }

    #[test]
    fn db_variant_from_rusqlite_error() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory sqlite");
        let err = conn
            .execute("SELECT * FROM missing_table", [])
            .map(|_| ())
            .map_err(LlmusageError::from)
            .expect_err("bad query should produce db error");
        assert!(matches!(err, LlmusageError::Db(_)));
    }

    #[test]
    fn not_initialized_variant_has_actionable_message() {
        let err = LlmusageError::NotInitialized;
        assert!(err.to_string().contains("llmusage init"));
    }

    #[test]
    fn migration_failed_variant_preserves_version_and_source() {
        let err = LlmusageError::MigrationFailed {
            version: 7,
            name: "test",
            source: anyhow::anyhow!("boom"),
        };
        assert!(matches!(
            err,
            LlmusageError::MigrationFailed { version: 7, .. }
        ));
        assert!(err.to_string().contains("migration 7"));
    }

    #[test]
    fn parse_variant_carries_context_and_serde_source() {
        let serde_err = serde_json::from_str::<serde_json::Value>("{")
            .expect_err("malformed JSON should fail to parse");
        let err = LlmusageError::Parse {
            context: "pricing snapshot",
            source: serde_err,
        };
        assert!(err.to_string().contains("parse pricing snapshot"));
    }

    #[test]
    fn lock_busy_variant_renders_holder() {
        let err = LlmusageError::LockBusy {
            holder: "cli:1234@2026-05-08T10:00:00Z".to_string(),
        };
        assert!(err.to_string().contains("cli:1234"));
        assert!(err.to_string().contains("worker lock busy"));
    }

    #[test]
    fn config_invalid_variant_includes_detail() {
        let err = LlmusageError::ConfigInvalid {
            detail: "missing required `--source` flag".to_string(),
        };
        assert!(err.to_string().contains("missing required"));
    }

    #[test]
    fn cancelled_variant_identifies_operation() {
        let err = LlmusageError::Cancelled {
            operation: "dashboard query",
        };
        assert_eq!(err.to_string(), "cancelled: dashboard query");
    }

    #[test]
    fn pricing_missing_variant_identifies_source_and_model() {
        let err = LlmusageError::PricingMissing {
            source_id: "antigravity".to_string(),
            model: "gemini-2.5-pro".to_string(),
        };
        let rendered = err.to_string();
        assert!(rendered.contains("antigravity:gemini-2.5-pro"));
    }
}
