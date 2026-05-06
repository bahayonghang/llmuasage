//! 集中式源注册表。
//!
//! "系统支持哪些源" 由这一处工厂决定。增/删源时只改本文件即可，
//! 三个上层 fan-out（`integrations::probe_all` / `integrations::install_all` /
//! `integrations::uninstall_all`、`commands::sync::run_once`）会自动跟随。
//!
//! Deletion-test：删掉本模块 → `commands/sync.rs` 与 `integrations/mod.rs`
//! 必须重新硬列三连，新增第四个源会让两处 fan-out 各加一行。

use crate::{
    integrations::{
        Integration, claude::ClaudeIntegration, codex::CodexIntegration,
        opencode::OpencodeIntegration,
    },
    parsers::{ClaudeParser, CodexParser, OpencodeParser, SourceParser},
};

/// 工厂：当前 build 支持的所有 sync parser。
pub fn registered_parsers() -> Vec<Box<dyn SourceParser>> {
    vec![
        Box::new(CodexParser),
        Box::new(ClaudeParser),
        Box::new(OpencodeParser),
    ]
}

/// 工厂：当前 build 支持的所有本地集成。
pub fn registered_integrations() -> Vec<Box<dyn Integration>> {
    vec![
        Box::new(CodexIntegration),
        Box::new(ClaudeIntegration),
        Box::new(OpencodeIntegration),
    ]
}
