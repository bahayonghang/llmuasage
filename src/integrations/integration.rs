use anyhow::Result;

use crate::{app::AppContext, models::SourceKind, store::Store};

use super::{IntegrationAction, IntegrationProbe};

/// Local third-party integration (Codex `notify`, Claude hooks, OpenCode plugin, …).
///
/// 把 "系统支持哪些 integration" 收敛到 `crate::registry::registered_integrations()`
/// 的单点工厂。新增源时只在工厂处加一行；`probe_all` / `install_all` /
/// `uninstall_all` 自动 fan-out。
pub trait Integration: Send + Sync {
    /// Source this integration manages — used by `probe_all` / `install_all`
    /// to tag actions when the underlying call returns an error.
    fn source(&self) -> SourceKind;

    /// Inspect the integration's current configuration, never mutating it.
    fn probe(&self, app: &AppContext) -> Result<IntegrationProbe>;

    /// Install or refresh the integration so the local hook wrapper is wired.
    fn install(&self, app: &AppContext, store: &Store) -> Result<IntegrationAction>;

    /// Restore the integration's configuration to a state without llmusage hooks.
    fn uninstall(&self, app: &AppContext, store: &Store) -> Result<IntegrationAction>;
}
