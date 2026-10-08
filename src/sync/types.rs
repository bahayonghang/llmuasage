//! Core sync domain types shared between the sync engine and adapters.
//!
//! These types live here rather than in `commands::sync` so the application
//! layer (`sync::`) does not need to import CLI-adapter code — fixing the
//! ARCH-002 reverse dependency.

use std::{error::Error, fmt, future::Future, path::PathBuf, pin::Pin, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{models::SourceKind, parsers::SourceSyncStats};

pub const MAX_SYNC_PARALLELISM: usize = 32;
pub const MAX_RECENT_DAYS: u32 = 3650;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncRequestErrorCode {
    UnknownSource,
    InvalidParallelism,
    InvalidRecentDays,
}

impl SyncRequestErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownSource => "unknown_source",
            Self::InvalidParallelism => "invalid_parallelism",
            Self::InvalidRecentDays => "invalid_recent_days",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncRequestError {
    pub code: SyncRequestErrorCode,
    pub message: String,
}

impl fmt::Display for SyncRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl Error for SyncRequestError {}

/// Transport-shaped sync input shared by Web and the public Rust API.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SyncRequestInput {
    pub rebuild: bool,
    pub recent_days: Option<u32>,
    pub source: Option<String>,
    pub parallelism: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncSourceSelection {
    All,
    One(SourceKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedSyncRequest {
    rebuild: bool,
    source: SyncSourceSelection,
    recent_days: Option<u32>,
    parallelism: usize,
}

impl ValidatedSyncRequest {
    pub fn new(input: SyncRequestInput) -> Result<Self, SyncRequestError> {
        let source = match input.source {
            None => SyncSourceSelection::All,
            Some(source) => SourceKind::parse_id(&source).map_or_else(
                || {
                    Err(SyncRequestError {
                        code: SyncRequestErrorCode::UnknownSource,
                        message: format!("unknown source: {source}"),
                    })
                },
                |source| Ok(SyncSourceSelection::One(source)),
            )?,
        };
        if let Some(days) = input.recent_days
            && !(1..=MAX_RECENT_DAYS).contains(&days)
        {
            return Err(SyncRequestError {
                code: SyncRequestErrorCode::InvalidRecentDays,
                message: format!("recent_days must be between 1 and {MAX_RECENT_DAYS}, got {days}"),
            });
        }
        let parallelism = match input.parallelism {
            None => std::thread::available_parallelism()
                .map(|value| value.get().min(4))
                .unwrap_or(1),
            Some(value) if (1..=MAX_SYNC_PARALLELISM).contains(&value) => value,
            Some(value) => {
                return Err(SyncRequestError {
                    code: SyncRequestErrorCode::InvalidParallelism,
                    message: format!(
                        "parallelism must be between 1 and {MAX_SYNC_PARALLELISM}, got {value}"
                    ),
                });
            }
        };
        Ok(Self {
            rebuild: input.rebuild,
            source,
            recent_days: input.recent_days,
            parallelism,
        })
    }

    pub const fn rebuild(&self) -> bool {
        self.rebuild
    }

    pub const fn source(&self) -> SyncSourceSelection {
        self.source
    }

    pub const fn source_kind(&self) -> Option<SourceKind> {
        match self.source {
            SyncSourceSelection::All => None,
            SyncSourceSelection::One(source) => Some(source),
        }
    }

    pub const fn recent_days(&self) -> Option<u32> {
        self.recent_days
    }

    pub const fn parallelism(&self) -> usize {
        self.parallelism
    }

    pub fn recent_cutoff(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.recent_days
            .map(|days| now - Duration::days(i64::from(days)))
    }
}

/// Summary returned by a completed sync run.
#[derive(Debug, Clone)]
pub struct SyncSummary {
    pub sources: Vec<SourceSyncStats>,
    pub total_seen: usize,
    pub total_inserted: usize,
    pub stored_events: usize,
}

impl SyncSummary {
    /// Stable run-log summary shared by CLI and in-process JobRegistry syncs.
    pub fn summary_text(&self) -> String {
        format!(
            "sources={} seen={} inserted_delta={} stored_events={}",
            self.sources.len(),
            self.total_seen,
            self.total_inserted,
            self.stored_events
        )
    }
}

/// Decision returned by an interactive prompt for Antigravity recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityRecoveryChoice {
    Keep,
    AcceptLoss,
}

/// Callback invoked when an Antigravity product has recoverable loss and may be rebuilt.
pub type AntigravityPromptFn = Arc<
    dyn Fn(
            SourceKind,
            &crate::parsers::antigravity::AntigravityProductCoverage,
        ) -> AntigravityRecoveryChoice
        + Send
        + Sync,
>;

pub type PricingFetcherFn = Arc<
    dyn Fn(&str) -> Pin<Box<dyn Future<Output = std::result::Result<String, String>> + Send>>
        + Send
        + Sync,
>;

/// Options accepted by a sync run.
#[derive(Clone, Default)]
pub struct SyncRunOptions {
    pub rebuild: bool,
    pub source: Option<SourceKind>,
    pub recent_days: Option<u32>,
    pub parallelism: Option<usize>,
    pub provider_map: Option<PathBuf>,
    pub json_events: bool,
    pub allow_lossy_rebuild: bool,
    /// Human sync whose stdin, stdout, and stderr are all terminals and which
    /// is not `--json-events`. This stays set for `--recent-days`: that run
    /// must not read stdin, but its post-table notice still points at an
    /// unwindowed sync. A prompt callback is not this signal.
    pub interactive_terminal: bool,
    pub recovery_prompt: Option<AntigravityPromptFn>,
    pub pricing_fetcher: Option<PricingFetcherFn>,
}

impl fmt::Debug for SyncRunOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncRunOptions")
            .field("rebuild", &self.rebuild)
            .field("source", &self.source)
            .field("recent_days", &self.recent_days)
            .field("parallelism", &self.parallelism)
            .field("provider_map", &self.provider_map)
            .field("json_events", &self.json_events)
            .field("allow_lossy_rebuild", &self.allow_lossy_rebuild)
            .field("interactive_terminal", &self.interactive_terminal)
            .field(
                "recovery_prompt",
                &self.recovery_prompt.as_ref().map(|_| "<callback>"),
            )
            .field(
                "pricing_fetcher",
                &self.pricing_fetcher.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}
impl SyncRunOptions {
    pub fn validate(&self) -> Result<ValidatedSyncRequest, SyncRequestError> {
        ValidatedSyncRequest::new(SyncRequestInput {
            rebuild: self.rebuild,
            source: self.source.map(|source| source.as_str().to_string()),
            recent_days: self.recent_days,
            parallelism: self.parallelism,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_validator_rejects_each_invalid_input_with_stable_code() {
        let cases = [
            (
                SyncRequestInput {
                    source: Some("unknown".to_string()),
                    ..Default::default()
                },
                SyncRequestErrorCode::UnknownSource,
            ),
            (
                SyncRequestInput {
                    recent_days: Some(0),
                    ..Default::default()
                },
                SyncRequestErrorCode::InvalidRecentDays,
            ),
            (
                SyncRequestInput {
                    parallelism: Some(MAX_SYNC_PARALLELISM + 1),
                    ..Default::default()
                },
                SyncRequestErrorCode::InvalidParallelism,
            ),
            (
                SyncRequestInput {
                    parallelism: Some(0),
                    ..Default::default()
                },
                SyncRequestErrorCode::InvalidParallelism,
            ),
            (
                SyncRequestInput {
                    recent_days: Some(MAX_RECENT_DAYS + 1),
                    ..Default::default()
                },
                SyncRequestErrorCode::InvalidRecentDays,
            ),
        ];
        for (input, code) in cases {
            assert_eq!(ValidatedSyncRequest::new(input).unwrap_err().code, code);
        }
        let accepted = ValidatedSyncRequest::new(SyncRequestInput {
            recent_days: Some(MAX_RECENT_DAYS),
            ..Default::default()
        })
        .expect("recent_days=3650 is accepted");
        assert_eq!(accepted.recent_days(), Some(MAX_RECENT_DAYS));
    }
}
