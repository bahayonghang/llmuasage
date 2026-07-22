# Daily cache 汇总展示修复 - 技术设计

## Evidence

- 用户样本中两天的 cache create 值在 llmusage 与 ccusage 间完全相同；稳定的
  `2026-07-17` 样本只有 Codex 表格总量多出 `76,820`。
- llmusage `TokenTotals.total_tokens` 沿查询链保留 SQLite 中的来源权威总量；统一表格
  当前直接渲染该字段，因此把未展示的 reasoning/extra token 混进了表格总量。
- ccusage 的统一 JSON/内部行同样保留权威总量，但
  `ref/repo/ccusage/rust/crates/ccusage/src/adapter/all/report.rs` 中
  `table_total_tokens` 专门重算四个可见分量。总行也逐 period 使用相同函数求和。
- llmusage 已有 `tui::format::token_compact`，其 K/M/B 行为和边界测试可直接复用，
  无需新增 formatter。

## Boundaries

- 只修改终端文本投影与相应测试。
- 不修改 parser、SQLite schema、query aggregation、CLI JSON 或成本计算。
- unified 与 focused 表格共享相同的可见总量和紧凑格式；它们是同一 CLI 报表契约。
- 其他 dashboard/export/TUI 数据消费者保持原样。

## Presentation Contract

新增私有 helper：

```text
table_total_tokens(input, output, cache_creation, cache_read)
  = saturating_sum(input, output, cache_creation, cache_read)
```

统一行、来源子行、model breakdown 和最终 Total 行均通过该 helper 生成文本
`Total Tokens`。`TokenTotals.total_tokens` 与 `ModelCostBreakdown.total_tokens` 不被覆写，
JSON DTO 继续读取原值。

上述表格所有 token 单元格使用 `format_token_compact`。小于 1,000 的数值保持整数，
其余使用两位小数的 `K`/`M`/`B`，沿用已有 formatter 的舍入行为。

## Total Row Hierarchy

通用 box-table renderer 在下一行是 `Total`/`TOTAL` 时，用双线横向 junction
`╞═╪═╡` 代替普通行间 `├─┼─┤`。这提供非颜色环境下的稳定层级，不改变列宽、单元格
内容或 ANSI 行为。其他拥有 Total 行的表格会得到相同的纯展示增强。

## Compatibility And Rollback

- 人类可读 `Total Tokens` 有意与来源权威总量分离，以匹配当前可见列和 ccusage。
- JSON 与内部数据无兼容变化。
- 回滚仅需撤销 `report_table` 的 helper/format/separator 改动及测试。

## Codex Replay Prefix Accounting

Codex fork/subagent rollout 可以在文件开头复制父会话历史，并把复制记录的 timestamp
统一改写到 fork 创建秒。仅靠 `timestamp + model + token tuple` 的全局 event key 无法
去重，因为改写后的 timestamp 与父文件不同。

采用 ccusage 的双重门控：

1. 只对文件头 16 KiB 包含 `thread_spawn` 或 `forked_from_id` 的文件启用检测。
2. 只有前两个有效 `token_count` 位于同一秒时，才把该秒判定为 replay 秒。
3. 跳过 replay 秒内的 token 事件和待绑定 tool evidence，但持续更新
   `total_token_usage` baseline。
4. 第一个不在 replay 秒内的 token 事件恢复正常解析；普通文件即使同秒产生多次请求，
   也不会因为缺少 replay marker 而被过滤。

该变化会使历史 Codex 行需要重建。新增 source-aware expected accounting version：Codex
使用 `3`，Claude/OpenCode 保持 `2`。已有 Codex marker `2` 被识别为 legacy，沿用现有
lossy-rebuild guard；不通过迁移或普通 sync 静默删除历史。
