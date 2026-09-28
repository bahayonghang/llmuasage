# Sync 诊断分类与定位契约

## 目标

让用户准确区分来源被阻断、记录无法解析、有意跳过和 usage 不完整，并能定位问题、理解样本总数。父任务：09-26-sync-diagnostics-performance；覆盖 R1/R2/R3/R5/R7/R8。

## 事实

Antigravity 缺成员目前由 record_failure 计为 malformed；自由文本清洗损失语义。Codex/Grok 总计各 13、仅保留 8 个样本。Grok 有 reason 时摘要不显示 offset，session hash 又无法按文件 hash 解析 basename。用户日志存在 LockAcquired 与 WARN 混行；具体竞态尚未复现。

## 需求

- 源级不可用有独立稳定 code、数量、观察范围/时间和可操作提示，不能增加 malformed 行计数。
- 不丢失持久化来源故障；旧 JSON 缺新字段仍正确读取。
- code、offset、来源和安全文件定位同时存在；样本显示总计和省略数。
- 保留有界记录读取和原生不完整 usage 的保守处理，不静默补零或降级为无问题。
- TTY/non-TTY 警告不破坏行边界，JSON 模式 stdout 保持 NDJSON。

## 验收

- [x] D1（R1/R3）：缺成员 fixture 的 malformed=0，独立来源阻断准确且重启后仍可见；无法认证为干净成功。
- [x] D2（R2）：13 个同类问题最多显示 8 个样本，并明确显示 5 个未展示；reason 不覆盖 offset，定位关联正确，控制字符/正文/完整私有路径/path_hash 不泄露。
- [x] D3（R2）：4 MiB 以上非 usage 记录仍有界跳过；大 token_count 读取与 EOF/cursor 原契约不变；Grok usageIsIncomplete 保留已提供数值与警告，禁止混入 fallback。
- [x] D4（R3/R5）：本次诊断改动保留现有公开 Rust struct literal；用户已选择 2.0.0 边界承接既有公开 API 破坏，迁移说明完整且正式 semver 门禁通过；旧诊断 JSON round-trip、source-status、diagnostics、doctor、远端 header/trailer 与 marker 守卫通过；区分库存时点与本轮观察。
- [x] D5（R7）：验证连续与交错 warning/LockAcquired，TTY/non-TTY 输出可读，NDJSON 每行可解析；现有退出码语义不变。

## 范围外与依赖

不修改 token 算法、不扩大样本上限、不增新来源、不重建数据、不把 warning 全部升级为退出失败。不依赖其余子任务；Antigravity 预检子任务消费本任务的诊断契约。

## 未知项

混行已在隔离 indicatif 终端适配中复现：永久输出填充到终端宽度后，原始 warning 可接到同一行。实现使用 suspend 边界与完整换行写入；独立检查和最终门禁仍需完成。源级 JSON 的最小字段由设计约束；若必须改 schema，返回父任务。

## 已批准的范围扩展

用户要求先修复既有门禁再继续。追加三个时区相关 lib 失败和 v1.2.0 基线的公开 API 差异处置；实施与验证清单见 implement.md 的 G1–G3。原诊断改动继续保持既有公开类型兼容，公开 API 的既有破坏单独记录版本/兼容性决策。现有来源身份、host 隔离、cost 和数据保护行为保持有效。
