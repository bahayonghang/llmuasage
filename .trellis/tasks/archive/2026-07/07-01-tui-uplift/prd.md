# TUI 展示观感升级（Child B）

Parent: `07-01-token-stats-tui-uplift`

## Goal

对标 `ref/token-tracker` 的 Rich TUI 观感，提升 llmusage `ratatui` 仪表板的信息层次
与视觉冲击，并把**已实现但仅在 CLI 可见**的报告接进交互式 TUI。

## 现状核对

- 单套硬编码调色板：`src/tui/theme.rs`（全 `const Color`）。
- 贡献热力图仅**单行 strip**：`src/tui/panels/stats.rs:159`（只写 `inner.y` 一行）。
- 柱状图/进度条单色 green：`stats.rs:321 render_bar`。
- burn-rate/blocks、sessions 报告已实现但**无 TUI 面板**（`app.rs` `Panel` 枚举 8 项）。

## Requirements

### B1 GitHub 风格贡献热力图（P1，视觉核心）
- 把 `render_contribution` 从单行升级为 **7 行（周日→周六）× N 周** 网格。
- 分档着色：按过去区间 token 分位（P25/P50/P75/P99）分 5 档深浅。
- 顶部月份表头 + 右侧总览（tokens/cost/active days/streak/peak）。
- 自适应终端宽度决定周数（对齐现有 narrow/very_narrow 逻辑）。

### B2 多主题系统（P2）
- 引入 `Theme` 结构（语义槽位：accent/positive/warn/error/muted/bar_ok/bar_warn/
  bar_danger/heat[5] 等），替换散落 `const Color` 直引用。
- 至少 2 套：默认（保持当前观感）+ 一套深色主题（Catppuccin Mocha 或 Dracula）。
- 运行时切换：TUI 内快捷键或 `--theme` 参数；持久化到已有配置（如适用）。

### B3 进度条/柱状图分级着色（P2）
- `render_bar` 支持三阶：<50% 绿 / 50–80% 黄 / >80% 红（用于占比/预算类）。
- 排行类（Models/Cost/Source Mix）：榜首高亮、其余压暗；过滤 ≤2% 长尾防溢行。

### B4 已有报告接入交互 TUI（P3，可选，高 ROI）
- 新增 Blocks 面板（burn-rate/projected/limit%，数据源 `reports.rs`）和/或 Sessions
  面板（`session.rs`）。复用现有 query，只做 TUI 渲染层。

## Acceptance Criteria

- [ ] B1：Health 面板热力图为 7×N 网格 + 分位分档 + 月份表头；空数据与窄屏优雅降级。
- [ ] B2：≥2 套主题可运行时切换，全部面板着色走主题槽位（无残留裸 `const Color` 关键路径）；
      切换即时生效不重启。
- [ ] B3：占比类进度条三阶着色；排行榜首高亮、长尾过滤生效。
- [ ] B4（若做）：新面板接入导航条与数字键跳转，复用现有缓存/刷新机制，无新查询逻辑。
- [ ] 无回归：8 面板（含新面板）在宽/窄/极窄布局不 panic、不溢出。
- [ ] `cargo fmt --check` / `clippy -D warnings` / `cargo test` 通过。

## 约束

- 纯展示层改动优先；B4 不得新增 query 逻辑（只消费已有 `Dashboard` 方法）。
- 主题重构避免大范围顺手改：只替换关键路径 `const Color` 引用，保持默认观感不变。
- `.rs` 编辑后跑 `cargo fmt`；渲染改动补 narrow/very_narrow 分支验证。
