# Design: Dash Models 对齐 tokscale 彩色表

## Architecture

本任务停在展示层和一次只读查询扩展。解析器、store 主键、定价目录、web 页面都不改。

```
usage_bucket_30m
    → Dashboard::model_breakdown  (GROUP BY model + 聚合 sources)
    → ModelBreakdown
    → TUI Models panel
         ├ vendor_from_model / shade rank  (纯函数)
         ├ theme::vendor_style / metric_*  (语义色)
         └ format::{stat_compact, cost_compact, cache_multiplier, cost_per_million}
```

web `/api/models` 继续走 `PublicModelBreakdown`。该公开类型不增加 `sources`，JSON 字段集与默认 `ORDER BY SUM(total_tokens) DESC` 不变。TUI 读 `ModelBreakdown` 后在内存里按 Cost 降序排。

## Boundaries

| 层 | 做 | 不做 |
| --- | --- | --- |
| query | `ModelBreakdown.sources: Vec<String>`；SQL `GROUP_CONCAT(DISTINCT source)`，Rust 侧排序去空 | 改 `ORDER BY`、拆行、加 duration |
| web | `From<ModelBreakdown>` 忽略 `sources` | 改 `render/models.js`、改公开字段 |
| theme | 厂商坡度 accessor；Cache× / Cost/1M 两个新语义槽；ANSI16/NoColor 走现有 `adapt_color` / `fg_style` | 面板文件写 `Color::*` |
| format | 新增 `cost_compact` / `cache_multiplier` / `cost_per_million` | 改现有 `cost` / `stat_compact` 契约 |
| Models 面板 | 宽/窄列集、分色、默认 Cost 排序、取消折叠 | 改 Cost 面板折叠 |
| 其他面板 | 不改着色 | Daily / Hourly / Overview 本任务不跟着色 |

## Data flow

1. `model_breakdown` 仍按 model 聚合 token/成本/事件。新增一列 `GROUP_CONCAT(DISTINCT source)`。映射时按 `,` 切开、去空、按 id 排序，写入 `sources: Vec<String>`。
2. TUI 接受 payload 后：
   - 不再计算 `model_collapse`（恒为 `None`）。
   - `update_scroll_total` 对 Models 使用原始行数。
   - `SortState` 初始为 `{ key: Cost, descending: true }`。`stable_sort_refs` 按 `cost_with_cache_usd` 排。
3. 渲染前用**完整** payload 建一次 `(vendor, model) → shade rank` 表，避免滚动时变色。
4. 按 `area.width` 选列集：`<60` Model+Cost；`<80` Model+Total+Cost；否则 13 列。`#` 是排序后的 1-based 绝对行号。
5. 每行只格式化 `visible_range` 内的行。

## Contracts

### Query

```text
ModelBreakdown.sources: Vec<String>   # 排序后的 source id，可为空向量（测试夹具）
GROUP BY model
ORDER BY SUM(total_tokens) DESC, model ASC   # 不变
```

SQLite `GROUP_CONCAT(DISTINCT source)` 的顺序不稳定。权威顺序在 Rust：split → trim → 去空 → sort → 去重。展示为 `codex, claude`。

### Vendor map

着色与 Provider 列共用一套映射，语义对齐 tokscale `get_provider_from_model`：

| 模型 id 分隔 token | vendor id | 显示名 |
| --- | --- | --- |
| claude / opus / sonnet / haiku / 独立 token `fable` | anthropic | Anthropic |
| gpt / chatgpt / codex / o1* / o3* | openai | OpenAI |
| gemini | google | Google |
| grok | xai | xAI |
| 独立 token `glm` | zai | Zhipu |
| 独立 token `kimi` | moonshot | Moonshot |
| deepseek | deepseek | DeepSeek |
| llama | meta | Meta |
| 其余 | unknown | Unknown |

`*` 表示前缀匹配（`o1`、`o3`）。`fable` / `glm` / `kimi` 必须按分隔 token 匹配，避免 `unfabled` 误入 Anthropic。

