//! Private neutral facts for existing safety refusals. Display stays canonical.
use serde::{Deserialize, Serialize};

use crate::models::SourceKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RebuildRisk {
    pub(crate) host_id: String,
    pub(crate) source: SourceKind,
    pub(crate) missing_files: u64,
    pub(crate) protected_events: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub(crate) enum FailureKind {
    LossyRebuild { risks: Vec<RebuildRisk> },
    OmpBeforePiSplit,
    UnsupportedRebuild { source: SourceKind },
}

impl FailureKind {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::LossyRebuild { .. } => "lossy_rebuild",
            Self::OmpBeforePiSplit => "omp_before_pi_split",
            Self::UnsupportedRebuild { .. } => "unsupported_rebuild",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{canonical}")]
pub(crate) struct SyncFailure {
    pub(crate) kind: FailureKind,
    canonical: String,
}

impl SyncFailure {
    pub(crate) fn new(kind: FailureKind, canonical: String) -> Self {
        Self { kind, canonical }
    }
}

/// Optional additive tracing fields. No locale or translated text crosses here.
pub(crate) fn log_facts(error: &anyhow::Error) -> Option<(&'static str, String)> {
    let failure = error.downcast_ref::<SyncFailure>()?;
    Some((
        failure.kind.code(),
        serde_json::to_string(&failure.kind).ok()?,
    ))
}
