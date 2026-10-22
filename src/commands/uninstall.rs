use super::cli_errors::Diagnostic;
use crate::common::i18n::commands::{OperationDiagnostic, cleanup_detail, message, status_label};
use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, integrations, store::Store};

pub async fn run(app: &AppContext, purge: bool) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：清理旧版 hook / plugin 遗留，并按需清理本地目录
     * ========================================================================
     * 目标：
     * 1) 恢复 Codex notify，并摘除 Claude/Antigravity hooks、OpenCode plugin
     * 2) 仅在实际清理或失败时记录 integration_install 审计
     * 3) 只有 --purge 才删除 ~/.llmusage
     */
    info!(event_code = "commands.uninstall.01", "开始执行本地卸载");

    let store = Store::new(&app.paths)?;
    super::cli_errors::pricing_result(store.bootstrap_with_pricing_facts())?;
    let actions = super::run_tracked(
        &store,
        "uninstall",
        async {
            integrations::cleanup_all_with_facts(app, &store).map_err(|error| {
                if let Some(failure) = error.downcast_ref::<integrations::CleanupFailure>() {
                    Diagnostic::with_facts(
                        error.to_string(),
                        OperationDiagnostic::CleanupFailed {
                            failures: failure.facts.clone(),
                        },
                    )
                    .into()
                } else {
                    error
                }
            })
        },
        |_| Some("legacy hook cleanup completed".to_string()),
    )
    .await?;

    println!("{}", message!(CleanupFinished));
    for outcome in actions {
        println!(
            "- {}: {} ({})",
            outcome.action.source,
            status_label(&outcome.action.status),
            cleanup_detail(&outcome.fact)
        );
    }

    if purge && app.paths.root_dir.exists() {
        std::fs::remove_dir_all(&app.paths.root_dir)?;
        println!("{}", message!(Purged, app.paths.root_dir.display()));
    }

    info!(event_code = "commands.uninstall.02", "完成本地卸载");
    Ok(())
}
