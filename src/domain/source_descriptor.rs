//! Static source capability descriptors.
//!
//! `SourceKind` remains the stable persisted identifier.  This module carries
//! the source metadata that would otherwise drift across status, integrations,
//! sync, and future passive-reader onboarding code.

use super::models::SourceKind;

/// Coarse activation family for one usage source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationMode {
    /// llmusage writes a hook / notify command into the target tool config.
    Hook(HookActivation),
    /// llmusage installs or manages a target-tool plugin.
    Plugin(PluginActivation),
    /// llmusage only reads existing local artifacts and writes no tool config.
    Passive(PassiveActivation),
    /// Hook / plugin triggers and passive artifact discovery can both apply.
    Hybrid(HybridActivation),
}

/// Hook-specific activation metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookActivation {
    /// Human-readable target event names, e.g. `Stop` or `notify`.
    pub events: &'static [&'static str],
    /// Whether the target tool exposes only one notify/hook slot.
    pub singleton: bool,
    /// Whether local artifacts can still be read when the hook is absent.
    pub passive_fallback: bool,
}

/// Plugin-specific activation metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginActivation {
    /// Human-readable plugin trigger/event names.
    pub events: &'static [&'static str],
}

/// Passive-reader activation metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassiveActivation {
    /// Human-readable artifact family names.
    pub artifacts: &'static [&'static str],
    /// Whether reading the artifacts requires local auth material.
    pub requires_auth: bool,
}

/// Hybrid activation metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HybridActivation {
    pub hook: HookActivation,
    pub passive: PassiveActivation,
}

/// Source-level implementation capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCapabilities {
    /// A `SourceParser` is registered for this source.
    pub parser: bool,
    /// An `Integration` is registered for this source.
    pub integration: bool,
    /// Hook-run signals can name this source.
    pub hook_signal: bool,
    /// Passive artifact probe/status semantics are expected for this source.
    pub passive_probe: bool,
}

/// Declared token/usage quality for imported events from a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageQuality {
    /// Token fields are sourced from first-party structured local usage data.
    Precise,
    /// The source exposes a reliable total but not all token subchannels.
    TotalOnly,
    /// Values are estimated and must be labelled as such.
    Estimated,
}

/// Coarse privacy boundary for local artifact reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyClass {
    /// Reads local transcript/log artifacts to derive usage.
    LocalArtifacts,
    /// Reads a local database to derive usage.
    LocalDatabase,
}

/// Static descriptor for one persisted source id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceDescriptor {
    pub kind: SourceKind,
    pub stable_id: &'static str,
    pub aliases: &'static [&'static str],
    pub display_name: &'static str,
    pub activation: ActivationMode,
    pub capabilities: SourceCapabilities,
    pub quality: UsageQuality,
    pub privacy: PrivacyClass,
}

impl SourceDescriptor {
    /// Returns true when `value` names this source by stable id or alias.
    pub fn matches_id(self, value: &str) -> bool {
        self.stable_id == value || self.aliases.contains(&value)
    }
}

/// Static source descriptors in stable CLI/display order.
pub const SOURCE_DESCRIPTORS: &[SourceDescriptor] = &[
    SourceDescriptor {
        kind: SourceKind::Codex,
        stable_id: "codex",
        aliases: &[],
        display_name: "Codex",
        activation: ActivationMode::Hybrid(HybridActivation {
            hook: HookActivation {
                events: &["notify"],
                singleton: true,
                passive_fallback: true,
            },
            passive: PassiveActivation {
                artifacts: &["sessions/rollout-*.jsonl"],
                requires_auth: false,
            },
        }),
        capabilities: SourceCapabilities {
            parser: true,
            integration: true,
            hook_signal: true,
            passive_probe: false,
        },
        quality: UsageQuality::Precise,
        privacy: PrivacyClass::LocalArtifacts,
    },
    SourceDescriptor {
        kind: SourceKind::Claude,
        stable_id: "claude",
        aliases: &[],
        display_name: "Claude",
        activation: ActivationMode::Hybrid(HybridActivation {
            hook: HookActivation {
                events: &["Stop", "SessionEnd"],
                singleton: false,
                passive_fallback: true,
            },
            passive: PassiveActivation {
                artifacts: &["projects/*.jsonl"],
                requires_auth: false,
            },
        }),
        capabilities: SourceCapabilities {
            parser: true,
            integration: true,
            hook_signal: true,
            passive_probe: false,
        },
        quality: UsageQuality::Precise,
        privacy: PrivacyClass::LocalArtifacts,
    },
    SourceDescriptor {
        kind: SourceKind::Opencode,
        stable_id: "opencode",
        aliases: &[],
        display_name: "OpenCode",
        activation: ActivationMode::Plugin(PluginActivation {
            events: &["session.updated"],
        }),
        capabilities: SourceCapabilities {
            parser: true,
            integration: true,
            hook_signal: true,
            passive_probe: false,
        },
        quality: UsageQuality::Precise,
        privacy: PrivacyClass::LocalDatabase,
    },
    SourceDescriptor {
        kind: SourceKind::Antigravity,
        stable_id: "antigravity",
        aliases: &[],
        display_name: "Antigravity",
        activation: ActivationMode::Hook(HookActivation {
            events: &["Stop"],
            singleton: false,
            passive_fallback: false,
        }),
        capabilities: SourceCapabilities {
            parser: false,
            integration: true,
            hook_signal: true,
            passive_probe: false,
        },
        quality: UsageQuality::TotalOnly,
        privacy: PrivacyClass::LocalArtifacts,
    },
    SourceDescriptor {
        kind: SourceKind::KimiCode,
        stable_id: "kimi_code",
        aliases: &[],
        display_name: "Kimi Code",
        activation: ActivationMode::Passive(PassiveActivation {
            artifacts: &[
                ".kimi-code/sessions/**/wire.jsonl",
                "KIMI_CODE_HOME/sessions/**/wire.jsonl",
            ],
            requires_auth: false,
        }),
        capabilities: SourceCapabilities {
            parser: true,
            integration: false,
            hook_signal: false,
            passive_probe: true,
        },
        quality: UsageQuality::Precise,
        privacy: PrivacyClass::LocalArtifacts,
    },
    SourceDescriptor {
        kind: SourceKind::Pi,
        stable_id: "pi",
        aliases: &[],
        display_name: "Pi / Oh My Pi",
        activation: ActivationMode::Passive(PassiveActivation {
            artifacts: &[
                ".pi/agent/sessions/**/*.jsonl",
                ".omp/agent/sessions/**/*.jsonl",
                "PI_AGENT_DIR/**/*.jsonl",
            ],
            requires_auth: false,
        }),
        capabilities: SourceCapabilities {
            parser: true,
            integration: false,
            hook_signal: false,
            passive_probe: true,
        },
        quality: UsageQuality::Precise,
        privacy: PrivacyClass::LocalArtifacts,
    },
];

/// Current build's source descriptors in stable CLI/display order.
pub fn registered_source_descriptors() -> &'static [SourceDescriptor] {
    SOURCE_DESCRIPTORS
}

/// Look up a descriptor by stable source kind.
pub fn source_descriptor(kind: SourceKind) -> Option<&'static SourceDescriptor> {
    SOURCE_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.kind == kind)
}

/// Parse a stable source id or descriptor alias.
pub fn parse_source_id(value: &str) -> Option<SourceKind> {
    SOURCE_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.matches_id(value))
        .map(|descriptor| descriptor.kind)
}
