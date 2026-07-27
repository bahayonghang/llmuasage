//! Static metadata for persisted usage sources.

use super::models::SourceKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCapabilities {
    /// A passive `SourceParser` is registered for this source.
    pub parser: bool,
    /// Passive artifact probe/status semantics are expected for this source.
    pub passive_probe: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageQuality {
    Precise,
    TotalOnly,
    Estimated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyClass {
    LocalArtifacts,
    LocalDatabase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceDescriptor {
    pub kind: SourceKind,
    pub stable_id: &'static str,
    pub aliases: &'static [&'static str],
    pub display_name: &'static str,
    pub capabilities: SourceCapabilities,
    pub quality: UsageQuality,
    pub privacy: PrivacyClass,
}

impl SourceDescriptor {
    pub fn matches_id(self, value: &str) -> bool {
        self.stable_id == value || self.aliases.contains(&value)
    }
}

pub const SOURCE_DESCRIPTORS: &[SourceDescriptor] = &[
    SourceDescriptor {
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
    },
    SourceDescriptor {
        kind: SourceKind::Claude,
        stable_id: "claude",
        aliases: &[],
        display_name: "Claude",
        capabilities: SourceCapabilities {
            parser: true,
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
        capabilities: SourceCapabilities {
            parser: true,
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
        capabilities: SourceCapabilities {
            parser: false,
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
        capabilities: SourceCapabilities {
            parser: true,
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
        capabilities: SourceCapabilities {
            parser: true,
            passive_probe: true,
        },
        quality: UsageQuality::Precise,
        privacy: PrivacyClass::LocalArtifacts,
    },
    SourceDescriptor {
        kind: SourceKind::Grok,
        stable_id: "grok",
        aliases: &[],
        display_name: "Grok Build",
        capabilities: SourceCapabilities {
            parser: true,
            passive_probe: true,
        },
        quality: UsageQuality::TotalOnly,
        privacy: PrivacyClass::LocalArtifacts,
    },
];

pub fn registered_source_descriptors() -> &'static [SourceDescriptor] {
    SOURCE_DESCRIPTORS
}

pub fn source_descriptor(kind: SourceKind) -> Option<&'static SourceDescriptor> {
    SOURCE_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.kind == kind)
}

pub fn parse_source_id(value: &str) -> Option<SourceKind> {
    SOURCE_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.matches_id(value))
        .map(|descriptor| descriptor.kind)
}
