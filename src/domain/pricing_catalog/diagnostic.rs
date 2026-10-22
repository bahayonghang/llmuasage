//! Private presentation facts carried alongside the unchanged public pricing error.
use crate::error::LlmusageError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DiagnosticFacts {
    SnapshotKind {},
    OverlayKind {},
    CompileKind {},
    UnknownRemoval {
        model: String,
    },
    CompiledMatcher {
        version: String,
        mode: String,
        source: String,
        matcher: String,
    },
    RootObject {},
    NoLiteRows {},
    LegacyMatcher {
        index: usize,
    },
    Schema {
        actual: u32,
        expected: u32,
    },
    EmptyVersion {},
    BaseRemovals {},
    DuplicateModel {
        model: String,
    },
    NoSources {
        model: String,
    },
    DuplicateSource {
        model: String,
        source: String,
    },
    NoMatchers {
        model: String,
    },
    EmptyMatcher {
        model: String,
    },
    DuplicateMatcher {
        model: String,
        mode: String,
        matcher: String,
    },
    TierName {
        model: String,
    },
    TierThreshold {
        model: String,
    },
    ContextWindow {
        model: String,
    },
    DuplicateRemoval {
        model: String,
    },
    Identifier {
        label: String,
        value: String,
    },
    InvalidRate {
        model: String,
        tier: String,
        channel: String,
    },
    Disagree {
        model: String,
    },
    MarkerEncode {
        detail: String,
    },
    MetadataIncomplete {
        layer: String,
    },
    Load {
        in_progress: bool,
        identity: String,
        reason: Box<LoadReason>,
    },
    RefreshIdentity {
        identity: String,
    },
    MarkerParse {
        detail: String,
    },
    MarkerMissingIdentity {},
    MarkerEmpty {},
    DigestCollision {
        path: String,
    },
    MetadataMismatch {
        overlay: bool,
        identity: String,
        declared: String,
    },
    OverlayNotAddressed {
        identity: String,
    },
    UnsafePath {
        path: String,
    },
    LocalOnly {
        snapshot: bool,
    },
    NotFile {
        snapshot: bool,
        path: String,
    },
    InvalidFilename {
        path: String,
    },
    DigestMismatch {
        path: String,
        expected: String,
        actual: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum LoadReason {
    Known { facts: DiagnosticFacts },
    Original { detail: String },
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) error: LlmusageError,
    pub(crate) facts: Option<Box<DiagnosticFacts>>,
}

pub(crate) type Result<T> = std::result::Result<T, Failure>;

impl Failure {
    pub(crate) fn config(detail: impl Into<String>, facts: DiagnosticFacts) -> Self {
        Self {
            error: LlmusageError::ConfigInvalid {
                detail: detail.into(),
            },
            facts: Some(Box::new(facts)),
        }
    }

    pub(crate) fn load(self, in_progress: bool, identity: &str) -> Self {
        let detail = self.error.to_string();
        let canonical = if in_progress {
            format!(
                "pricing recompute in-progress catalog `{identity}` could not be loaded: {detail}"
            )
        } else {
            format!("active pricing catalog `{identity}` could not be loaded: {detail}")
        };
        let reason = match self.facts {
            Some(facts) => LoadReason::Known { facts: *facts },
            None => LoadReason::Original { detail },
        };
        Self::config(
            canonical,
            DiagnosticFacts::Load {
                in_progress,
                identity: identity.into(),
                reason: Box::new(reason),
            },
        )
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
impl From<LlmusageError> for Failure {
    fn from(error: LlmusageError) -> Self {
        Self { error, facts: None }
    }
}
impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        LlmusageError::from(error).into()
    }
}
impl From<Failure> for LlmusageError {
    fn from(failure: Failure) -> Self {
        failure.error
    }
}
