use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;

use crate::{
    app::AppContext,
    domain::{
        platform_monitor::{self, ParserSupportStatus, PlatformProbe},
        source_descriptor::{SourceDescriptor, UsageQuality},
    },
    models::{ParseIssues, SourceKind},
    query::{Dashboard, QueryFilter, SourceBreakdown},
    registry,
    store::{Host, Store},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceCapabilityStatus {
    pub source: SourceKind,
    pub stable_id: &'static str,
    pub display_name: &'static str,
    pub status: &'static str,
    pub quality: &'static str,
    pub total_tokens: i64,
    pub last_event_at: Option<String>,
    pub token_accounting_version: Option<u32>,
    pub legacy_token_accounting: bool,
    pub token_accounting_warning: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlatformMonitorStatus {
    pub platform_id: &'static str,
    pub display_name: &'static str,
    pub source: Option<SourceKind>,
    pub probe_status: &'static str,
    pub parser_status: &'static str,
    pub quality: Option<&'static str>,
    pub privacy: &'static str,
    pub roots_checked: usize,
    pub roots_detected: usize,
    pub artifact_patterns: &'static [&'static str],
    pub detail: String,
    pub next_action: &'static str,
}

pub async fn run(app: &AppContext) -> Result<()> {
    let store = Store::new(&app.paths)?;
    store.require_initialized()?;
    let dashboard = Dashboard::open(&store)?;
    let platform_statuses = build_platform_monitor_statuses();
    let hosts = store.hosts().list()?;

    println!("Source status:");
    for host in &hosts {
        println!(
            "Host {}: status={}",
            host.label,
            host_lifecycle_status(host)
        );
        let sources = dashboard.source_breakdown(&QueryFilter {
            host_id: Some(host.host_id.clone()),
            ..Default::default()
        })?;
        let mut capability_statuses = build_source_capability_statuses(&sources);
        apply_token_accounting_statuses(&store, &mut capability_statuses)?;
        let parse_issues = store
            .sync_status()
            .load_source_sync_statuses(&host.host_id)?
            .into_iter()
            .map(|status| (status.source, status.parse_issues))
            .collect::<BTreeMap<_, _>>();
        print_human_statuses(&capability_statuses, &[], &parse_issues);
    }
    print_human_statuses(&[], &platform_statuses, &BTreeMap::new());
    Ok(())
}

pub fn build_source_capability_statuses(
    sources: &[SourceBreakdown],
) -> Vec<SourceCapabilityStatus> {
    let usage = sources
        .iter()
        .filter_map(|source| {
            SourceKind::parse_id(&source.source).map(|kind| (kind, source.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    registry::registered_source_descriptors()
        .iter()
        .map(|descriptor| {
            let source_usage = usage.get(&descriptor.kind);
            source_status_from_parts(descriptor, source_usage)
        })
        .collect()
}

pub fn build_platform_monitor_statuses() -> Vec<PlatformMonitorStatus> {
    platform_monitor::probe_registered_platforms()
        .into_iter()
        .map(platform_monitor_status_from_probe)
        .collect()
}

pub fn apply_token_accounting_statuses(
    store: &Store,
    statuses: &mut [SourceCapabilityStatus],
) -> Result<()> {
    for status in statuses {
        if !registry::source_descriptor(status.source)
            .is_some_and(|descriptor| descriptor.capabilities.parser)
        {
            continue;
        }
        status.token_accounting_version = store.token_accounting_version(status.source)?;
        status.legacy_token_accounting = store.has_legacy_token_accounting(status.source)?;
        if status.legacy_token_accounting {
            status.token_accounting_warning = Some(
                crate::store::SyncStatusStore::legacy_repair_warning(status.source),
            );
        }
    }
    Ok(())
}

pub fn print_human_statuses(
    capability_statuses: &[SourceCapabilityStatus],
    platform_statuses: &[PlatformMonitorStatus],
    parse_issues_by_source: &BTreeMap<String, ParseIssues>,
) {
    for status in capability_statuses {
        println!(
            "- Source status {}: status={} quality={} total={} last={} accounting={} ({})",
            status.source,
            status.status,
            status.quality,
            status.total_tokens,
            status.last_event_at.as_deref().unwrap_or("never"),
            if status.legacy_token_accounting {
                "legacy"
            } else if status.token_accounting_version.is_some() {
                "current"
            } else {
                "unversioned"
            },
            status.display_name
        );
        if let Some(warning) = &status.token_accounting_warning {
            println!("  warning: {warning}");
        }
        if let Some(issues) = parse_issues_by_source.get(status.source.as_str()) {
            for line in parse_issue_status_lines(issues) {
                println!("{line}");
            }
        }
    }
    for platform in platform_statuses {
        println!(
            "- Platform monitor {}: status={} parser={} quality={} roots={}/{} privacy={} ({}) next={}",
            platform.platform_id,
            platform.probe_status,
            platform.parser_status,
            platform.quality.unwrap_or("unavailable"),
            platform.roots_detected,
            platform.roots_checked,
            platform.privacy,
            platform.display_name,
            platform.next_action
        );
    }
}

/// Read-only host lifecycle for `source-status`. `live` is a sync-event
/// signal only and is never returned here.
pub fn host_lifecycle_status(host: &Host) -> &'static str {
    match (
        host.last_contacted_at.as_deref(),
        host.last_error.as_deref(),
    ) {
        (None, _) => "never_contacted",
        (_, Some(error)) if !error.is_empty() => "unreachable",
        _ => "idle",
    }
}

fn parse_issue_status_lines(issues: &ParseIssues) -> Vec<String> {
    let Some(summary) = issues.summary_text() else {
        return Vec::new();
    };
    let mut lines = vec![format!("  parse issues: {summary}")];
    for sample in &issues.samples {
        lines.push(format!("    {}", sample.cli_line(None)));
    }
    lines
}

fn platform_monitor_status_from_probe(probe: PlatformProbe) -> PlatformMonitorStatus {
    PlatformMonitorStatus {
        platform_id: probe.platform_id,
        display_name: probe.display_name,
        source: probe.source_kind,
        probe_status: probe.status.as_str(),
        parser_status: parser_status_label(probe.parser_status),
        quality: probe.quality,
        privacy: probe.privacy,
        roots_checked: probe.roots_checked,
        roots_detected: probe.roots_detected,
        artifact_patterns: probe.artifact_patterns,
        detail: probe.detail,
        next_action: probe.next_action,
    }
}

fn source_status_from_parts(
    descriptor: &SourceDescriptor,
    usage: Option<&SourceBreakdown>,
) -> SourceCapabilityStatus {
    let has_data = usage.is_some_and(|usage| usage.event_count > 0 || usage.total_tokens > 0);
    let status = if !descriptor.capabilities.parser {
        "historical_only"
    } else if has_data {
        "passive_ready"
    } else {
        "passive_no_data"
    };
    let quality = quality_label(descriptor.quality);
    let total_tokens = usage.map(|usage| usage.total_tokens).unwrap_or_default();
    let last_event_at = usage.and_then(|usage| usage.last_event_at.clone());
    let detail = if descriptor.capabilities.parser {
        "passive local artifact reader".to_string()
    } else {
        "historical usage is retained; no passive reader is available".to_string()
    };

    SourceCapabilityStatus {
        source: descriptor.kind,
        stable_id: descriptor.stable_id,
        display_name: descriptor.display_name,
        status,
        quality,
        total_tokens,
        last_event_at,
        token_accounting_version: None,
        legacy_token_accounting: false,
        token_accounting_warning: None,
        detail,
    }
}

fn quality_label(quality: UsageQuality) -> &'static str {
    match quality {
        UsageQuality::Precise => "precise",
        UsageQuality::TotalOnly => "total_only",
        UsageQuality::Estimated => "estimated",
    }
}

fn parser_status_label(status: ParserSupportStatus) -> &'static str {
    status.as_str()
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::platform_monitor::{
            ParserSupportStatus, PlatformProbe, PlatformProbeStatus, registered_platform_monitors,
        },
        domain::source_descriptor::{
            PrivacyClass, SourceCapabilities, SourceDescriptor, UsageQuality,
        },
        models::{ParseIssueKind, ParseIssueSample, ParseIssues, SourceKind},
        query::SourceBreakdown,
    };

    use super::{
        host_lifecycle_status, parse_issue_status_lines, platform_monitor_status_from_probe,
        source_status_from_parts,
    };
    use crate::store::Host;
    use std::collections::BTreeMap;

    const TEST_DESCRIPTOR: SourceDescriptor = SourceDescriptor {
        kind: SourceKind::Codex,
        stable_id: "codex",
        aliases: &[],
        display_name: "Codex",
        capabilities: SourceCapabilities {
            parser: true,
            passive_probe: false,
        },
        quality: UsageQuality::Precise,
        privacy: PrivacyClass::LocalArtifacts,
    };

    fn sample_host(last_contacted_at: Option<&str>, last_error: Option<&str>) -> Host {
        Host {
            host_id: "devbox".to_string(),
            label: "devbox".to_string(),
            transport: "ssh".to_string(),
            ssh_target: Some("me@devbox".to_string()),
            command: "llmusage".to_string(),
            added_at: "2026-08-20T00:00:00Z".to_string(),
            last_contacted_at: last_contacted_at.map(str::to_string),
            last_error: last_error.map(str::to_string),
            import_watermark: None,
        }
    }

    #[test]
    fn host_lifecycle_status_is_idle_unreachable_or_never_contacted() {
        assert_eq!(
            host_lifecycle_status(&sample_host(None, None)),
            "never_contacted"
        );
        assert_eq!(
            host_lifecycle_status(&sample_host(
                Some("2026-08-20T01:00:00Z"),
                Some("ssh timed out")
            )),
            "unreachable"
        );
        assert_eq!(
            host_lifecycle_status(&sample_host(Some("2026-08-20T01:00:00Z"), None)),
            "idle"
        );
        for host in [
            sample_host(None, None),
            sample_host(Some("2026-08-20T01:00:00Z"), Some("ssh timed out")),
            sample_host(Some("2026-08-20T01:00:00Z"), None),
        ] {
            assert_ne!(host_lifecycle_status(&host), "live");
        }
    }

    #[test]
    fn status_reports_passive_no_data_without_history() {
        let status = source_status_from_parts(&TEST_DESCRIPTOR, None);

        assert_eq!(status.status, "passive_no_data");
        assert_eq!(status.quality, "precise");
    }

    #[test]
    fn status_reports_passive_ready_when_data_exists() {
        let usage = SourceBreakdown {
            source: "codex".to_string(),
            total_tokens: 42,
            last_event_at: Some("2026-05-28T00:00:00Z".to_string()),
            event_count: 1,
        };

        let status = source_status_from_parts(&TEST_DESCRIPTOR, Some(&usage));

        assert_eq!(status.status, "passive_ready");
        assert_eq!(status.total_tokens, 42);
    }

    #[test]
    fn parserless_antigravity_is_historical_only() {
        let descriptor = SourceDescriptor {
            kind: SourceKind::Antigravity,
            stable_id: "antigravity",
            aliases: &[],
            display_name: "Antigravity",
            capabilities: SourceCapabilities {
                parser: false,
                passive_probe: false,
            },
            quality: UsageQuality::TotalOnly,
            privacy: PrivacyClass::LocalArtifacts,
        };

        let status = source_status_from_parts(&descriptor, None);

        assert_eq!(status.status, "historical_only");
    }

    #[test]
    fn platform_status_keeps_monitor_only_platform_out_of_source_kind() {
        let gemini = registered_platform_monitors()
            .iter()
            .find(|descriptor| descriptor.platform_id == "gemini")
            .expect("gemini monitor should exist");
        let probe = PlatformProbe {
            platform_id: gemini.platform_id,
            display_name: gemini.display_name,
            source_kind: gemini.source_kind,
            status: PlatformProbeStatus::Unavailable,
            parser_status: ParserSupportStatus::BlockedNoSamples,
            quality: None,
            privacy: "local_artifacts",
            roots_checked: 1,
            roots_detected: 0,
            artifact_patterns: gemini.artifact_patterns,
            detail: "no candidate roots detected".to_string(),
            next_action: gemini.next_action,
        };

        let status = platform_monitor_status_from_probe(probe);

        assert_eq!(status.platform_id, "gemini");
        assert_eq!(status.source, None);
        assert_eq!(status.probe_status, "unavailable");
        assert_eq!(status.parser_status, "blocked_no_samples");
    }

    #[test]
    fn parse_issue_summary_is_emitted_for_any_nonzero_class() {
        let mut issues = BTreeMap::new();
        issues.insert(
            "codex".to_string(),
            ParseIssues {
                skipped_lines: 3,
                accounting_anomaly_lines: 1,
                ..ParseIssues::default()
            },
        );
        let status = source_status_from_parts(&TEST_DESCRIPTOR, None);
        let mut output = Vec::new();
        {
            // Capture by formatting the same helper the printer uses.
            let summary = issues
                .get(status.source.as_str())
                .and_then(ParseIssues::summary_text);
            output.push(summary);
        }
        assert_eq!(output[0].as_deref(), Some("skipped=3 accounting=1"));
        assert!(ParseIssues::default().summary_text().is_none());
    }

    #[test]
    fn parse_issue_status_prints_reason_without_at_zero() {
        let issues = ParseIssues {
            skipped_lines: 1,
            samples: vec![ParseIssueSample {
                source: SourceKind::Zcode,
                path_hash: "zcode-hash".to_string(),
                offset: 0,
                kind: ParseIssueKind::Skipped,
                reason: "zcode_unfinished:error:invalid_request".to_string(),
            }],
            ..ParseIssues::default()
        };
        let lines = parse_issue_status_lines(&issues);
        assert_eq!(lines[0], "  parse issues: skipped=1");
        assert_eq!(
            lines[1],
            "    skipped zcode_unfinished:error:invalid_request"
        );
        assert!(lines.iter().all(|line| !line.contains("@0")));
        assert!(lines.iter().all(|line| !line.contains("zcode-hash")));
    }
}
