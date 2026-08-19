# 规划审阅修正证据

## 现场复核

- 安装版本：`llmusage 1.2.0`。
- `llmusage logs --limit 40` 的最新 usage-import 是 `#178 [success] sync`，完成时间 `2026-08-19T08:58:21Z`；截图中 12:39 的 JobRegistry 同步没有 run row。
- `llmusage diagnostics` 显示所有来源 `recent_completed_at=2026-08-19T12:39:53Z`；Claude 为 `missing_file_count=728`、`protected_event_count=36495`、`lossy_rebuild_risk=true`。
- 这证明同步数据已经更新而命令审计仍旧；诊断只能证明源文件当前不存在，不能证明由哪个程序或用户删除。

## 已确认的代码边界

- `src/sync/job_registry.rs:404-484`：JobRegistry 取得围栏锁并调用 executor，但不调用 run_log；锁等待期取消在创建任何 run row 前返回。
- `src/store/run_log.rs:7-9`：`USAGE_IMPORT_COMMANDS` 已包含 `sync`、`sync --rebuild`、`hook-run`；`src/commands/sync.rs:115-118,244-247,380-383` 的手写恢复列表遗漏 `sync --rebuild`。
- `src/commands/mod.rs:222-268`：CLI `run_tracked` 把成功路径的 `finish_run` 失败作为命令失败返回；Job 设计不能把同类失败降级为旁路日志错误。
- `src/web/assets/render/sync-command-center.js:43-78`：overlay 缺少 completed 分支；`:220-240` 的 current job 详情没有渲染 `finished_at`；`:261-303` 的 secondary status 也没有 completed 分支。
- `src/web/assets/app.js:1770-1787`：terminal snapshot 先渲染，completed 后 reload，finally 再清空活动快照；无闪回测试必须覆盖这三个连续状态。
- `src/tui/panels/sync_status.rs:85-100,264-299`：TUI 直接消费共享 tone/headline/reason/source status，并根据 `lossy_rebuild_risk` 强制 warning row；共享 query 语义变化必然影响 TUI。
- `src/web/mod.rs:554,1208-1267`：现有本地写入口 `POST /api/diagnostics/forget` 已存在；规划不得声称只有 CLI forget。

## 现有规范约束

- `.trellis/spec/llmusage/backend/source-sync-contracts.md:148-158`：命令中心目前把 rebuild risk 排在 failed 之后，并定义三个 usage-import command；本任务需同步更新该权威契约。
- `.trellis/spec/llmusage/backend/write-fencing-contracts.md:30-37,46-51`：run-log 写入必须使用有效 fenced Store；丢失 permit 后不能通过无界重试恢复写权限。
- `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md:440-448`：终态 hook 负责 diagnostics cache 失效，post-sync 必须走 interactive reload。该文件超过上下文注入上限，因此清单引用本研究摘要，实施者仍按 `implement.md` 前置阅读完整原文。
- `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md:114-127`：ARCH-002 禁止 `src/sync/** -> crate::commands/**`，共享逻辑必须放在 sync/store 层。

## 审阅后决策

1. 不新增 Web 专用 query 投影；Web 与 TUI 共享中性重建保护语义。
2. store 提供唯一 usage-import stale recovery 入口，覆盖三个命令。
3. run-log start/finish 失败不得产生 completed JobSnapshot。
4. 锁前取消无 run row；执行期取消持久化为 `cancelled`，且不计健康失败。
5. completed overlay、重载 payload、清空 overlay 三阶段必须用可观察 DOM 回归证明无旧状态闪回。
