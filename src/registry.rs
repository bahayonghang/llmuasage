//! 集中式源注册表。
//!
//! "系统支持哪些源" 由这一处工厂决定。增/删源时只改本文件即可，
//! 三个上层 fan-out（`integrations::probe_all` / `integrations::install_all` /
//! `integrations::uninstall_all`、`commands::sync::run_once`）会自动跟随。
//!
//! Deletion-test：删掉本模块 → `commands/sync.rs` 与 `integrations/mod.rs`
//! 必须重新硬列三连，新增第四个源会让两处 fan-out 各加一行。

use crate::{
    domain::{
        platform_monitor::{self, PlatformMonitorDescriptor},
        source_descriptor::{self, SourceDescriptor},
    },
    integrations::{
        Integration, antigravity::AntigravityIntegration, claude::ClaudeIntegration,
        codex::CodexIntegration, opencode::OpencodeIntegration,
    },
    parsers::{ClaudeParser, CodexParser, KimiCodeParser, OpencodeParser, PiParser, SourceParser},
};

/// 工厂：当前 build 支持的所有 sync parser。
pub fn registered_parsers() -> Vec<Box<dyn SourceParser>> {
    vec![
        Box::new(CodexParser),
        Box::new(ClaudeParser),
        Box::new(OpencodeParser),
        Box::new(KimiCodeParser),
        Box::new(PiParser),
    ]
}

/// Current build's source descriptors in stable CLI/display order.
pub fn registered_source_descriptors() -> &'static [SourceDescriptor] {
    source_descriptor::registered_source_descriptors()
}

/// Current build's source and monitor-only platform descriptors.
pub fn registered_platform_monitors() -> &'static [PlatformMonitorDescriptor] {
    platform_monitor::registered_platform_monitors()
}

/// Look up a descriptor by stable source kind.
pub fn source_descriptor(kind: crate::models::SourceKind) -> Option<&'static SourceDescriptor> {
    source_descriptor::source_descriptor(kind)
}

/// Parse a stable source id or descriptor alias.
pub fn parse_source_id(value: &str) -> Option<crate::models::SourceKind> {
    source_descriptor::parse_source_id(value)
}

/// 工厂：当前 build 支持的所有本地集成。
pub fn registered_integrations() -> Vec<Box<dyn Integration>> {
    vec![
        Box::new(CodexIntegration),
        Box::new(ClaudeIntegration),
        Box::new(OpencodeIntegration),
        Box::new(AntigravityIntegration),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::models::SourceKind;

    use super::*;

    fn parser_sources() -> Vec<SourceKind> {
        registered_parsers()
            .into_iter()
            .map(|parser| parser.source())
            .collect()
    }

    fn integration_sources() -> Vec<SourceKind> {
        registered_integrations()
            .into_iter()
            .map(|integration| integration.source())
            .collect()
    }

    #[test]
    fn descriptors_cover_parser_and_integration_registries() {
        let descriptor_sources = registered_source_descriptors()
            .iter()
            .map(|descriptor| descriptor.kind)
            .collect::<Vec<_>>();

        let parser_sources = parser_sources();
        let registered_integration_sources = integration_sources();

        for parser_source in &parser_sources {
            assert!(
                descriptor_sources.contains(parser_source),
                "parser source {parser_source} missing descriptor"
            );
        }
        // A passive-only source (e.g. Kimi Code) legitimately has a descriptor
        // and a parser without an integration, so the invariant is that every
        // integration source has a descriptor — not strict set equality.
        for integration_source in &registered_integration_sources {
            assert!(
                descriptor_sources.contains(integration_source),
                "integration source {integration_source} missing descriptor"
            );
        }
    }

    #[test]
    fn descriptors_keep_stable_ids_and_aliases_unique() {
        let mut ids = BTreeSet::new();

        for descriptor in registered_source_descriptors() {
            assert_eq!(descriptor.stable_id, descriptor.kind.as_str());
            assert!(
                ids.insert(descriptor.stable_id),
                "duplicate source id {}",
                descriptor.stable_id
            );
            for alias in descriptor.aliases {
                assert!(
                    ids.insert(alias),
                    "duplicate source alias/id {alias} on {}",
                    descriptor.stable_id
                );
                assert_ne!(*alias, descriptor.stable_id);
            }
        }
    }

    #[test]
    fn source_id_parsing_uses_descriptors() {
        assert_eq!(parse_source_id("codex"), Some(SourceKind::Codex));
        assert_eq!(parse_source_id("claude"), Some(SourceKind::Claude));
        assert_eq!(parse_source_id("opencode"), Some(SourceKind::Opencode));
        assert_eq!(
            parse_source_id("antigravity"),
            Some(SourceKind::Antigravity)
        );
        assert_eq!(parse_source_id("kimi_code"), Some(SourceKind::KimiCode));
        assert_eq!(parse_source_id("pi"), Some(SourceKind::Pi));
        assert_eq!(parse_source_id("gemini"), None);
        assert_eq!(parse_source_id("missing"), None);
    }

    #[test]
    fn platform_monitors_cover_registered_sources() {
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
    fn declared_capabilities_match_current_registries() {
        let parser_sources = parser_sources().into_iter().collect::<BTreeSet<_>>();
        let integration_sources = integration_sources().into_iter().collect::<BTreeSet<_>>();

        for descriptor in registered_source_descriptors() {
            assert_eq!(
                descriptor.capabilities.parser,
                parser_sources.contains(&descriptor.kind),
                "parser capability drift for {}",
                descriptor.stable_id
            );
            assert_eq!(
                descriptor.capabilities.integration,
                integration_sources.contains(&descriptor.kind),
                "integration capability drift for {}",
                descriptor.stable_id
            );
        }
    }
}
