# GPT 模型 Cache Create = 0 根因分析

## 现象

`llmusage daily` 中 Codex（gpt-5.6-sol / gpt-5.6-terra）与 OpenCode
（deepseek-v4-flash-free）的 Cache Create 列恒为 0，而 Cache Read 有值
（如 514.72M），Claude 源两列均有值。

## 数据链路核对（llmusage）

- `src/parsers/codex.rs` 读取 `~/.codex/sessions/rollout-*.jsonl` 的
  `token_count` 事件，usage 对象字段为：`input_tokens`、
  `cached_input_tokens`、`output_tokens`、`reasoning_output_tokens`、
  `total_tokens`。
- `parse_usage_tokens`（codex.rs:652-669）已防御性查找
  `cache_creation_tokens` / `cache_creation_input_tokens` 及多层嵌套别名，
  但 Codex rollout 数据里从不出现这些键 → 恒为 0。
- Cache Read 解析正确：`cached_input_tokens` 为含在 input 内的口径，
  解析时从 input 中扣除（与 ccusage `non_cached_input_tokens` 一致）。

## ccusage 参照（ref/repo/ccusage）

- `rust/crates/ccusage/src/adapter/codex/types.rs`：`CodexRawUsage` 仅有
  `input_tokens`、`cached_input_tokens`、`output_tokens`、
  `reasoning_output_tokens`、`total_tokens` —— 没有 cache creation 字段。
- `rust/crates/ccusage/src/adapter/codex/report.rs:65,89,119`：报表 JSON
  硬编码 `"cacheCreationTokens": 0`。
- `rust/crates/ccusage/src/adapter/codex/mod.rs:145,149`：测试显式断言
  `cacheCreationTokens == 0`。

## 结论

- OpenAI 自动 prompt caching 的写入不计费也不上报，usage 中只有
  `cached_tokens`（读取命中）。Anthropic 因 1.25x 计费 cache 写入而上报
  `cache_creation_input_tokens`，所以只有 Claude 源有 Cache Create。
- llmusage 与 ccusage 行为完全一致：**Cache Create = 0 是数据源特性，
  不是解析 bug，无数据可恢复**。
- OpenCode 解析器支持 `tokens.cache.write` → cache_creation
  （opencode.rs:467-470），走 Anthropic 模型时会有值；deepseek 等
  OpenAI 兼容供应商同样无写入指标 → 0。
- 改进方向是展示语义：0 值 cache 单元格渲染 `-` 表示"无该项数据"。
