# tokscale Models vs llmusage dash Models

Date: 2026-08-18
Surfaces compared: user screenshot of `llmusage dash` panel 3, user screenshot of tokscale Models tab, `ref/repo/tokscale` TUI source, current `src/tui/panels/models.rs` and `src/query/mod.rs`.

`llmusage dash` is the interactive TUI (`tui-presentation-contracts.md`). The web `serve` Models section is a separate surface.

## 1. Visible layout

| Item | llmusage dash Models | tokscale Models |
| --- | --- | --- |
| Row identity | Model name only | Rank `#` + model name |
| Columns (wide) | Model, Total Tokens, Events, Cost (USD) | `#`, Model, Provider, Source, Input, Output, Cache R, Cache W, Cache×, Total, ms/1K, Cost, Cost/1M |
| Default order | `SUM(total_tokens) DESC` | Cost descending (`Cost ▼` in screenshot) |
| Cost text | `12592.8488` (4 decimals, no `$`) | `$12.3K` compact, green |
| Model name color | None. First row uses accent + bold | Vendor/family shade, bold |
| Metric color | None | Input green, Output red, Cache R cyan, Cache W purple, Cache× cyan, Total themed, ms/1K yellow, Cost green, Cost/1M pale green |
| Long tail | Keep first 8, fold sub-2% tail to `+41 more · 6%` | Every model stays a row; scroll |
| Narrow terminal | Same 4 columns, percentage widths | Very narrow: Model + Cost. Narrow: Model + Tokens + Cost |
| Sort | `o` / `O` on Tokens and Cost. Sort cancels collapse | Header arrows on Tokens and Cost |
| Grouping | Always `GROUP BY model` | Default `GroupBy::Model`; workspace grouping adds a Workspace column |

tokscale wide-column constraints live in `ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/models.rs` (rank 3, model `Min(20)`, Provider 18, Source 14, token/cost columns 8–10). Narrow and very-narrow layouts drop identity and metric columns instead of squeezing every column.

## 2. Color system in tokscale

Three layers:

1. **Vendor of the model id** — `get_provider_from_model` in `widgets.rs`. Delimited-token match: `claude`/`opus`/`sonnet`/`haiku`/`fable` → anthropic; `gpt`/`codex`/`o1`/`o3` → openai; `gemini` → google; `grok` → xai; delimited `glm` → zai; delimited `kimi` → moonshotai. Gateway ids such as `github-copilot` have no vendor palette, so color still follows the model vendor.
2. **Shade rank inside one vendor** — `build_model_shade_map` in `colors.rs`. Anthropic ranks by family (fable > opus > sonnet > haiku), then version, then cost, then name. Other vendors rank by version, then cost. Same model through a gateway and a native client share one shade bucket.
3. **Hard-coded 7-step RGB ramps** — Anthropic `#DA7756`…, OpenAI `#10B981`…, Google `#3B82F6`…, xAI `#EAB308`…, Moonshot `#14B8A6`…, Zhipu `#A855F7`…, DeepSeek, Meta, Cursor, Sakana, unknown gray.

Metric colors are theme slots (`metric_input_style`, `metric_output_style`, `metric_cache_read_style`, `metric_cache_write_style`). Cost is `Color::Green`. Cache× is cyan. Cost/1M is `Rgb(150, 200, 150)`. ms/1K is yellow.

The tokscale web helper `packages/frontend/src/components/profile/modelColors.ts` uses a simpler first-match family map (fable/opus/sonnet/haiku/gpt/gemini/…). The TUI shade map is the screenshot source of truth.

## 3. Derived metrics in tokscale

From `widgets.rs`:

- **Cache×** = `cache_read / (input + cache_write)`. Paid denominator 0 and cache_read > 0 → `∞`. Both 0 → `—`.
- **Cost/1M** = `cost / total_tokens * 1_000_000`. No tokens → `—`.
- **ms/1K** = optional `performance.ms_per_1k_tokens`. Missing/non-finite → `—`.
- **Provider display** = title-case vendor, with joined lists such as `Custom, OpenAI` preserved per segment.
- **Source display** = client display name (`Codex CLI`, `Claude Code`, `grok`, `pi`, `OpenCode`). Multiple clients join with `, `.

## 4. What llmusage already has

`ModelBreakdown` (`src/query/mod.rs:161-190`) already carries:

- `model`
- `input_tokens`, `output_tokens`, `cache_creation_tokens`, `cache_read_tokens`, `reasoning_output_tokens`, `total_tokens`
- `event_count`
- `cost_with_cache_usd`, `cost_without_cache_usd`, `cache_savings_usd`
- `pricing_status`, `pricing_source`, `pricing_rate`

`Dashboard::model_breakdown` groups `usage_bucket_30m` by `model` only. It does not return `source`, `provider_label`, or duration.

TUI theme already has `metric_input` / `metric_output` / `metric_cache_read` / `metric_cache_write` / `metric_reasoning` (`src/tui/theme.rs`). Models panel does not use them. Presentation contract forbids `Color::*` in panel files; new vendor colors must be theme accessors.

`CostLine` already has `(source, model, events, tokens, cost)`. That panel also folds a long tail.

