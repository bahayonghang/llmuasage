# 添加 GPT-6 Astra 与 Claude Fable/Mythos 5.1 定价覆盖

## Goal

让本地 Codex / OpenCode 的 `gpt-6-astra` 日志，以及 Claude / OpenCode 的 `claude-fable-5-1` 与 `claude-mythos-5-1` 日志，使用官方 Standard 费率与上下文窗口计费和展示。新导入和内置目录升级后的历史事件不再落到 `unpriced`，也不再误用 GPT-5、Claude Fable 5 或 Claude Mythos 5 的 cache 费率。

用户价值：Astra、Fable 5.1、Mythos 5.1 的成本、模型分布、上下文压力与报表数字和官方价卡一致。

## Background

官方模型（2026-09-05 核对）：

- GPT-6 Astra。API / Codex id `gpt-6-astra`；公开 snapshot/alias 只有自身；Codex 用法 `codex -m gpt-6-astra`。上下文 1,050,000。Standard 短上下文 USD/MTok：input `10.00`、cached `1.00`、cache write `12.50`、output `50.00`。单条请求 `prompt_tokens > 272_000` 时全请求 2× 输入与 cache 通道、1.5× 输出。来源：https://developers.openai.com/api/docs/models/gpt-6-astra 、https://developers.openai.com/api/docs/pricing
- Claude Fable 5.1。API id `claude-fable-5-1`；Bedrock `anthropic.claude-fable-5-1`；2026-09-01 发布。上下文 1,000,000，全程标准单价。USD/MTok：input `10.00`、output `50.00`、5m cache write `12.50`、1h cache write `20.00`、cache read `0.25`。相对 Fable 5，只有 cache read 从 `1.00` 降到 `0.25`。来源：https://platform.claude.com/docs/en/models/fable-5-1/overview 、https://platform.claude.com/docs/en/models/fable-5-1/whats-new-fable-5-1
- Claude Mythos 5.1。API id `claude-mythos-5-1`。与 Fable 5.1 同规格同价，仅 Project Glasswing。来源：https://platform.claude.com/docs/en/models/fable-5-1/whats-new-fable-5-1

仓库行为：

- 内置目录 `pricing/static-v2.json`，`schema_version = 2`，`version = "static-v2"`。Parser 写入的模型字符串不变；目录只负责识别、费率和窗口。`.trellis/spec/llmusage/backend/pricing-catalog-contracts.md`
- Family 匹配：`normalized == matcher` 或 `starts_with("{matcher}-")`。`exact` 优先于 `family`，同 mode 取最长 matcher。`src/domain/pricing_catalog.rs:370-393`、`725-737`
- Codex `gpt-5` family 不命中 `gpt-6-astra`，Astra 在 Codex 上当前 `unpriced`。
- OpenCode `gpt` family 命中 `gpt-6-astra`，Astra 在 OpenCode 上当前按 GPT-5 费率 `1.25 / 0.125 / 10.0` 误计价。`pricing/static-v2.json:125-141`
- `claude-fable-5` family 命中 `claude-fable-5-1`，Fable 5.1 当前按 Fable 5 的 cache read `1.00` 误计价。`pricing/static-v2.json:89-104`
- `claude-mythos-5` family 对 `claude-mythos-5-1` 有同样前缀陷阱。
- 未 pin 的 `static-*` 目录只在 `embedded.version` 变化时 bootstrap 重算。`src/store/pricing_catalog.rs:496-500`
- Fable 5 成本夹具：input 1M + cache_read 200k + cache_creation 300k + output 400k → with-cache `33.95`。`src/domain/pricing.rs:263-286`
- GPT-5.6 用 exact matcher + 272K tier，是 Astra 应复用的数据形状。`.trellis/spec/llmusage/backend/pricing-catalog-contracts.md` §7

## Scope Decision

2026-09-05 用户确认：本任务包含 Claude Mythos 5.1。理由与 2026-07-03 Fable 5 任务相同——共享 Fable 5.1 规格与价格，且现有 `claude-mythos-5` family 已经误标 `claude-mythos-5-1`。

## Requirements

