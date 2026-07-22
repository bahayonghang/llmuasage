# 聚焦来源子命令 - 技术设计（C5）

## CLI 树

新增四个顶层 source host：`claude`、`codex`、`opencode`、`antigravity`，每个包含
`daily|weekly|monthly|session`。它们复用同名 top-level args，因此共享日期、no-cost、breakdown、
compact、by-agent 和 sections 的 parser。`blocks` 不挂入 source tree。

位置 source 会注入 `ReportFilter.source`：已有 `--source` 为空或同值时接受；不同值返回明确冲突错误。
这保持旧的 `<period> --source` 表面不变。

## 复用路径

聚焦命令调用 C1/C2/C4 的 source-filtered `load_unified_report` / `load_sections`，然后执行纯视图投影：

- daily/weekly/monthly 从每个 `All` 行取唯一 source breakdown，提升为顶层 row；
- session 已是 source row，直接保留；
- 结果没有 All 行和 `agent_breakdowns`。

因此 token/cost/filter/order 语义和 `<period> --source <source>` 相同，不复制 query。

## 呈现与 JSON

`render_focused_table` 使用 C1 的数值格式和 model breakdown 规则，但列为
`<Period>|Models|Input|Output|...|Cost`，不含 Agent；标题为 `<Source> Usage Report - <Period>`，不含
Detected。`--no-cost` 仍裁剪末列。

聚焦 JSON 是 `{ "<period>":[rows], "totals":{...} }`，row 保留
`period/modelsUsed/inputTokens/outputTokens/cacheCreationTokens/cacheReadTokens/totalTokens/totalCost/
modelBreakdowns`，删除 `agent` 和 `agents`。sections 使用同样的扁平有序结构和 command totals。

## 能力矩阵

四个 llmusage sources 均支持四个 period。这是 llmusage 已有统一 `--source` 能力的扩展，文档必须
明确它不是 ccusage 各来源能力矩阵的逐项镜像。

## 验证

- Clap/dispatch test 覆盖四 source × 四 period（至少 command help/parse）。
- fixture 比较 focused JSON totals 与对应 `--source` unified report；focused rows 无 Agent/agents。
- text 断言无 `Agent`/`Detected:`，并保留 source 标题、token/cost 或 no-cost 行为。
- 冲突 `claude daily --source codex` 返回错误；相同 source 可运行。
