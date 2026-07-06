use std::process::{Command, Stdio};

use anyhow::Result;
use tracing::{debug, info, warn};

use crate::{
    app::AppContext,
    integrations::codex,
    models::SourceKind,
    store::{HolderKind, Store},
    util::now_utc,
};

use super::sync::{SyncRunOptions, run_once_with_options};

pub async fn run(app: &AppContext, source: SourceKind, trigger: &str, _auto: bool) -> Result<()> {
    /*
     * ========================================================================
     * 步骤1：记录 hook 信号并尝试拉起后台 worker
     * ========================================================================
     * 目标：
     * 1) 先写 trigger_state，确保信号不丢
     * 2) 拿到全局锁才真正做 sync
     * 3) 若运行期间又有新信号，当前 worker 自动补一轮
     */
    info!(source = %source, trigger, "开始处理 hook-run 信号");

    // 1.1 先落 trigger_state，再尝试拿全局 worker 锁
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let signaled_at = now_utc();
    store
        .triggers()
        .upsert_trigger_state(source, trigger, &signaled_at)?;
    #[allow(deprecated)]
    let Some(lock) = store.acquire_worker_lock()? else {
        return Ok(());
    };
    debug_assert_eq!(lock.meta().holder_kind, HolderKind::Hook.as_str());
    let heartbeat = lock.start_default_heartbeat();
    store
        .run_log()
        .recover_running_runs(&["sync", "hook-run"])?;

    // 1.2 当前 worker 按 snapshot 差异循环补跑
    let total_inserted = super::run_tracked(
        &store,
        "hook-run",
        async {
            let mut snapshot = store.triggers().trigger_snapshot()?;
            let mut total_inserted = 0usize;
            for _ in 0..3 {
                let started_at = now_utc();
                store
                    .triggers()
                    .mark_trigger_worker_started(source, &started_at)?;
                let attempt = run_once_with_options(
                    app,
                    &store,
                    0,
                    &SyncRunOptions {
                        source: Some(source),
                        ..SyncRunOptions::default()
                    },
                    None,
                )
                .await;
                let finished_at = now_utc();
                store
                    .triggers()
                    .mark_trigger_worker_finished(source, &finished_at)?;
                let summary = attempt?;
                total_inserted += summary.total_inserted;

                let next_snapshot = store.triggers().trigger_snapshot()?;
                if next_snapshot == snapshot {
                    break;
                }
                snapshot = next_snapshot;
            }
            Ok(total_inserted)
        },
        |inserted| Some(format!("hook source={source} inserted={inserted}")),
    )
    .await?;
    drop(heartbeat);
    drop(lock);

    info!(source = %source, inserted = total_inserted, "完成 hook-run 信号处理");
    chain_original_notify_if_needed(app, source)?;
    Ok(())
}

fn chain_original_notify_if_needed(app: &AppContext, source: SourceKind) -> Result<()> {
    if source != SourceKind::Codex {
        return Ok(());
    }

    let Some(original) = codex::original_notify(app)? else {
        return Ok(());
    };
    let current = crate::integrations::HookTarget::current(app).notify_args(source, "notify");
    if !codex::should_chain_original_notify(&current, &original) {
        debug!("跳过 Codex original notify chaining");
        return Ok(());
    }

    let Some((program, args)) = original.split_first() else {
        return Ok(());
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    match command.spawn() {
        Ok(_) => {
            debug!("已启动 Codex original notify chaining");
        }
        Err(err) => {
            warn!(error = %err, "Codex original notify chaining 启动失败");
        }
    };
    Ok(())
}
