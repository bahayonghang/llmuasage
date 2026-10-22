//! Private, language-neutral provenance collected alongside the public report.
use serde::{Deserialize, Serialize};

use super::UsageFetchReport;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) enum WindowHint {
    Session,
    Weekly,
    Opus,
    Frequent,
    Occasional,
    Seconds(i64),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) enum RemainingHint {
    Integer { remaining: i64, limit: i64 },
    Rounded { remaining: f64, limit: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct MetricHint {
    pub window: WindowHint,
    pub prefix: Option<String>,
    pub remaining: Option<RemainingHint>,
}

impl MetricHint {
    pub fn window(window: WindowHint) -> Self {
        Self {
            window,
            prefix: None,
            remaining: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) enum DiagnosticHint {
    MissingCredentials,
    MissingOauth,
    MissingToken,
    CredentialsModified,
    InvalidAuthObject,
    Unavailable,
    NoUsage,
    Http { status: u16 },
}

#[derive(Debug)]
struct PresentationError {
    canonical: String,
    hint: DiagnosticHint,
}
impl std::fmt::Display for PresentationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.canonical)
    }
}
impl std::error::Error for PresentationError {}

pub(super) fn error(canonical: impl Into<String>, hint: DiagnosticHint) -> anyhow::Error {
    PresentationError {
        canonical: canonical.into(),
        hint,
    }
    .into()
}

pub(super) fn diagnostic_hint(error: &anyhow::Error) -> Option<DiagnosticHint> {
    error
        .downcast_ref::<PresentationError>()
        .map(|error| error.hint.clone())
}

pub(super) type PresentedOutput = (super::UsageOutput, Vec<MetricHint>);

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuotaPresentationHints {
    fingerprint: Option<[u8; 32]>,
    pub metrics: Vec<Vec<MetricHint>>,
    pub diagnostics: Vec<Option<DiagnosticHint>>,
}

impl QuotaPresentationHints {
    pub fn bind(&mut self, report: &UsageFetchReport) {
        self.fingerprint = fingerprint(report);
    }

    pub fn matches(&self, report: &UsageFetchReport) -> bool {
        self.fingerprint.is_some()
            && self.fingerprint == fingerprint(report)
            && self.metrics.len() == report.outputs.len()
            && self.metrics.iter().zip(&report.outputs).all(|(hints, output)| {
                hints.len() == output.metrics.len() && hints.iter().all(|hint| (!matches!(hint.window, WindowHint::Seconds(seconds) if seconds <= 0)) && match hint.remaining {
                    Some(RemainingHint::Rounded { remaining, limit }) => remaining.is_finite() && limit.is_finite() && limit > 0.0,
                    Some(RemainingHint::Integer { limit, .. }) => limit > 0,
                    None => true,
                })
            })
            && self.diagnostics.len() == report.diagnostics.len()
            && self.diagnostics.iter().flatten().all(|hint| !matches!(hint, DiagnosticHint::Http { status } if !(100..=599).contains(status)))
    }
}

fn fingerprint(report: &UsageFetchReport) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    serde_json::to_vec(report)
        .ok()
        .map(|bytes| Sha256::digest(bytes).into())
}
