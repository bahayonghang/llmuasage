use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;

use crate::{
    app::AppContext,
    domain::{
        platform_monitor::{self, ParserSupportStatus, PlatformProbe},
        source_descriptor::{SourceDescriptor, UsageQuality},
    },
    models::SourceKind,
    query::{Dashboard, SourceBreakdown},
    registry,
    store::Store,
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
    let sources = dashboard.source_breakdown(&Default::default())?;
    let mut capability_statuses = build_source_capability_statuses(&sources);
    apply_token_accounting_statuses(&store, &mut capability_statuses)?;
    let platform_statuses = build_platform_monitor_statuses();

    println!("Source status:");
    print_human_statuses(&capability_statuses, &platform_statuses);
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
            status.token_accounting_warning = Some(format!(
                "legacy token accounting; run unbounded `llmusage sync` for automatic safe repair; if blocked, restore source files and run `llmusage sync --rebuild --source {}`",
                status.source.as_str()
            ));
        }
    }
    Ok(())
}

pub fn print_human_statuses(
    capability_statuses: &[SourceCapabilityStatus],
    platform_statuses: &[PlatformMonitorStatus],
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
        models::SourceKind,
        query::SourceBreakdown,
    };

    use super::{platform_monitor_status_from_probe, source_status_from_parts};

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
}