1. 在内置 base catalog 增加稳定模型 id `gpt-6-astra`，来源至少 `codex` 和 `opencode`。
2. Astra matcher 必须压过 OpenCode 的 `gpt` family。使用 exact `gpt-6-astra`，并保留 family `gpt-6-astra` 覆盖 dated snapshot（如 `gpt-6-astra-2026-09-03`）。不要增加无官方依据的 `gpt-6` 或 `gpt-6-astra-aeon` 别名。
3. Astra 费率与窗口：default `10.0 / 1.0 / 12.5 / 50.0`；long_context 在 `prompt_tokens_above = 272000` 时为 `20.0 / 2.0 / 25.0 / 75.0`；`context_window = 1050000`；推理 token 保持 `included_in_output`。
4. 在内置 base catalog 增加稳定模型 id `claude-fable-5-1` 与 `claude-mythos-5-1`，来源至少 `claude` 和 `opencode`。
5. 5.1 matcher 必须压过现有 Fable 5 / Mythos 5 family。覆盖 `claude-fable-5-1`、`fable-5-1`、`claude-mythos-5-1`、`mythos-5-1`、点分 `claude-fable-5.1` / `claude-mythos-5.1`，以及 OpenCode/Bedrock 形 `anthropic.claude-fable-5-1`、`anthropic/claude-fable-5-1`、`anthropic.claude-mythos-5-1`、`anthropic/claude-mythos-5-1`。不要使用裸 `mythos` matcher。
6. Fable 5.1 / Mythos 5.1 费率与窗口：default `10.0 / 0.25 / 12.5 / 50.0`；`cache_creation_per_mtok = 12.5` 继续用 5m write 近似聚合 cache-creation；无 long-context tier；`context_window = 1000000`。
7. 不改 parser 写入的原始模型名，不新增 `SourceKind`，不做 schema migration。
8. 把 catalog 文档 `version` 从 `static-v2` 升到 `static-v3`，使未 pin 的内置目录在下次 `sync` 时 bootstrap 重算已落库的误计价事件。`schema_version` 保持 `2`。
9. 别名与费率只进 catalog 数据。不要为这些模型增加生产 Rust 分支。
10. 更新定价契约、README 中英、架构文档中英，写明 Astra、Fable 5.1、Mythos 5.1 覆盖与 `static-v3` 身份。
11. 已 pin 的完整 snapshot 与 overlay 保持 pin；本任务不强制 overlay rebase。
12. 保持 local-first：不增加远程拉价。

## Acceptance Criteria

- [ ] AC1 `PricingCatalog::embedded().find("codex", "gpt-6-astra")` 与 `find("opencode", "gpt-6-astra")` 返回 id `gpt-6-astra`，default 费率 `10 / 1 / 12.5 / 50`，窗口 `1_050_000`，并带 272K long-context tier。对应 R1–R3。
- [ ] AC2 OpenCode 上 `gpt-6-astra` 不再命中 `gpt-5-legacy-opencode`。`find("opencode", "gpt-6-astra").id == "gpt-6-astra"`。对应 R2。
- [ ] AC3 `compute_cost("codex", "gpt-6-astra", tokens(100_000, 100_000, 72_000, 100_000, 0))` 走 default tier，with-cache `7.0`；`cache_creation = 72_001` 走 `long_context`，with-cache `11.500025`。`pricing_status = Static`，`pricing_source` 为 `static-v3`。对应 R3、R8。
- [ ] AC4 `find("claude", "claude-fable-5-1")` 与 `find("claude", "claude-mythos-5-1")` 返回各自稳定 id，cache read `0.25`，窗口 `1_000_000`。OpenCode 的 `anthropic.claude-fable-5-1`、`anthropic/claude-mythos-5-1` 同样命中。对应 R4–R6。
- [ ] AC5 `find("claude", "claude-fable-5")` 与 `find("claude", "claude-mythos-5")` 仍是 5.0 行，`cached_per_mtok = 1.0`。Fable/Mythos 5 成本夹具 `33.95` 不回归。对应 R5。
- [ ] AC6 `compute_cost("claude", "claude-fable-5-1", tokens(1_000_000, 200_000, 300_000, 400_000, 0))` 与 Mythos 5.1 同夹具的 with-cache 为 `33.80`，without-cache 为 `35.0`。对应 R6。
- [ ] AC7 上下文压力把 Fable 5.1 / Mythos 5.1 当作已知 1M 窗口，把 Astra 当作已知 1.05M 窗口；三者都不计入 unpriced/unknown-window。对应 R3、R6。
- [ ] AC8 同步夹具：Codex + OpenCode 的 `gpt-6-astra`、Claude 的 `claude-fable-5-1` 与 `claude-mythos-5-1` 落库后 `pricing_status = static`，模型名不被改写成 GPT-5 / Fable 5 / Mythos 5。对应 R1、R4、R7。
- [ ] AC9 未 pin 的 `static-v2` 库在第一次 `sync` 时升级到 `static-v3` 并重算成本。Pin 的 snapshot / overlay 不自动切换。对应 R8、R11。
- [ ] AC10 负例：`not-gpt-6-astra`、`gpt-6-rewrite`、`not-fable-5-1`、`not-mythos-5-1`、`claude-mythos-preview` 不得领取新费率。对应 R2、R5。
- [ ] AC11 `cargo fmt --check`，`cargo clippy --all-targets --all-features -- -D warnings`，聚焦定价/catalog/sync 测试，以及 `cargo test -- --test-threads=1`。文档变更则 `npm --prefix docs run docs:build`。对应 R10。

## Out Of Scope

- 把 GPT-5.6 内置费率改成当前 OpenAI 促销价（Sol 文档现为 `4 / 0.4 / 20`，目录仍是 `5 / 0.5 / 30`）。
- Fast / Batch / Flex / 数据驻留加价、工具调用费。
- 拆分 Claude 5m / 1h cache-write 列。
- 新增 parser、数据源或 schema。
- 为无官方价卡的 `gpt-6-astra-aeon` 建独立模型。
- 强制用户 overlay rebase。
- 远程拉价。
- 仅为 `mythos-5-1` 这种不含 `claude` 的短名补 TUI vendor 着色。