色阶：Anthropic 按家族 fable > opus > sonnet > haiku > 其他，再按版本号新→旧，再按成本，再按名字。其他厂商按版本、成本、名字。同一 vendor 的 shade 在 7 档坡度上封顶。

厂商坡度是跨主题常量（对齐 tokscale 截图），不放进 `Theme` 的 4 套调色板。accessor 在 `theme.rs`：查 RGB 表 → `adapt_color(color_mode())` → `bold_fg_style`。这样 ANSI16 / NoColor 仍走契约。

### Theme slots

现有 `metric_input` / `metric_output` / `metric_cache_read` / `metric_cache_write` 不变。新增：

- `metric_cache_hit`：Cache×
- `metric_cost_per_million`：Cost/1M

Cost 列用已有 `positive_fg`。`Theme::adapted` 与「覆盖全部槽」测试要带上这两个新槽。`default_dark` 既有槽的 RGB 必须保持历史值。

选中行：行背景用 `selection_style` 的 bg；单元格自己设 fg 时保留厂商色。不要再给第 0 行单独套 accent。

### Format

| helper | 规则 | 谁用 |
| --- | --- | --- |
| `stat_compact` | 现有契约 | token / events |
| `cost_compact(f64)` | 非有限或 <0 → `$0.00`；≥1000 → `$12.3K`；否则 `$12.59` | Models Cost |
| `cache_multiplier(read, input, write)` | paid=`input+write`；paid=0 且 read>0 → `∞`；都为 0 → `—`；否则 `{:.1}x` | Cache× |
| `cost_per_million(cost, total)` | total=0 或成本非有限 → `—`；否则 `$` + 两位小数 | Cost/1M |

不要改 `format::cost`（`$x.xx`）。CLI 报表继续用自己的格式。

### TUI runtime

- Models 不再使用 `collapse_plan`。`tui-runtime-contracts.md` 改为：仅 Cost 在未排序时折叠；Models 始终用原始行数。
- `AppState::new` 把 `sort[Models]` 设为 Cost 降序。`cycle_sort` 从 Cost 下一步是 Tokens（`sort_keys` 仍是 `[Tokens, Cost]`）。`sort_state_is_remembered_per_panel` 现有断言仍然成立。
- `models::render` 的无 sort 重载给测试用时，应显式传入默认 Cost 降序，或测试改走 `render_with_plan`。属性测试改为断言紧凑成本字符串，不再断言 `{:.4}`。

### Width

约束对齐 tokscale 的「数字列 Length、Model 吃剩余」：

宽表（`area.width >= 80`）：`#` Length(3)，Model Min(16)，Provider Length(10)，Source Length(12)，各度量 Length(8–10)。

## Compatibility

- `PublicModelBreakdown` 字段集不变。多出来的 `sources` 只留在内部 DTO。
- `/api/models` 顺序仍是 token 降序。web 前 8 条条形图不因 TUI 默认改 Cost 排序而换序。
- Cost 面板、`longtail.rs` API、其他面板 sort 默认值不变。
- 中文文档不因 TUI 英文表头改写。

## Trade-offs

| 选择 | 取舍 |
| --- | --- |
| TUI 内存按 Cost 排，SQL 仍按 token 排 | web 顺序稳定；TUI 多一次稳定排序，行数通常几十，可接受 |
| 厂商色不进 Theme 四套调色板 | 跨主题识别稳定；mocha/graphite 下厂商色不会跟主题走 |
| 选中行保留单元格 fg | 对齐 tokscale；与当前整行 `selection_fg` 不同，仅 Models |
| 公开 JSON 不加 `sources` | 少一条兼容面；web 本任务也不展示 Source |

## Rollback

改动集中在 `model_breakdown` 增列、`theme` 增槽、`panels/models.rs` 重画、runtime 取消 Models 折叠。回滚这几处即可。无 migration。无破坏性 store 变化。
