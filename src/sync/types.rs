//! Core sync domain types shared between the sync engine and adapters.
//!
//! These types live here rather than in `commands::sync` so the application
//! layer (`sync::`) does not need to import CLI-adapter code — fixing the
//! ARCH-002 reverse dependency.

use std::{error::Error, fmt, path::PathBuf};

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

/// Options accepted by a sync run.
#[derive(Debug, Clone, Default)]
pub struct SyncRunOptions {
    pub rebuild: bool,
    pub source: Option<SourceKind>,
    pub recent_days: Option<u32>,
    pub parallelism: Option<usize>,
    pub provider_map: Option<PathBuf>,
    pub json_events: bool,
    pub allow_lossy_rebuild: bool,
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
        ];
        for (input, code) in cases {
            assert_eq!(ValidatedSyncRequest::new(input).unwrap_err().code, code);
        }
    }
}
