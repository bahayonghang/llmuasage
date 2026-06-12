use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    domain::{
        platform_monitor::{self, ParserSupportStatus, PlatformProbe},
        source_descriptor::{ActivationMode, SourceDescriptor, UsageQuality},
    },
    integrations::IntegrationProbe,
    models::SourceKind,
    query::SourceBreakdown,
    registry,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceCapabilityStatus {
    pub source: SourceKind,
    pub stable_id: &'static str,
    pub display_name: &'static str,
    pub activation: &'static str,
    pub configured: bool,
    pub status: &'static str,
    pub quality: &'static str,
    pub total_tokens: i64,
    pub last_event_at: Option<String>,
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

pub fn build_source_capability_statuses(
    probes: &[IntegrationProbe],
    sources: &[SourceBreakdown],
) -> Vec<SourceCapabilityStatus> {
    let probes = probes
        .iter()
        .map(|probe| (probe.source, probe))
        .collect::<BTreeMap<_, _>>();
    let usage = sources
        .iter()
        .filter_map(|source| {
            SourceKind::parse_id(&source.source).map(|kind| (kind, source.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    registry::registered_source_descriptors()
        .iter()
        .map(|descriptor| {
            let probe = probes.get(&descriptor.kind).copied();
            let source_usage = usage.get(&descriptor.kind);
            source_status_from_parts(descriptor, probe, source_usage)
        })
        .collect()
}

pub fn build_platform_monitor_statuses() -> Vec<PlatformMonitorStatus> {
    platform_monitor::probe_registered_platforms()
        .into_iter()
        .map(platform_monitor_status_from_probe)
        .collect()
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
    probe: Option<&IntegrationProbe>,
    usage: Option<&SourceBreakdown>,
) -> SourceCapabilityStatus {
    let configured =
        probe.is_some_and(|probe| matches!(probe.status.as_str(), "ready" | "partial"));
    let has_data = usage.is_some_and(|usage| usage.event_count > 0 || usage.total_tokens > 0);
    let status = match descriptor.activation {
        ActivationMode::Hook(_) | ActivationMode::Plugin(_) | ActivationMode::Hybrid(_) => {
            if configured {
                "configured"
            } else if has_data {
                "degraded_hook_missing"
            } else {
                "not_detected"
            }
        }
        ActivationMode::Passive(_) => {
            if has_data {
                "passive_ready"
            } else {
                "passive_no_data"
            }
        }
    };
    let quality = quality_label(descriptor.quality);
    let total_tokens = usage.map(|usage| usage.total_tokens).unwrap_or_default();
    let last_event_at = usage.and_then(|usage| usage.last_event_at.clone());
    let detail = probe
        .map(|probe| probe.detail.clone())
        .unwrap_or_else(|| "no integration probe available".to_string());

    SourceCapabilityStatus {
        source: descriptor.kind,
        stable_id: descriptor.stable_id,
        display_name: descriptor.display_name,
        activation: activation_label(descriptor.activation),
        configured,
        status,
        quality,
        total_tokens,
        last_event_at,
        detail,
    }
}

fn activation_label(activation: ActivationMode) -> &'static str {
    match activation {
        ActivationMode::Hook(_) => "hook",
        ActivationMode::Plugin(_) => "plugin",
        ActivationMode::Passive(_) => "passive",
        ActivationMode::Hybrid(_) => "hybrid",
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
            ActivationMode, HookActivation, PrivacyClass, SourceCapabilities, SourceDescriptor,
            UsageQuality,
        },
        integrations::IntegrationProbe,
        models::SourceKind,
        query::SourceBreakdown,
    };

    use super::{platform_monitor_status_from_probe, source_status_from_parts};

    const TEST_DESCRIPTOR: SourceDescriptor = SourceDescriptor {
        kind: SourceKind::Codex,
        stable_id: "codex",
        aliases: &[],
        display_name: "Codex",
        activation: ActivationMode::Hook(HookActivation {
            events: &["notify"],
            singleton: true,
            passive_fallback: true,
        }),
        capabilities: SourceCapabilities {
            parser: true,
            integration: true,
            hook_signal: true,
            passive_probe: false,
        },
        quality: UsageQuality::Precise,
        privacy: PrivacyClass::LocalArtifacts,
    };

    #[test]
    fn status_reports_configured_when_probe_ready() {
        let probe = IntegrationProbe {
            source: SourceKind::Codex,
            status: "ready".to_string(),
            detail: "ready".to_string(),
            config_path: None,
        };

        let status = source_status_from_parts(&TEST_DESCRIPTOR, Some(&probe), None);

        assert!(status.configured);
        assert_eq!(status.status, "configured");
        assert_eq!(status.activation, "hook");
        assert_eq!(status.quality, "precise");
    }

    #[test]
    fn status_reports_hook_missing_when_data_exists_without_ready_probe() {
        let usage = SourceBreakdown {
            source: "codex".to_string(),
            total_tokens: 42,
            last_event_at: Some("2026-05-28T00:00:00Z".to_string()),
            event_count: 1,
        };

        let status = source_status_from_parts(&TEST_DESCRIPTOR, None, Some(&usage));

        assert!(!status.configured);
        assert_eq!(status.status, "degraded_hook_missing");
        assert_eq!(status.total_tokens, 42);
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
