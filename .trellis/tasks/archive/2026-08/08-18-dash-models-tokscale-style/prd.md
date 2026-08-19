# Dash Models 对齐 tokscale 彩色表

## Goal

让 `llmusage dash` 的 Models 面板达到 tokscale Models 的可读性：模型按厂商族着色，用量通道分色，已入库模型全部可滚动查看，并补上当前 DTO 已经具备、面板却没画出来的列。

## User Value

用户在 Models 页能分辨厂商族、输入/输出/缓存构成、单模型成本和单位成本，不必再靠 `+N more` 摘要或切到 Cost 页才能看完整名单。

## Background

对照来源：用户 2026-08-18 两张截图、`ref/repo/tokscale` TUI、`src/tui/panels/models.rs`、`Dashboard::model_breakdown`。完整对照见 `research/tokscale-models-gap.md`。

当前 Models 只有 Model、Total Tokens、Events、Cost (USD) 四列，行文本单色，默认按 `total_tokens` 降序。`longtail.rs` 至少保留 8 行，并把连续 ≤2% 份额折成 `+N more`；截图中为 `+41 more · 6%`。

`ModelBreakdown`（`src/query/mod.rs:161-190`）已有 input / output / cache_creation / cache_read / reasoning / total / event_count / 三项成本。SQL（`src/query/mod.rs:1108-1168`）只 `GROUP BY model`，不返回 source、vendor、duration。TUI 主题已有 metric 色槽，面板未使用。面板禁止直接写 `Color::*`。

tokscale 宽表列为 `#`、Model、Provider、Source、Input、Output、Cache R、Cache W、Cache×、Total、ms/1K、Cost、Cost/1M。模型名按厂商坡度着色。默认按 Cost 降序。不折叠长尾。其 Provider 是模型厂商（OpenAI / Anthropic / xAI）。llmusage 的 `provider_label` 是 CCR 中继名。

Cache× = `cache_read / (input + cache_write)`。Cost/1M = `cost / total * 1e6`。库里没有事件耗时，无法画 ms/1K。

截图中 tokscale 的 `grok-4.6`（1.2B）高于 2% 折叠线，不属于被折叠的 41 行。`kimi-code/k3-256k` 显示 `$0.0000` 属于定价目录未覆盖。两者都不在本任务修复。

## Requirements

- R1. Models 默认渲染全部已入库模型行，用现有 `ScrollState` 滚动。不再把子 2% 尾部折成 `+N more`。Cost 面板折叠策略不变。
- R2. 宽终端列：`#`、Model、Provider、Source、Input、Output、Cache R、Cache W、Cache×、Total、Events、Cost、Cost/1M。Provider 由模型 id 推断厂商显示名。Source 为该模型去重后、按 id 排序的 source 列表（`codex, claude`）。一行仍是一个 model。
- R3. 窄终端沿用现有宽度档：`<60` 为 Model + Cost；`<80` 为 Model + Total + Cost。
- R4. 模型名按厂商族着色，同厂商内按家族/版本分色阶。着色键来自模型 id 的分隔 token，不使用 `provider_label`。网关来源上的 Claude/GPT 仍跟厂商色走。
- R5. Input / Output / Cache R / Cache W 使用已有 metric 主题槽。Cost 使用 positive 语义色。Cache× 与 Cost/1M 使用独立语义槽。`NO_COLOR` / `LLMUSAGE_NO_COLOR` / ANSI16 行为保持 `tui-presentation-contracts.md`。
- R6. Cost 用带 `$` 的紧凑格式（≥1000 用一位小数 `K`）。Cache× 保留一位小数并带 `x`；分母为 0 且 cache_read > 0 时显示 `∞`，两者都为 0 时显示 `—`。Cost/1M 在 total 为 0 时显示 `—`。
- R7. 打开 Models 时默认按 Cost 降序，表头显示 `▼`。`o` 仍在 Tokens 与 Cost 之间循环，`O` 反转方向。排序后仍显示全部行。
- R8. 查询层为 `ModelBreakdown` 增加聚合 Source 列表。不拆行，不改 `usage_event` / bucket 主键，不加 duration 列。`model_breakdown` 的 `ORDER BY SUM(total_tokens) DESC` 保持不变，以免改动 web `/api/models` 的默认顺序。
- R9. 交互文案保持英文。TestBackend 覆盖：宽/窄列集、着色在 NoColor 下消失、长尾不再折叠、滚动窗口只格式化可见行、默认 Cost 降序。

## Acceptance Criteria

- [ ] AC1. `llmusage dash` 打开 Models，已入库的全部模型都能通过滚动到达，缓冲区不含 `+N more`。
- [ ] AC2. 终端宽度 ≥80 时表头含 Provider、Source、Input、Output、Cache R、Cache W、Cache×、Total、Events、Cost、Cost/1M。通道与成本数值来自现有 `ModelBreakdown` 字段或由其直接算出。
- [ ] AC3. Provider 显示由模型 id 推断的厂商名（Anthropic / OpenAI / xAI / Google / Moonshot / Zhipu 等）；与模型名使用同一套厂商映射。
- [ ] AC4. 同一厂商族的模型名颜色可区分（Claude 橙、GPT 绿、Gemini 蓝、Grok 黄）；`NO_COLOR=1` 下无前景色、无修饰。
- [ ] AC5. 同一模型跨多个 source 时仍是一行，Source 列按 id 排序后列出那些 source。
- [ ] AC6. 打开 Models 时 Cost 列表头带 `▼`，行按 `cost_with_cache_usd` 降序。
- [ ] AC7. Cost 面板仍可折叠长尾。CLI 报表 JSON 与 web `/api/models` 既有字段名、类型、默认 token 降序不变。
- [ ] AC8. `cargo fmt --check`、严格 Clippy、相关 TUI/query 测试通过。

## Out of Scope

- 为 Gemini CLI、Copilot、Cursor、WorkBuddy、Qwen CLI 等 monitor-only 平台新增解析器或补同步。
- 重算或对齐 tokscale 价目表。Grok `unpriced` / `total_only` 合同不变。
- 伪造 ms/1K。
- 把 `provider_label` 当成 Provider 列的数据源。
- 把 tokscale 的 Workspace 分组搬进 Models。
- 改 web `serve` 的模型条形图/百分比表。
- 改 Daily / Hourly / Overview 的着色，除非 Models 抽出的纯函数可无行为变化地复用。

## Decisions

| 决策 | 选择 | 日期 |
| --- | --- | --- |
| 范围 | 只改已入库模型的 TUI Models 展示 | 2026-08-18 |
| 新来源解析 | 另开任务 | 2026-08-18 |
| web Models | 不纳入 | 2026-08-18 |
| Provider 列 | 宽表加，由模型 id 推断；窄屏不加 | 2026-08-19 |
| 默认排序 | Cost 降序 | 2026-08-19 |
