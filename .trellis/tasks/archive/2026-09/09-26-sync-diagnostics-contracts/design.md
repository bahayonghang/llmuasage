# 诊断契约设计

## 所属层与最小改动

源数据含义由 parser 提供，domain 诊断结构负责分类与序列化，sync engine 保留失败安全判断，CLI 只格式化。改动候选是 src/domain/models.rs、src/parsers/antigravity.rs、src/parsers/grok.rs、src/commands/sync_summary.rs、source_status.rs、sync_progress.rs 及现有诊断存储适配。

沿用 parse_issues_json 存储容器。引入私有持久化 DTO，flatten 现有 ParseIssues 字段并增加默认缺省的有界 source_issues；不直接增加公开 ParseIssues 或 SourceSyncStatus 的必填字段。Antigravity family 与 crate-private drive_with_rebuild 内部结果携带源级事实，现有公开 drive/drive_with_events 与 SourceParser trait 保持返回类型和兼容委托。engine 统一把同一轮来源状态与附加诊断送入 store。保留运行时 last_error 对不完整库存 sweep 的保护。不得只停止 record(Malformed) 而丢失故障持久化。

store 以私有 DTO 统一读写既有列，通过兼容的附加访问器向内部展示提供源级诊断；query/diagnostics 的独立 SELECT/deserialize 入口也必须接入。当前二进制所有状态更新路径都应保留本轮 source_issues，成功新轮明确清除旧阻断；不能经过旧 typed serialization 静默丢弃。旧 JSON 无该字段时读取为空。ParseIssues::total() 仍只统计 malformed+oversized，informational_total() 仍为 skipped+accounting，四类合计用于省略提示；源级阻断另行判定。

逐一追踪 doctor、query、source-status、sync engine 及 remote importer 的“无错误”判定。旧 ParseIssues::total() 不能成为新增阻断的唯一判定；新源级错误也必须阻止 marker certification。新增私有 DTO/helper 仅服务现有持久诊断和安全检查，不新增通用错误框架。运行期 source issues 不通过解析 last_error 自由文本或复用虚假记录样本传递。

## 定位与隐私

reason 使用稳定枚举/code，展示文本由 formatter 生成。保持公开 sample 的现有字段，修正 Grok 记录位置使用真实 sidecar 文件 hash；事件/session/reset 的目录 hash 不变。样本仍为 8 条。reason 与非零 offset 独立渲染，明确更新 source-sync-contracts.md:502-505 的显示契约。

默认人类摘要只显示安全 basename、source、非零 offset 与 reason，禁止打印完整或短 path_hash。basename 相同的样本可按样本顺序与既有显式诊断导出中的内部 hash 关联；旧 session-hash 样本若无法准确反查，应明确定位不可用，不能猜测文件。保留现有样本隐私边界，交互 dashboard 继续仅暴露计数与安全状态。

实施审查确认既有 offset 字段含不同单位：JSONL 为字节偏移，ZCode 为 completed_at 或 started_at 毫秒时间戳，OpenCode 为私有 rowid。保留字段和值，按来源显示 JSONL `@` 与 ZCode `timestamp_ms=`；OpenCode 不显示 rowid，遵守既有隐私契约。Antigravity 行诊断当前 offset 为 0。

Codex >4 MiB 的合法非 usage 记录跳过保留为可解释诊断；Grok usageIsIncomplete 保留为 accounting quality 信息。不能通过过滤警告改变处理事实。

## 输出边界

先在隔离测试重现 renderer 与 tracing/eprintln 的组合。让同一命令的用户可见警告遵守现有进度渲染生命周期；避免新增依赖或修改全局 logger 语义。JSON stdout 必须与人类警告分开。无法重现时记录原因未查明并保留对应验收未完成，不能仅凭补换行宣称修复。

## 兼容性

无 schema migration；旧 JSON 无新字段时按空源级诊断读取；新 JSON 保存有限 code/数量/安全定位。远端和状态消费者接受附加字段，老字段意义不变。旧版二进制可能丢弃未知诊断字段，回滚只影响附加诊断保留，不改变使用历史。新增公共方法须为兼容性增加，不能破坏旧方法签名、公开 struct literal 或 exhaustive enum match；执行项目 semver 门禁。

## 已批准的 2.0 API 边界（2026-09-28）

用户对具体版本方案回复“都按照你爱推荐的来即可”，选择推荐的 2.0.0 开发树版本边界。按 `research/semver-remediation.md` 同步 13 个版本入口并提供双语迁移说明。既有来源、host、成本和 CLI 行为保持，诊断改动继续保持现有公开类型结构。

正式命令仍为 `cargo semver-checks --baseline-rev v1.2.0`，不抑制检查、不传 release-type、不移动基线。major 下通过表示已声明正确的版本边界，不表示恢复 v1.2.0 源码兼容。SQLite schema、source accounting 和 remote wire 版本不随 crate major 改变。该授权不包含 tag、发布、安装或真实数据重建。
