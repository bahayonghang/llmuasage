use crate::common::i18n::commands::message;
use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, commands::source_status, query::Dashboard, store::Store};

pub async fn run(app: &AppContext) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：读取本地统计与被动来源摘要
     * ========================================================================
     * 目标：
     * 1) 输出 DB 路径、bucket 数和最近同步时间
     * 2) 汇总来源层与项目层的用量
     * 3) 展示来源状态与最近失败
     */
    info!(event_code = "commands.status.01", "开始输出状态摘要");

    // 1.1 读取概览、来源和健康信息
    let store = Store::new(&app.paths)?;
    store.require_initialized()?;
    let dashboard = Dashboard::open(&store)?;
    let overview = dashboard.overview(&Default::default())?;
    let sources = dashboard.source_breakdown(&Default::default())?;
    let health = dashboard.health()?;
    let mut capability_statuses = source_status::build_source_capability_statuses(&sources);
    source_status::apply_token_accounting_statuses(&store, &mut capability_statuses)?;
    let platform_statuses = source_status::build_platform_monitor_statuses_with_facts();
    let parse_issues = store
        .sync_status()
        .load_source_sync_statuses("local")?
        .into_iter()
        .map(|status| (status.source, status.parse_issues))
        .collect::<std::collections::BTreeMap<_, _>>();
    let lock = store.current_worker_lock()?;

    // 1.2 打印人读摘要
    println!("{}", message!(StatusTitle));
    println!("{}", message!(StatusDb, app.paths.db_path.display()));
    println!("{}", message!(StatusBuckets, overview.bucket_count));
    println!(
        "{}",
        message!(
            LastSync,
            overview
                .last_sync_at
                .as_deref()
                .unwrap_or(crate::common::i18n::text("never", "从未"))
        )
    );
    println!(
        "{}",
        message!(
            LastExport,
            overview
                .last_export_at
                .as_deref()
                .unwrap_or(crate::common::i18n::text("never", "从未"))
        )
    );
    for source in sources {
        println!(
            "{}",
            message!(
                SourceTotal,
                source.source,
                source.total_tokens,
                source
                    .last_event_at
                    .as_deref()
                    .unwrap_or(crate::common::i18n::text("never", "从未"))
            )
        );
    }
    source_status::print_human_statuses_for_host(
        &store,
        crate::store::LOCAL_HOST_ID,
        &capability_statuses,
        &[],
        &parse_issues,
        &Default::default(),
        &Default::default(),
    )?;
    source_status::print_platform_monitor_statuses(&platform_statuses);
    if let Some(lock) = lock {
        println!(
            "{}",
            message!(LockHeld, lock.holder_identity(), lock.lease_expires_at)
        );
    } else {
        println!("{}", message!(LockIdle));
    }
    if let Some(run) = health.recent_failures.first() {
        println!(
            "{}",
            message!(RecentError, run.command, run.error.as_deref().unwrap_or(""))
        );
    }

    info!(event_code = "commands.status.02", "完成状态摘要输出");
    Ok(())
}
