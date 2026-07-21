# TUI 紧凑数字显示

## Goal

让 `llmusage dash` 中数量级较大的 token 与分析型计数可以被快速扫读，避免截图中
`18,214,785,227`、`1,029,915,980` 这类长数字占据主要视觉宽度；显示层改用紧凑量级，
但底层数据、计算、排序和查询结果保持不变。

## Background

- 截图中的 Overview 将 Total Tokens、24h Tokens、Token Mix、Events 等指标全部按千分位完整展开。
- `src/tui/panels/overview.rs:10` 将 `grouped` 别名为 `format_tokens`，因此 token 和 event
  都走精确千分位；同一文件的 KPI、Token Mix、Recent Activity 和 24h Pulse 均复用该路径。
- `src/tui/format.rs` 已有多套不同契约：`grouped` 精确千分位；`tokens` 在 10K 后输出一位
  小数的 `k/M` 且不支持 B；`footer_compact` 规则相似；`token_compact` 为 CLI report table
  输出两位小数的 `K/M/B`。`tui-presentation-contracts.md` 明确要求不同阈值、精度或后缀大小写
  的 helper 不得被误合并。
- Models、Daily、Hourly、Cost、Stats、Behavior、Blocks 仍混用 `grouped` 与 `tokens`；排序在
  `SortState` 上对原始数值执行，不依赖渲染字符串。
- Web dashboard 已使用一位小数、可进位的 `K/M/B/T` 紧凑格式；它是可参考的产品行为，
  但本任务不修改 Web，也不跨语言共享实现。

## Requirements

### R1. 交互式统计量级格式

- 新增一个名称和契约清晰的交互 TUI 紧凑统计 formatter。
- 采用十进制量级：`K = 10^3`、`M = 10^6`、`B = 10^9`、`T = 10^12`。
- 绝对值小于 1,000 时保留整数；达到量级后最多显示一位小数，并去掉末尾 `.0`。
- 四舍五入后达到下一量级时必须进位，禁止出现 `1000K`、`1000M` 或 `1000B`。
- 正确处理 0、负值、量级边界及 `i64::MIN`，不得因取绝对值溢出。
- 后缀采用大写 `K/M/B/T`，以对齐 Web 和 CLI report table。

示例：

| 原值 | 显示 |
| ---: | ---: |
| `999` | `999` |
| `1,000` | `1K` |
| `12,500` | `12.5K` |
| `288,694,891` | `288.7M` |
| `18,214,785,227` | `18.2B` |
| `1,000,000,000,000` | `1T` |

### R2. 应用范围

- Overview：Total/24h token KPI、Token Mix、Recent Activity、24h Pulse 及其中的 Avg/event、
  Events、Buckets 等分析型指标。
- Models、Daily、Hourly、Cost、Stats、Behavior、Blocks：token 总量、事件/调用/会话/轮次等
  面向比较和扫读的统计列。
- Footer：总 token 摘要与面板采用同一量级风格。
- Sources 数量等通常很小的分类基数仍可走同一 formatter，但小于 1,000 时输出不变。
- Usage 同步中心的 `events_seen`、`inserted_delta`、`stored_events`、`skipped_files` 等诊断计数
  保持精确整数，便于同步对账；如一个字段兼具分析与诊断语义，以精确值优先。

### R3. 保持不变的输出

- 成本、百分比、时间戳、模型名、来源名等非 count/token 值不改格式。
- 非交互 CLI report table、statusline、JSON、Web dashboard 的文本契约不变。
- `grouped` 与 report-table `token_compact` 保留其现有命名契约；不得借本任务统一其精度。
- 不增加显示模式开关、配置项、tooltip 或精确值弹窗。

### R4. 数据语义与交互不变

- 不改 query payload、SQLite、token accounting、成本计算或同步行为。
- 排序、长尾折叠、选择、滚动继续基于原始数值；不得解析格式化后的字符串参与逻辑。
- 宽屏与窄屏只改变可见列，不改变同一指标的量级格式。

### R5. 测试与契约

- formatter 需要表驱动边界测试，覆盖每个量级、进位、负值、0 和 `i64::MIN`。
- Overview 需要使用截图量级数据的 `TestBackend` 渲染回归，断言紧凑值出现且长精确值不再出现。
- 受影响的表格渲染测试必须按新的显示契约断言，不得在测试中继续复制旧千分位算法。
- 更新 `tui-presentation-contracts.md`，记录新的 helper 签名、阈值、精度、后缀和适用边界。

## Acceptance Criteria

- [x] A1：`format` 单测证明 `<1K`、K/M/B/T、进位、负值和 `i64::MIN` 均符合 R1。
- [x] A2：Overview 在 120x30 和窄布局下分别渲染 `18.2B`、`288.7M` 等紧凑值，且不再渲染
  对应的完整千分位 token 字符串。
- [x] A3：R2 列出的分析型 count/token 调用点统一使用新 formatter；Usage 同步诊断计数仍为精确值。
- [x] A4：Models、Daily、Hourly、Cost、Stats、Behavior、Blocks 的代表性大数渲染测试通过，
  原始排序顺序、选择行与长尾折叠行为不变。
- [x] A5：现有 CLI report-table compact 与 grouped 测试逐字节不变；成本和百分比测试不变。
- [x] A6：`cargo fmt --all -- --check`、`cargo clippy --all-targets --all-features -- -D warnings`、
  focused TUI tests、`cargo test -- --test-threads=1` 全部通过。

## Out Of Scope

- Web dashboard、普通 CLI 表格、JSON/API 的格式改版。
- token/cost 统计口径、数据库、同步或查询性能调整。
- 新增精确值切换、悬浮提示、复制功能或用户配置。
- 对成本使用 `$K/$M` 量级，或改变百分比小数位。
