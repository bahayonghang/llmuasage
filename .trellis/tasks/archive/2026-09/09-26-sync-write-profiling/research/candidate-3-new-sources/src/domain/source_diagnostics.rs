//! Source failures stored beside the public record-level diagnostic payload.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::models::{ParseIssues, SourceKind};

pub(crate) type SourceIssues = BTreeMap<SourceKind, Vec<SourceIssue>>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceIssueCode {
    DiscoveryIncomplete,
    FingerprintUnavailable,
    TrackedMemberMissing,
    TrackedMemberOutOfScope,
    TrackedMemberUnreadable,
    SnapshotChanged,
    MetadataUnreadable,
    IncompleteSnapshot,
}

impl SourceIssueCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::DiscoveryIncomplete => "discovery_incomplete",
            Self::FingerprintUnavailable => "fingerprint_unavailable",
            Self::TrackedMemberMissing => "tracked_member_missing",
            Self::TrackedMemberOutOfScope => "tracked_member_out_of_scope",
            Self::TrackedMemberUnreadable => "tracked_member_unreadable",
            Self::SnapshotChanged => "snapshot_changed",
            Self::MetadataUnreadable => "metadata_unreadable",
            Self::IncompleteSnapshot => "incomplete_snapshot",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceIssueScope {
    ProductGroup,
}

/// No path, record body, or free-text error enters the persisted source issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SourceIssue {
    pub(crate) code: SourceIssueCode,
    pub(crate) count: u64,
    pub(crate) observed_at: chrono::DateTime<chrono::Utc>,
    pub(crate) scope: SourceIssueScope,
}

impl SourceIssue {
    pub(crate) fn record(issues: &mut Vec<Self>, code: SourceIssueCode, count: u64) {
        if let Some(issue) = issues.iter_mut().find(|issue| issue.code == code) {
            issue.count = issue.count.saturating_add(count);
        } else {
            // One entry per closed-set code bounds the list to eight entries.
            issues.push(Self {
                code,
                count,
                observed_at: chrono::Utc::now(),
                scope: SourceIssueScope::ProductGroup,
            });
        }
    }

    pub(crate) fn cli_line(&self, source: SourceKind) -> String {
        let action = match self.code {
            SourceIssueCode::TrackedMemberMissing => format!(
                "restore missing databases, or explicitly accept history loss with `llmusage sync --rebuild --source {source} --allow-lossy-rebuild`"
            ),
            SourceIssueCode::TrackedMemberOutOfScope => format!(
                "tracked paths exist outside current discovery coverage; restore supported input coverage, or explicitly accept history loss with `llmusage sync --rebuild --source {source} --allow-lossy-rebuild`"
            ),
            SourceIssueCode::DiscoveryIncomplete
            | SourceIssueCode::FingerprintUnavailable
            | SourceIssueCode::TrackedMemberUnreadable => {
                "restore database access and retry sync".to_string()
            }
            SourceIssueCode::SnapshotChanged => {
                "retry sync after database writes finish".to_string()
            }
            SourceIssueCode::MetadataUnreadable | SourceIssueCode::IncompleteSnapshot => {
                "restore readable complete databases and retry sync".to_string()
            }
        };
        format!(
            "{} count={} scope=product_group observed_at={}; history preserved; group writes skipped; {action}",
            self.code.as_str(),
            self.count,
            self.observed_at.to_rfc3339()
        )
    }
}

/// Keeps the existing JSON shape and public Rust struct literals compatible.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct PersistedDiagnostics {
    #[serde(flatten)]
    pub(crate) parse_issues: ParseIssues,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) source_issues: Vec<SourceIssue>,
}

impl PersistedDiagnostics {
    pub(crate) fn has_source_errors(&self) -> bool {
        !self.source_issues.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_issues_legacy_json_and_source_failure_round_trip() {
        let old = r#"{"malformed_lines":0,"oversized_lines":0,"samples":[]}"#;
        let mut decoded: PersistedDiagnostics = serde_json::from_str(old).unwrap();
        assert!(!decoded.has_source_errors());
        SourceIssue::record(
            &mut decoded.source_issues,
            SourceIssueCode::TrackedMemberMissing,
            147,
        );
        assert_eq!(decoded.parse_issues.total(), 0);
        let json = serde_json::to_string(&decoded).unwrap();
        let copy: PersistedDiagnostics = serde_json::from_str(&json).unwrap();
        assert!(copy.has_source_errors());
        assert_eq!(copy.source_issues, decoded.source_issues);
        let public: ParseIssues = serde_json::from_str(&json).unwrap();
        assert_eq!(public, ParseIssues::default());
    }
}