`SourceKind` already includes `codex`, `claude`, `opencode`, `antigravity`, `kimi_code`, `pi`, `grok`, `zcode`, `deepseek_harness`. Gemini CLI, Copilot, Cursor, WorkBuddy, Zed remain monitor-only (`docs/agents/passive-source-candidates.md`).

`provider_label` is the CCR relay label (anyrouter / methink / …), not tokscale's vendor column.

No `usage_event` duration field exists. `ms/1K` cannot be computed from current store data.

Long-tail fold: `src/tui/panels/longtail.rs` keeps at least 8 rows, folds a contiguous tail where each row is ≤ 2% of total tokens, and only when at least 2 rows fold. Sort disables the fold (`tui-runtime-contracts.md`). Screenshot 1 matches this: 8 kept rows + `+41 more · 6%` (1.9B).

## 5. Screenshot row comparison

Same machine, same day, two tools. Totals and prices do not line up 1:1 (different windows, token accounting, and price books). Rank-level gaps still stand:

| Model in tokscale | tokscale tokens | llmusage Models visible? |
| --- | --- | --- |
| gpt-5.6-sol | 14.9B | Yes, 15.4B |
| gpt-5.5 | 7.0B | Yes, 8.5B |
| claude-fable-5 | 445.1M | Yes, 2.9B |
| claude-opus-5 | 996.8M | Yes |
| grok-4.6 | 1.2B | No. 1.2B is above the 2% fold threshold, so this is not the `+41 more` row |
| claude-opus-4-8 | 457.0M | Yes, 1.7B |
| gpt-5.6-terra | 813.9M | Yes |
| grok-4.5 | 127.9M | Not in top 8 |
| gemini-* / glm-* / copilot-hosted variants | present | Not in top 8 |

`kimi-code/k3-256k` is visible in llmusage at 367.2M / `$0.0000`. tokscale shows `k3` at 317.9M / `$149.30`. Name and price catalog differ.

`grok-4.6` source in tokscale is `grok, pi`. llmusage already has both parsers. Absence on the llmusage top list is therefore either unsynced local data, a different model id after normalization, or Grok `total_only` rows that the user has not rebuilt. It is not explained by the long-tail fold.

tokscale rows such as `gemini-3-pro-preview` (Gemini CLI), `coder-model` (Qwen CLI), `hy3` (WorkBuddy), Copilot-hosted Claude/GPT variants require sources that llmusage does not import.

## 6. Gap classes

### A. Presentation of already-loaded rows

- Model name has no vendor color.
- Token channels sit in the DTO and stay unused.
- Cost format and color do not match tokscale.
- Cache× and Cost/1M are computable from current fields.
- Rank `#` is local to the sorted view.
- Vendor can be inferred from the model id with a delimited-token map. Do not reuse `provider_label`.
- Source can be added as `GROUP_CONCAT(DISTINCT source)` without splitting the model row.
- Long-tail fold hides 41 imported models in the screenshot.

### B. Query/grouping changes

- Splitting one model into one row per source would change rank, totals, and the meaning of "a model row". tokscale default still merges clients onto one row and joins names.
- Workspace grouping is a tokscale extra. llmusage already has a Projects/Hourly panel.

### C. Data llmusage cannot show honestly

- `ms/1K`: no event duration in SQLite.
- Gemini CLI / Copilot / Cursor / WorkBuddy / Qwen CLI models: no parser, monitor-only.
- Grok cost: Grok source is `total_only` and `unpriced` by contract.
- Exact tokscale cost figures: different price book and token accounting.

### D. Shared TUI contracts that a Models restyle must keep

- `tui-presentation-contracts.md`: theme slots, `NO_COLOR` / `LLMUSAGE_NO_COLOR`, ANSI16 mapping, English interactive copy, `stat_compact` for token/count cells, no `Color::*` in panels.
- `tui-runtime-contracts.md`: visible-row windowing, collapse-plan reuse, sort disables collapse, wrap/page/Home/End, `o`/`O` sort.
- Cost panel shares `longtail`. Changing fold policy for Models only must not silently change Cost.

## 7. Recommended MVP (pending user confirmation)

Keep one model row. Stop folding the Models tail so every imported model is reachable by scroll. Add wide columns that current data can fill: `#`, Model (vendor-colored), Source (joined source ids), Input, Output, Cache R, Cache W, Cache×, Total, Events, Cost, Cost/1M. Infer vendor from model id and assign shade ranks inside each vendor. Use existing metric theme slots for token channels; add vendor shade accessors in `theme.rs`. Drop to Model/Total/Cost on narrow widths. Keep `ms/1K` as later work. Do not add parsers in this task.

## 8. File anchors

- llmusage render: `src/tui/panels/models.rs`
- llmusage collapse: `src/tui/panels/longtail.rs`
- llmusage DTO + SQL: `src/query/mod.rs:161-190`, `1108-1168`
- llmusage theme: `src/tui/theme.rs`
- llmusage panel sort keys: `src/tui/app.rs:620-628`
- tokscale render: `ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/models.rs`
- tokscale color: `ref/repo/tokscale/crates/tokscale-cli/src/tui/colors.rs`, `.../ui/widgets.rs` (`get_provider_from_model`, `get_provider_shade`)
- tokscale formatters: `ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/widgets.rs:12-89`
- web dash Models (out of TUI scope): `src/web/assets/render/models.js`, `src/web/shell.rs` Models section
