use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, models::SourceKind, store::Store, util::now_utc};

use super::sync::run_once;

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
    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let signaled_at = now_utc();
    store.upsert_trigger_state(source, trigger, &signaled_at)?;
    let Some(_lock) = store.acquire_worker_lock()? else {
        return Ok(());
    };

    // 1.2 当前 worker 按 snapshot 差异循环补跑
    let run_id = store.record_run_start("hook-run")?;
    let mut snapshot = store.trigger_snapshot()?;
    let mut total_inserted = 0usize;
    for _ in 0..3 {
        let started_at = now_utc();
        store.mark_trigger_worker_started(source, &started_at)?;
        let summary = run_once(app, &store)?;
        total_inserted += summary.total_inserted;
        let finished_at = now_utc();
        store.mark_trigger_worker_finished(source, &finished_at)?;

        let next_snapshot = store.trigger_snapshot()?;
        if next_snapshot == snapshot {
            break;
        }
        snapshot = next_snapshot;
    }

    let summary = format!("hook source={source} inserted={total_inserted}");
    store.finish_run(run_id, "success", Some(&summary), None)?;
    info!(source = %source, inserted = total_inserted, "完成 hook-run 信号处理");
    Ok(())
}
