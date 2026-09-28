# 总体设计

## 边界与事实来源

| 层 | 责任 | 权威事实 |
| --- | --- | --- |
| native source / parser | 数据是否可读、记录结构、usage 字段 | 原始 usage 元数据及独立 fixtures |
| inventory / preflight | 成员覆盖、发现失败、观察时间 | 本次成功的文件发现与已有 membership |
| SyncRunWriter | 原子 reset、去重、成本、bucket、cursor、marker | 现有 transaction / write fence |
| sync engine | 选择来源、legacy 跳过、重建权限、最终状态 | ValidatedSyncRequest 与 accounting marker |
| DTO / CLI / diagnostics | 分类、持久诊断、有限定位、输出边界 | parser/engine 的结构化结果 |

旧 source_file.live 是持久观察状态；来源失败时不执行 missing sweep，因此不是实时存在性证明。普通同步的 success 只表示编排完成。

## 三条改动路径

1. 诊断：将源级阻断从记录错误计数中分离。已有 JSON 持久容器增加可默认缺省的源级事实；保留老四类记录计数语义，审核所有“无错误”守卫。安全样本包含 code、source、有限定位与 offset；默认人类输出仅用 basename，不显示消息正文、完整路径或 path_hash。reason 与非零 offset 同显属于对 source-sync-contracts.md:502-505 的明确修订，禁止输出 hash 的规则保留。
2. Antigravity：发现与覆盖判断先于 usage 解码。所有选中产品已阻断时返回保留历史的明确状态；未知归属或存在跨产品影响时沿用完整核验。后续提交继续由现有原子组 writer 执行。
3. 写入：在既有事务边界内分段测量。先区分 provider mapping、BEGIN wait、reset、event/pricing/bucket、behavior、cursor/inventory、commit。只对实际热点实施优化；不重复建设已存在的批处理、临时 key 表或 prepared statement。

## 参考实现取舍

ccusage/tokscale 的坏记录继续读取、usage 结构校验、完整行 cursor、确定性缓存和去重测试可作为设计样本。不能照搬缺字段默认 0、Grok usage 与 fallback 混算、缺表当空库、mtime 作为请求时间或 tokscale 的 Antigravity token 字段映射。

tokscale Codex 的 append 路径仍可能哈希完整旧前缀，不保证降低本项目 I/O。ccusage/tokscale 不具有本项目相同的 SQLite bucket/behavior 写入链，不能从架构比较推出性能百分比。精确参考位置见 research/upstream-comparison.md。

## 兼容性与回滚

- 不新增数据库 schema，不直接向公开 ParseIssues / ParseIssueSample / SourceSyncStats / SourceSyncStatus 增加必填字段。使用私有持久化 DTO 与 crate-private 运行期结果携带新事实，保留现有公开返回类型和兼容入口；新 JSON 字段可缺省、样本有界。必须通过 cargo semver-checks --baseline-rev v1.2.0；若证明需要破坏公开 API 或迁移 schema，先修订计划。
- legacy repair 继续显式执行，缺失输入继续要求恢复或显式允许有损；规划不会替用户选择数据丢失。
- root / host / source 隔离、跨产品 copy 归属、bounded cursor、hook 历史、远端 marker 安全检查都是回归条件。
- 三个子任务各自提交可回滚的代码和测试。回滚优化不需要重建数据库或修改源文件。

## 风险与可验证性

源级诊断只改显示而未覆盖持久化会造成重启后信息丢失；只改计数而漏改认证守卫会产生数据安全回归。提前跳过未知归属会破坏 copied DB 处理。盲改 Claude append 会破坏项目级去重。基准噪声或数据库初态不同会产生错误性能结论。各风险均映射到子任务 AC 和父任务 AC1–AC7。
