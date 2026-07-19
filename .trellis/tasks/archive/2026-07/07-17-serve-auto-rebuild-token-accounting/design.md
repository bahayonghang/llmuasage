# Technical Design

## Overview

在 `commands::serve` 的 bootstrap 与 Web 端口绑定之间增加一个 token accounting repair 阶段。该阶段从 parser registry 获取稳定顺序的源，只处理具有历史行但缺少当前 accounting marker 的源。

## Boundaries

- `commands::sync` 继续拥有 legacy 判定、lossy rebuild guard、reset、parser 执行和 marker 推进协议。
- `commands::serve` 只负责启动时协调：发现目标、预检风险、逐源调用现有 rebuild 入口、汇总 repair/blocked 结果。
- `Store` 不自动执行外部源重建。`bootstrap` 仍只负责本地 schema/pricing 初始化。
- `web::serve` 不感知迁移；只有 repair 阶段结束后才绑定端口。
- 无 source 的 command-level rebuild 不再调用无条件全表 reset；它按 parser registry 逐源调用 `Store::reset_for_source`，使删除集合与 reimport 集合一致。

## Proposed Interfaces

在 `commands::sync` 提供 crate 内可复用的查询函数：

```rust
pub(crate) fn legacy_token_accounting_sources(store: &Store) -> Result<Vec<SourceKind>>
```

它复用 `registered_parsers()` 的稳定顺序，并只返回 `Store::has_legacy_token_accounting` 为真的源。普通 sync guard 也复用同一 parser-source 枚举逻辑，避免 detection 逻辑分叉。

在 `commands::serve` 增加可测试的启动准备函数和结果：

```rust
pub async fn repair_legacy_token_accounting(
    app: &AppContext,
    store: &Store,
) -> Result<TokenAccountingRepairReport>
```

`TokenAccountingRepairReport` 至少记录 `rebuilt_sources` 和带 lossy 计数的 `blocked_sources`，供终端输出、日志和测试断言使用。`commands` 模块本身是兼容性内部表面，该函数不加入 crate root facade。

## Data Flow

1. `serve::run` 创建 Store 并执行 bootstrap。
2. 查询 legacy parser source 列表；空列表直接返回。
3. 按 registry 顺序读取每源 `lossy_rebuild_risk`。
4. 有风险：不调用 reset，记录 blocked source 并 `warn!`；继续下一个源。
5. 无风险：调用 `sync::run_with_options`，参数固定为 `rebuild=true`、该 source、`allow_lossy_rebuild=false`。
6. rebuild 内部再次执行 lossy guard，覆盖预检后的文件状态竞态；成功后按现有协议写 marker 2。
7. 全部目标处理完毕后，输出 repaired/blocked 摘要，再调用 `web::serve`。

## Full Rebuild Safety

`sync --rebuild` 无 source 时，`reset_for_rebuild` 先把 `registered_parsers()` 映射成稳定的 `SourceKind` 列表。`assert_lossless_rebuild`、逐源 `Store::reset_for_source`、marker clear 和后续 parser driver 都使用该集合。

不再从 command path 调用 `Store::reset_usage_data()`，因为该方法会清空 parserless 来源。现有公开 Store 方法保持兼容，但用户可触达的 full rebuild 只删除真正可重建的来源。

## Failure Semantics

- 已知 lossy 风险：best effort 跳过，保留历史和 legacy marker；看板继续启动，Web/CLI 写入仍会被 guard 拒绝。
- 预检或状态查询失败：返回错误，不启动 Web。
- 安全源 rebuild 的 parser/SQLite/commit 错误：返回错误，不启动 Web。不得掩盖 reset 后失败。
- rebuild 内部二次 lossy guard 因竞态拒绝：返回错误，不自动接受丢失。
- 某个源已成功、后续源失败：成功源保持当前 marker；迁移按源独立且可重试，不回滚已经完成的安全重建。

## Compatibility

- 新库、空源、marker 已为 2：不触发 rebuild。
- 普通 `sync`、hook-run 和 Web jobs 的 legacy guard 不改变。
- parserless Antigravity 不进入 repair，即使它存在 missing files。
- full rebuild 保留 parserless Antigravity 历史和诊断状态；它的 missing files 也不会无谓阻塞 parser-backed rebuild。
- `serve` 首次升级启动可能耗时更久并输出 sync progress；后续启动幂等。

## Documentation

同步更新 `README.md`、`README.zh-CN.md`、中英文 first-sync、dashboard 和 safety 页面，说明 `serve` 会自动迁移安全 legacy 源、风险源只警告且绝不自动启用 lossy rebuild，并明确 full rebuild 保留 parserless 历史。

## Rollback

代码回滚只会移除启动协调器，不会回退已完成的 marker 或重新写回 legacy 数据。已成功 rebuild 的数据继续符合 v2 合约；blocked 源保持原状。
