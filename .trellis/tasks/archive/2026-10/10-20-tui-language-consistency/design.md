> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# T 技术设计

## 2026-10-21 review integration decisions

The CLI startup already resolves the profile language and emits any settings
diagnostic. Its dash adapter passes that captured language to a crate-private
terminal entry, which establishes the synchronous UI scope without rereading
settings. Public direct terminal/dashboard wrappers still resolve their Store
root once before raw mode. This avoids duplicate diagnostics and snapshot drift.

The registry observer must neither block nor discard notices. Existing bounded
event queues provide backpressure; the UI drains at most their 128-event batch
before consuming neutral facts, and removes remote facts at the matching skipped
event. This bounds in-flight work rather than retaining every configured host.
Public event delivery, job scheduling and sync decisions remain unchanged.

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：10-20-cli-language-foundation。Trellis父子关系不自动调度这些依赖。

## 架构与数据流

1. dash/tui从已解析store.paths.root_dir获得同一偏好，进入raw-mode前绑定语言；主event/render线程固定locale scope覆盖actions/status与draw，退出恢复。旧publicAppState/函数不增必填字段；无热更新/轮询。
2. T01-T10逐项迁移，包括短标签、time window、footer各种分支、period details、quota diagnostics、ready/watch/critical、monitor-only与x/y同步结果。Panel/TableSortKey/source/status/theme id不翻译；只投影显示文本。
3. nav_bar render与panel_at_position共享同一language标签和Unicode display width；排序逻辑用稳定key，数字/选中行不变。无需新增尺寸/DPI矩阵。
4. 现有quota Diagnostic没有code且绝对left被拼进String；按research/tui-localization.md在生产处形成private QuotaPresentationHints，内部fetch返回原outcome+hints，publicfetch_all不变。私有cache可加可选presentation，绑定report指纹，旧cache仍加载，rawreport/TTL/cache_hit不变。
5. TUI private session携带hints，在frame scope消费；fresh/live一致，旧cache/第三方unknown明确原始detail。绝对remaining/limit沿用原数值，禁止从百分比推算。metadata不含locale/凭据/token/responsebody。
6. 工作线程只返回中立数据/事件，UI线程翻译；保留generation拒旧结果、dirty redraw/250ms tick、可见行window、theme/NoColor、quota r/R/m及JobRegistry x取消/退出。

## 2026-10-21 源码核对后的最小边界补充

- 额度控制器当前只有取消 token，没有 generation。允许新增私有请求标签，仅用于把 report 与 hints 作为同一结果接受并拒绝已被替代的队列结果；这是新保护，不能写成原有 quota generation 行为。请求触发、缓存旁路、取消和轮询策略不扩展。
- Behavior 的 support.level、CompareMetric.id 等既有稳定事实用于标准说明投影，不解析 reason/warning 字符串。
- OptimizeFinding 的 title/evidence/recommendation 是应用自有文本；现有 DTO 丢失了 evidence 的原数值参数。允许 query/optimize.rs 在原 detector 决策点追加 crate-private 中立 facts，经同一核心的 optimize_with_presentation 返回。公开 optimize/DTO/序列化与 canonical 字符串完全不变；SQL、阈值、排序、分数、舍入与 savings 算法不改，不重复查询。data_loader 私有 envelope 将 hints 与既有 generation/filter/window 共同传递并接受，公开 AppState 不增必填字段。
- 旧 English-only 源码扫描测试依据用户明确双语范围替换为有效的成对覆盖；不能用 Unicode 转义绕过旧测试。颜色/theme 守卫继续保留。
- 实施交接来源：../10-20-cli-language-command-output/research/tui-implementation-handoff.md。UI 与 quota 实施者必须先冻结私有 result/hint scope 接口，再按互不重叠责任编辑。
- T10 另经源码确认：JobRegistry 在 JobRunOutcome::Failed 处先把 anyhow 错误转为 String，公开 SyncEvent/JobSnapshot 不再携带 SyncFailure.kind；仅翻译 UI 前缀会漏掉自有安全拒绝理由。允许 sync/job_registry.rs 增加 crate-private、同次失败时调用的可选 observer，TUI 控制器在转 String 前提取语言中立事实并按 job id 关联保存；默认/new 公共入口、公开事件/snapshot/错误/持久化内容和取消/任务/锁顺序均保持。observer 不执行 UI 渲染或读语言，不重跑任务，不增加全局最近错误缓存。若 bootstrap 价表 facts 需一并传递，只通过现有 private PricingResult 核心携带原错误旁的可选事实，canonical 错误仍原样存储；不得把相同 canonical 的新 Error source 包装重复写入 run log。其他共享层改动先报告确切缺口。

## T 实施分工与 S 消费者整合

C 独立检查与完整 Rust 门禁（1293 PASS / 19 原有 ignored）后启动 T。

- UI owner：tui 除 report_table.rs、quota.rs、panels/usage.rs 外的界面与事件循环，common/i18n/tui.rs，query/optimize.rs 的已批准私有 hints，普通 TUI 回归。
- Quota owner：subscription 全部、tui/quota.rs、tui/panels/usage.rs、独立 common/i18n/quota.rs 与额度回归。共享返回 report/outcome 与 hints，frame scope 只在匹配报告指纹时使用。
- Shared-sync owner：sync/engine.rs、notices.rs、job_registry.rs 及必要 default.rs 私有接线，common/i18n/sync.rs，commands/sync.rs/sync_progress.rs/sync_summary.rs 与相应回归。保留 public 默认 executor/registry 签名、canonical SyncEvent/JobSnapshot/run/host 数据；TUI 仅消费中立 observer，不能 import commands。
- Coordinator：shared common/i18n/mod.rs、tests/tui/main.rs 注册、规范与整合门禁。各 owner 不能修改另一 owner 源码；先协商私有接口。

S 复核确认 engine 仍用 public importer 后将 canonical error 放入 RemoteHostSkipped.reason 与 RemoteSkipped.detail，progress 和表后诊断会直接显示应用自有英文。Shared-sync owner 在原调用处消费 C 已验证的 import_with_facts，公开 event/host/skipped-map 保持原文；RunNotice 私有事实和每个 renderer/任务的有界同调用上下文供人读投影，未知外部详情明确保留。警告 canonical tracing 不改。若需要 default executor 的私有 feedback 入口，只委托同一 locked core，不重获锁或复制同步算法。新私有 observer/通知接口必须先发给 UI owner，不允许任何 process-global 最近 facts 缓冲。

## 兼容与接口

父design是共享接口真源；新公开类型/必填字段、schema/wire/accounting变更不在范围。人读文本从语言目录投影，数据/代码/用户值保留；原错误source、machine分支与既有退出码不变。共享修改需遵循上述文件责任，不能回退其他child的工作。

## 关键依据

- 父目录 `research/surface-coverage.md` 是完整覆盖与归属清单。
- 精确源码事实与相关合同在本任务implement/check清单的research/spec中，执行前核对未漂移。
- 所有功能验证当前NOT_RUN；不把规划或context校验当成实现证据。

## 回滚与限制

最危险是只翻译tab漏掉dialogs/worker字符串、中文点错导航、变更quota公开payload或从百分比造绝对余额。可选cache元数据必须兼容缺失/旧reader，不改凭据或触公网。

不做数据/凭据/历史日志语言迁移。native验收独立于headless；不建立尺寸/DPI/断点矩阵。
