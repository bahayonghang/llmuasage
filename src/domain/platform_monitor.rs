//! Static platform monitoring descriptors.
//!
//! These descriptors intentionally cover more platforms than `SourceKind`.
//! A monitored platform can be detected and explained without becoming a
//! persisted usage source or writing untrusted token rows.

use std::path::PathBuf;

use serde::Serialize;

use crate::{models::SourceKind, util::resolve_home_dir};

use super::source_descriptor::{PrivacyClass, UsageQuality};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserSupportStatus {
    Registered,
    Planned,
    BlockedNoSamples,
    BlockedNoUsage,
    Unsupported,
    ExternalOnly,
}

impl ParserSupportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Planned => "planned",
            Self::BlockedNoSamples => "blocked_no_samples",
            Self::BlockedNoUsage => "blocked_no_usage",
            Self::Unsupported => "unsupported",
            Self::ExternalOnly => "external_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformProbeStatus {
    Detected,
    Unavailable,
}

impl PlatformProbeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Detected => "detected",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorRoot {
    Home(&'static str),
    EnvOrHome {
        env: &'static str,
        env_relative: &'static str,
        home_relative: &'static str,
    },
    EnvListOrHome {
        env: &'static str,
        home_relative: &'static str,
    },
    XdgData(&'static str),
    XdgConfig(&'static str),
    ConfigDir(&'static str),
    DataDir(&'static str),
    AppData(&'static str),
    LocalAppData(&'static str),
}

impl MonitorRoot {
    pub fn label(self) -> &'static str {
        match self {
            Self::Home(_) => "home",
            Self::EnvOrHome { env, .. } => env,
            Self::EnvListOrHome { env, .. } => env,
            Self::XdgData(_) => "xdg_data",
            Self::XdgConfig(_) => "xdg_config",
            Self::ConfigDir(_) => "config_dir",
            Self::DataDir(_) => "data_dir",
            Self::AppData(_) => "appdata",
            Self::LocalAppData(_) => "localappdata",
        }
    }

    fn resolve_all(self, home_dir: &std::path::Path) -> Vec<PathBuf> {
        match self {
            Self::Home(relative) => vec![home_dir.join(relative)],
            Self::EnvOrHome {
                env,
                env_relative,
                home_relative,
            } => std::env::var_os(env)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|root| root.join(env_relative))
                .map(|path| vec![path])
                .unwrap_or_else(|| vec![home_dir.join(home_relative)]),
            Self::EnvListOrHome { env, home_relative } => std::env::var_os(env)
                .filter(|value| !value.is_empty())
                .map(|value| split_path_list(&value))
                .filter(|paths| !paths.is_empty())
                .unwrap_or_else(|| vec![home_dir.join(home_relative)]),
            Self::XdgData(relative) => xdg_data_dir(home_dir)
                .map(|root| root.join(relative))
                .into_iter()
                .collect(),
            Self::XdgConfig(relative) => xdg_config_dir(home_dir)
                .map(|root| root.join(relative))
                .into_iter()
                .collect(),
            Self::ConfigDir(relative) => dirs::config_dir()
                .map(|root| root.join(relative))
                .into_iter()
                .collect(),
            Self::DataDir(relative) => dirs::data_dir()
                .map(|root| root.join(relative))
                .into_iter()
                .collect(),
            Self::AppData(relative) => std::env::var_os("APPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|root| root.join(relative))
                .into_iter()
                .collect(),
            Self::LocalAppData(relative) => std::env::var_os("LOCALAPPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|root| root.join(relative))
                .into_iter()
                .collect(),
        }
    }
}

fn split_path_list(value: &std::ffi::OsStr) -> Vec<PathBuf> {
    value
        .to_string_lossy()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformMonitorDescriptor {
    pub platform_id: &'static str,
    pub display_name: &'static str,
    pub source_kind: Option<SourceKind>,
    pub roots: &'static [MonitorRoot],
    pub artifact_patterns: &'static [&'static str],
    pub parser_status: ParserSupportStatus,
    pub quality: Option<UsageQuality>,
    pub privacy: PrivacyClass,
    pub next_action: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformProbe {
    pub platform_id: &'static str,
    pub display_name: &'static str,
    pub source_kind: Option<SourceKind>,
    pub status: PlatformProbeStatus,
    pub parser_status: ParserSupportStatus,
    pub quality: Option<&'static str>,
    pub privacy: &'static str,
    pub roots_checked: usize,
    pub roots_detected: usize,
    pub artifact_patterns: &'static [&'static str],
    pub detail: String,
    pub next_action: &'static str,
}

pub const PLATFORM_MONITORS: &[PlatformMonitorDescriptor] = &[
    PlatformMonitorDescriptor {
        platform_id: "codex",
        display_name: "Codex",
        source_kind: Some(SourceKind::Codex),
        roots: &[MonitorRoot::EnvOrHome {
            env: "CODEX_HOME",
            env_relative: "sessions",
            home_relative: ".codex/sessions",
        }],
        artifact_patterns: &["*.jsonl"],
        parser_status: ParserSupportStatus::Registered,
        quality: Some(UsageQuality::Precise),
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "parsed by the registered Codex source parser",
    },
    PlatformMonitorDescriptor {
        platform_id: "claude",
        display_name: "Claude",
        source_kind: Some(SourceKind::Claude),
        roots: &[MonitorRoot::Home(".claude/projects")],
        artifact_patterns: &["*.jsonl"],
        parser_status: ParserSupportStatus::Registered,
        quality: Some(UsageQuality::Precise),
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "parsed by the registered Claude source parser",
    },
    PlatformMonitorDescriptor {
        platform_id: "opencode",
        display_name: "OpenCode",
        source_kind: Some(SourceKind::Opencode),
        roots: &[
            MonitorRoot::EnvOrHome {
                env: "OPENCODE_HOME",
                env_relative: "",
                home_relative: ".local/share/opencode",
            },
            MonitorRoot::LocalAppData("opencode"),
            MonitorRoot::XdgData("opencode/storage/message"),
        ],
        artifact_patterns: &["opencode*.db", "*.json"],
        parser_status: ParserSupportStatus::Registered,
        quality: Some(UsageQuality::Precise),
        privacy: PrivacyClass::LocalDatabase,
        next_action: "parsed by the registered OpenCode source parser",
    },
    PlatformMonitorDescriptor {
        platform_id: "antigravity",
        display_name: "Antigravity",
        source_kind: Some(SourceKind::Antigravity),
        roots: &[MonitorRoot::Home(".gemini/config/hooks.json")],
        artifact_patterns: &["hooks.json"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: Some(UsageQuality::TotalOnly),
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "integration-only until a token-bearing Antigravity fixture exists",
    },
    PlatformMonitorDescriptor {
        platform_id: "kimi_code",
        display_name: "Kimi Code",
        source_kind: Some(SourceKind::KimiCode),
        roots: &[MonitorRoot::EnvOrHome {
            env: "KIMI_CODE_HOME",
            env_relative: "sessions",
            home_relative: ".kimi-code/sessions",
        }],
        artifact_patterns: &["wire.jsonl"],
        parser_status: ParserSupportStatus::Registered,
        quality: Some(UsageQuality::Precise),
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "parsed by the registered Kimi Code source parser",
    },
    PlatformMonitorDescriptor {
        platform_id: "pi",
        display_name: "Pi / Oh My Pi",
        source_kind: Some(SourceKind::Pi),
        roots: &[
            MonitorRoot::EnvListOrHome {
                env: "PI_AGENT_DIR",
                home_relative: ".pi/agent/sessions",
            },
            MonitorRoot::Home(".omp/agent/sessions"),
        ],
        artifact_patterns: &["*.jsonl"],
        parser_status: ParserSupportStatus::Registered,
        quality: Some(UsageQuality::Precise),
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "parsed by the registered Pi source parser",
    },
    PlatformMonitorDescriptor {
        platform_id: "reasonix",
        display_name: "Reasonix",
        source_kind: None,
        roots: &[MonitorRoot::AppData("reasonix/projects")],
        artifact_patterns: &["sessions/*.jsonl", "*.telemetry.json"],
        parser_status: ParserSupportStatus::BlockedNoUsage,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; current sessions lack replayable per-turn usage",
    },
    PlatformMonitorDescriptor {
        platform_id: "gemini",
        display_name: "Gemini CLI",
        source_kind: None,
        roots: &[MonitorRoot::EnvOrHome {
            env: "GEMINI_CLI_HOME",
            env_relative: "tmp",
            home_relative: ".gemini/tmp",
        }],
        artifact_patterns: &["*.json", "*.jsonl"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires sanitized Gemini CLI samples and token semantics",
    },
    PlatformMonitorDescriptor {
        platform_id: "cursor",
        display_name: "Cursor",
        source_kind: None,
        roots: &[MonitorRoot::Home(".config/tokscale/cursor-cache")],
        artifact_patterns: &["usage*.csv"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires stable local usage export samples",
    },
    PlatformMonitorDescriptor {
        platform_id: "copilot",
        display_name: "GitHub Copilot",
        source_kind: None,
        roots: &[
            MonitorRoot::ConfigDir("github-copilot"),
            MonitorRoot::AppData("GitHub Copilot"),
        ],
        artifact_patterns: &["*.json", "*.db"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalDatabase,
        next_action: "monitor-only; requires privacy review and token semantics",
    },
    PlatformMonitorDescriptor {
        platform_id: "zed",
        display_name: "Zed",
        source_kind: None,
        roots: &[
            MonitorRoot::XdgData("zed"),
            MonitorRoot::Home("Library/Application Support/Zed"),
        ],
        artifact_patterns: &["*.db", "*.jsonl"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalDatabase,
        next_action: "monitor-only; requires Zed fixture coverage",
    },
    PlatformMonitorDescriptor {
        platform_id: "kiro",
        display_name: "Kiro",
        source_kind: None,
        roots: &[MonitorRoot::Home(".kiro")],
        artifact_patterns: &["*.json", "*.jsonl"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires Kiro session samples",
    },
    PlatformMonitorDescriptor {
        platform_id: "goose",
        display_name: "Goose",
        source_kind: None,
        roots: &[MonitorRoot::ConfigDir("goose")],
        artifact_patterns: &["*.jsonl", "*.db"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires Goose session samples",
    },
    PlatformMonitorDescriptor {
        platform_id: "grok",
        display_name: "Grok",
        source_kind: None,
        roots: &[MonitorRoot::Home(".grok")],
        artifact_patterns: &["*.json", "*.jsonl"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires Grok local artifact samples",
    },
    PlatformMonitorDescriptor {
        platform_id: "kimi",
        display_name: "Kimi / Qwen",
        source_kind: None,
        roots: &[
            MonitorRoot::Home(".kimi/sessions"),
            MonitorRoot::Home(".qwen"),
        ],
        artifact_patterns: &["wire.jsonl", "*.jsonl"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires Kimi/Qwen token semantics",
    },
    PlatformMonitorDescriptor {
        platform_id: "roo_kilo_cline",
        display_name: "Roo / Kilo / Cline",
        source_kind: None,
        roots: &[
            MonitorRoot::XdgConfig("Code/User/globalStorage"),
            MonitorRoot::AppData("Code/User/globalStorage"),
        ],
        artifact_patterns: &["tasks/*.json", "*.json"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires extension-specific fixture review",
    },
    PlatformMonitorDescriptor {
        platform_id: "codebuff",
        display_name: "Codebuff",
        source_kind: None,
        roots: &[MonitorRoot::Home(".codebuff")],
        artifact_patterns: &["*.json", "*.jsonl"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires Codebuff samples",
    },
    PlatformMonitorDescriptor {
        platform_id: "crush",
        display_name: "Crush",
        source_kind: None,
        roots: &[MonitorRoot::Home(".crush")],
        artifact_patterns: &["*.db", "*.json"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalDatabase,
        next_action: "monitor-only; requires Crush database schema review",
    },
    PlatformMonitorDescriptor {
        platform_id: "warp_oz",
        display_name: "Warp / Oz",
        source_kind: None,
        roots: &[MonitorRoot::ConfigDir("warp"), MonitorRoot::Home(".oz")],
        artifact_patterns: &["*.json", "*.db"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires Warp/Oz samples",
    },
    PlatformMonitorDescriptor {
        platform_id: "amp",
        display_name: "Amp",
        source_kind: None,
        roots: &[MonitorRoot::XdgData("amp/threads")],
        artifact_patterns: &["T-*.json"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires Amp thread samples",
    },
    PlatformMonitorDescriptor {
        platform_id: "hermes",
        display_name: "Hermes",
        source_kind: None,
        roots: &[MonitorRoot::DataDir("hermes")],
        artifact_patterns: &["*.db"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalDatabase,
        next_action: "monitor-only; requires Hermes database schema review",
    },
    PlatformMonitorDescriptor {
        platform_id: "trae",
        display_name: "Trae",
        source_kind: None,
        roots: &[MonitorRoot::ConfigDir("trae")],
        artifact_patterns: &["*.json", "*.db"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalDatabase,
        next_action: "monitor-only; requires Trae auth/privacy review and samples",
    },
    PlatformMonitorDescriptor {
        platform_id: "openclaw_pi_droid",
        display_name: "OpenClaw / Pi / Droid",
        source_kind: None,
        // Pi's `.pi/agent/sessions` root now belongs to the registered `pi`
        // source monitor; keep only the still-unparsed OpenClaw/Droid roots here
        // so one root is never reported as both registered and blocked.
        roots: &[
            MonitorRoot::Home(".openclaw/agents"),
            MonitorRoot::Home(".factory/sessions"),
        ],
        artifact_patterns: &["*.jsonl", "*.settings.json"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires per-client samples and token semantics",
    },
    PlatformMonitorDescriptor {
        platform_id: "gajae_code",
        display_name: "Gajae-Code",
        source_kind: None,
        roots: &[
            MonitorRoot::EnvOrHome {
                env: "GJC_CONFIG_DIR",
                env_relative: "agent/sessions",
                home_relative: ".gjc/agent/sessions",
            },
            MonitorRoot::XdgData("gjc/sessions"),
        ],
        artifact_patterns: &["*.jsonl"],
        parser_status: ParserSupportStatus::BlockedNoSamples,
        quality: None,
        privacy: PrivacyClass::LocalArtifacts,
        next_action: "monitor-only; requires Gajae-Code samples",
    },
    PlatformMonitorDescriptor {
        platform_id: "synthetic",
        display_name: "Synthetic",
        source_kind: None,
        roots: &[MonitorRoot::Home(".config/tokscale/synthetic.db")],
        artifact_patterns: &["synthetic.db"],
        parser_status: ParserSupportStatus::Unsupported,
        quality: None,
        privacy: PrivacyClass::LocalDatabase,
        next_action: "not a real local source; keep out of llmusage imports",
    },
];

pub fn registered_platform_monitors() -> &'static [PlatformMonitorDescriptor] {
    PLATFORM_MONITORS
}

pub fn probe_registered_platforms() -> Vec<PlatformProbe> {
    let home_dir = resolve_home_dir();
    PLATFORM_MONITORS
        .iter()
        .map(|descriptor| probe_platform_descriptor(descriptor, &home_dir))
        .collect()
}

pub fn probe_platform_descriptor(
    descriptor: &PlatformMonitorDescriptor,
    home_dir: &std::path::Path,
) -> PlatformProbe {
    let resolved_roots = descriptor
        .roots
        .iter()
        .flat_map(|root| {
            root.resolve_all(home_dir)
                .into_iter()
                .map(|path| (*root, path))
        })
        .collect::<Vec<_>>();
    let detected = resolved_roots
        .iter()
        .filter(|(_, path)| path.exists())
        .collect::<Vec<_>>();
    let status = if detected.is_empty() {
        PlatformProbeStatus::Unavailable
    } else {
        PlatformProbeStatus::Detected
    };
    let detail = if detected.is_empty() {
        let labels = descriptor
            .roots
            .iter()
            .map(|root| root.label())
            .collect::<Vec<_>>()
            .join(", ");
        format!("no candidate roots detected; checked {labels}")
    } else {
        let labels = detected
            .iter()
            .map(|(root, _)| root.label())
            .collect::<Vec<_>>()
            .join(", ");
        format!("candidate roots detected via {labels}")
    };

    PlatformProbe {
        platform_id: descriptor.platform_id,
        display_name: descriptor.display_name,
        source_kind: descriptor.source_kind,
        status,
        parser_status: descriptor.parser_status,
        quality: descriptor.quality.map(quality_label),
        privacy: privacy_label(descriptor.privacy),
        roots_checked: resolved_roots.len(),
        roots_detected: detected.len(),
        artifact_patterns: descriptor.artifact_patterns,
        detail,
        next_action: descriptor.next_action,
    }
}

fn xdg_data_dir(home_dir: &std::path::Path) -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(home_dir.join(".local/share")))
}

fn xdg_config_dir(home_dir: &std::path::Path) -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(home_dir.join(".config")))
}

fn quality_label(quality: UsageQuality) -> &'static str {
    match quality {
        UsageQuality::Precise => "precise",
        UsageQuality::TotalOnly => "total_only",
        UsageQuality::Estimated => "estimated",
    }
}

fn privacy_label(privacy: PrivacyClass) -> &'static str {
    match privacy {
        PrivacyClass::LocalArtifacts => "local_artifacts",
        PrivacyClass::LocalDatabase => "local_database",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use tempfile::TempDir;

    use super::*;
    use crate::domain::source_descriptor::registered_source_descriptors;

    #[test]
    fn platform_monitor_ids_are_unique() {
        let mut ids = BTreeSet::new();
        for descriptor in registered_platform_monitors() {
            assert!(
                ids.insert(descriptor.platform_id),
                "duplicate platform monitor id {}",
                descriptor.platform_id
            );
        }
    }

    #[test]
    fn source_descriptors_have_platform_monitors() {
        let monitored_sources = registered_platform_monitors()
            .iter()
            .filter_map(|descriptor| descriptor.source_kind)
            .collect::<BTreeSet<_>>();

        for descriptor in registered_source_descriptors() {
            assert!(
                monitored_sources.contains(&descriptor.kind),
                "source {} missing platform monitor",
                descriptor.stable_id
            );
        }
    }

    #[test]
    fn gemini_monitor_does_not_restore_gemini_source_id() {
        let gemini = registered_platform_monitors()
            .iter()
            .find(|descriptor| descriptor.platform_id == "gemini")
            .expect("gemini monitor should exist");

        assert_eq!(gemini.source_kind, None);
        assert_eq!(SourceKind::parse_id("gemini"), None);
    }

    #[test]
    fn reasonix_monitor_stays_parserless_without_usage_semantics() {
        let reasonix = registered_platform_monitors()
            .iter()
            .find(|descriptor| descriptor.platform_id == "reasonix")
            .expect("reasonix monitor should exist");

        assert_eq!(reasonix.source_kind, None);
        assert_eq!(reasonix.parser_status, ParserSupportStatus::BlockedNoUsage);
        assert_eq!(SourceKind::parse_id("reasonix"), None);
    }

    #[test]
    fn comma_separated_monitor_roots_match_pi_discovery_shape() {
        assert_eq!(
            split_path_list(std::ffi::OsStr::new(" C:/pi-a, C:/pi-b ,, C:/pi-a ")),
            vec![
                PathBuf::from("C:/pi-a"),
                PathBuf::from("C:/pi-b"),
                PathBuf::from("C:/pi-a"),
            ]
        );
    }

    #[test]
    fn probe_reports_detected_when_candidate_root_exists() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        std::fs::create_dir_all(temp.path().join(".claude/projects"))?;
        let claude = registered_platform_monitors()
            .iter()
            .find(|descriptor| descriptor.platform_id == "claude")
            .expect("claude monitor should exist");

        let probe = probe_platform_descriptor(claude, temp.path());

        assert_eq!(probe.status, PlatformProbeStatus::Detected);
        assert_eq!(probe.roots_detected, 1);
        assert_eq!(probe.parser_status, ParserSupportStatus::Registered);
        Ok(())
    }

    #[test]
    fn probe_reports_unavailable_without_candidate_roots() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let claude = registered_platform_monitors()
            .iter()
            .find(|descriptor| descriptor.platform_id == "claude")
            .expect("claude monitor should exist");

        let probe = probe_platform_descriptor(claude, temp.path());

        assert_eq!(probe.status, PlatformProbeStatus::Unavailable);
        assert_eq!(probe.roots_detected, 0);
        Ok(())
    }
}
