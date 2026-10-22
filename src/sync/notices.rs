//! Invocation feedback only. No locale, terminal I/O, storage or wire fields.
use crate::{
    domain::source_diagnostics::SourceIssue, models::SourceKind,
    parsers::antigravity::AntigravityProductCoverage,
};

#[derive(Debug, Clone)]
pub(crate) enum SourceFailure {
    SqliteOpen,
    Inventory {
        diagnostic: crate::parsers::feedback::InventoryDiagnostic,
    },
    Other {
        detail: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum RunNotice {
    LegacySkipped {
        source: SourceKind,
        retained: usize,
    },
    RecoveryOffered {
        source: SourceKind,
        coverage: AntigravityProductCoverage,
    },
    RecoveryChoice {
        source: SourceKind,
        accepted: bool,
    },
    Coverage {
        source: SourceKind,
        coverage: AntigravityProductCoverage,
        interactive: bool,
        windowed: bool,
        kept: bool,
    },
    RetainedHistory {
        source: SourceKind,
        retained: usize,
    },
    SourceIssue {
        source: SourceKind,
        issue: SourceIssue,
    },
    SourceFailed {
        source: SourceKind,
        failure: SourceFailure,
    },
    RemoteSkipped {
        host_id: String,
        label: String,
        detail: String,
        facts: Option<Box<crate::remote::diagnostic::DiagnosticFacts>>,
    },
    Cancelled,
}

impl RunNotice {
    pub(crate) fn source(&self) -> Option<SourceKind> {
        match self {
            Self::LegacySkipped { source, .. }
            | Self::RecoveryOffered { source, .. }
            | Self::RecoveryChoice { source, .. }
            | Self::Coverage { source, .. }
            | Self::RetainedHistory { source, .. }
            | Self::SourceIssue { source, .. }
            | Self::SourceFailed { source, .. } => Some(*source),
            Self::RemoteSkipped { .. } | Self::Cancelled => None,
        }
    }
}

pub(crate) type NoticeSink<'a> = &'a (dyn Fn(RunNotice) + Send + Sync);

pub(crate) fn emit(sink: Option<NoticeSink<'_>>, notice: RunNotice) {
    if let Some(sink) = sink {
        sink(notice);
    }
}
