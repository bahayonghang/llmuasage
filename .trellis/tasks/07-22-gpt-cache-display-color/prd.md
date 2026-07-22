# GPT cache 显示语义与报表颜色优化

## 背景

`llmusage daily` 统一报表中，Codex(GPT 模型) 行的 Cache Create 恒为 0，
用户误以为是解析缺陷。根因分析（见 research/cache-zero-root-cause.md）：

- OpenAI Responses API 的 usage 只上报 `cached_input_tokens`（缓存命中读取），
  **不存在任何 cache 写入指标**。OpenAI 自动缓存写入不计费，因此不上报。
- Anthropic 按 1.25x 计费 `cache_creation_input_tokens`，所以 Claude 源有值。
- ccusage（ref/repo/ccusage）的 Codex 适配器同样只有 `cached_input_tokens`
  字段，报表 JSON 硬编码 `cacheCreationTokens: 0` 并有测试断言。
- llmusage 解析器已防御性兼容多种 cache_creation 别名，数据源没有则为 0。
  行为与 ccusage 完全一致，**不是 bug**。

问题转化为展示语义：`0` 无法区分"该源不上报此指标"与"当天确实为 0"，
且统一报表除 Agent 标签外几乎无颜色，可读性差。

## 需求

1. 统一/聚焦报表中，Cache Create 与 Cache Read 列的 0 值渲染为 `-`，
   表示"无该项数据"；非零值保持紧凑格式不变。
2. 为统一/聚焦报表增加颜色样式（仅人类表格输出，遵循 ColorMode）：
   - 表头行加粗；
   - Total 行加粗；
   - Agent 源标签保持现有按源着色；
   - `-` 占位符使用暗淡（dim）样式。
3. CLI JSON 输出不变：数值仍为 0，键名与结构不动。
4. `--no-cost`、`--compact`、`NO_COLOR`/`LLMUSAGE_NO_COLOR` 行为不变；
   ColorMode::Never 下输出纯文本 `-`，无 ANSI 序列。

## 验收标准

- [ ] `llmusage daily`：Codex 行 Cache Create 显示 `-` 而非 `0`；
      Claude 行 cache 数值照常显示。
- [ ] 有色终端下表头与 Total 行加粗，`-` 为 dim；`NO_COLOR=1` 下无 ANSI。
- [ ] `llmusage daily --json` 中 `cacheCreationTokens` 仍为数值 0。
- [ ] `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、
      报表相关测试全部通过。

## 非目标

- 不改解析器、存储、query DTO、dashboard/TUI/export 序列化。
- 不为 Codex 伪造 cache creation 数据。
- 不改动 blocks/session 等其他表格的列结构。
