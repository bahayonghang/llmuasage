use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::UsageFetchReport;

const CACHE_TTL_SECS: u64 = 300;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CacheDocument {
    fetched_at: u64,
    report: UsageFetchReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    presentation: Option<serde_json::Value>,
}

#[cfg(test)]
pub fn load(path: &Path) -> Option<UsageFetchReport> {
    load_with_presentation(path).map(|(report, _)| report)
}

pub(super) fn load_with_presentation(
    path: &Path,
) -> Option<(UsageFetchReport, super::QuotaPresentationHints)> {
    let content = std::fs::read_to_string(path).ok()?;
    let document: CacheDocument = serde_json::from_str(&content).ok()?;
    let now = unix_now();
    if now.saturating_sub(document.fetched_at) > CACHE_TTL_SECS {
        return None;
    }
    let hints = document
        .presentation
        .and_then(|value| serde_json::from_value::<super::QuotaPresentationHints>(value).ok())
        .filter(|hints| hints.matches(&document.report))
        .unwrap_or_default();
    Some((document.report, hints))
}

#[cfg(test)]
pub fn save(path: &Path, report: &UsageFetchReport) {
    save_with_presentation(path, report, &super::QuotaPresentationHints::default());
}

pub(super) fn save_with_presentation(
    path: &Path,
    report: &UsageFetchReport,
    hints: &super::QuotaPresentationHints,
) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let document = CacheDocument {
        fetched_at: unix_now(),
        report: report.clone(),
        presentation: hints
            .matches(report)
            .then(|| serde_json::to_value(hints).ok())
            .flatten(),
    };
    if let Ok(json) = serde_json::to_string(&document) {
        let _ = std::fs::write(path, json);
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::types::{UsageFetchDiagnostic, UsageOutput};

    #[test]
    fn expired_cache_is_ignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cache.json");
        let document = CacheDocument {
            fetched_at: 1,
            presentation: None,
            report: UsageFetchReport {
                outputs: vec![UsageOutput {
                    provider: "Claude".into(),
                    account: None,
                    credential_source: None,
                    plan: None,
                    email: None,
                    metrics: Vec::new(),
                }],
                diagnostics: vec![UsageFetchDiagnostic::error("Claude", "old")],
            },
        };
        std::fs::write(&path, serde_json::to_string(&document).expect("json")).expect("write");
        assert!(load(&path).is_none());
    }

    #[test]
    fn fresh_cache_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cache.json");
        let report = UsageFetchReport::default();
        save(&path, &report);
        assert_eq!(load(&path), Some(report));
    }
}
