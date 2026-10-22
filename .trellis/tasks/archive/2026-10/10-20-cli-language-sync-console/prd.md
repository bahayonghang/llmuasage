> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# S：把CLI日志和sync进度/重建确认/诊断统一双语并格式化，完成统计升级为准确的十列线框表。

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：已实施并通过完整 Rust 门禁；独立验收见 `research/check-evidence.md`。** 前置：10-20-cli-language-foundation。Trellis父子关系不自动调度这些依赖。

## 目标与范围

把CLI日志和sync进度/重建确认/诊断统一双语并格式化，完成统计升级为准确的十列线框表。

R3-R8；父AC5-AC10，C07/C09/C22及其他CLI tracing输出；给C的logs人读重放提供投影接口。

## 文件责任

- `src/runtime/logging.rs`
- `src/commands/entrypoint.rs`（仅接入 CLI 专用 logger 初始化）
- `src/common/i18n/mod.rs`（仅登记 S 自有目录模块）
- `src/common/i18n/logging.rs`
- `src/common/i18n/sync.rs`
- `src/commands/sync.rs`
- `src/commands/sync_progress.rs`
- `src/commands/sync_summary.rs`
- `src/commands/sync_recovery.rs`
- `src/sync/engine.rs`
- `src/sync/notices.rs`
- `src/sync/mod.rs`
- `src/parsers/antigravity.rs`
- `src/parsers/mod.rs`
- `tests/sync/progress_io.rs`
- `tests/sync/runtime/progress_io.rs`
- `tests/sync/main.rs`

其他src目录仅允许为现有tracing发射点补稳定event_code/必要中立参数，不修改业务顺序或UI文案；这些文件随后交C/T继续迁移。file writer/filters/public wire与Web默认logger保持。

## 验收标准

- [x] S-AC1：同一pricing/legacy及各级app-owned事件分别生成en/zh终端说明，默认无完整哈希堆积，WARN/ERROR不丢；文件原字段/过滤/计数/轮转不变。
- [x] S-AC2：全部progress line/bar、bootstrap/migration/pricing/source/remote/fail/cancel文案配对；slow WARN不降级、不重复永久进度。
- [x] S-AC3：typed notice覆盖legacy、coverage、keep/accept/blocked、retained、remote/source failure、取消；公开SyncSummary/SyncEvent/Options形状不变。
- [x] S-AC4：英中k/K/r/R、空/EOF/read failure、首次及二次非法输入与三流TTY/flags的结果一致；missing与out_of_scope不混淆、unreadable不绕过。
- [x] S-AC5：十列边框在两语言display width正确，零/缺失/大整数/总数/单位与旧投影一致，无数值裁切；表后来源诊断最多8样本、omitted与隐私正确。
- [x] S-AC6：同fixture有/无feedback回调的usage/cursor/accounting/事务结果相同，仅已知动态时间/run_id可规范化；不新增逐文件查询或重扫。
- [x] S-AC7：stdout/stderr与JSON/NDJSON/shards隔离；警告不串进progress，取消/早退guard恢复，所有无色模式无ANSI。logs重放能翻译新id并保留旧raw。

## 范围外与风险

最危险是把翻译写回持久化状态/协议、回调改变事务、缩窄数值或误改恢复确认。回滚只回展示与私有feedback，不真实重建或降级数据版本。

不新增第三语言/热切换，不修改Web/desktop语言、token/query/sync算法，不操作真实usage或凭据，不安装/提交/推送/发布。没有阻塞产品决策；技术机制见design，执行前仍需用户明确解除不实现限制。
