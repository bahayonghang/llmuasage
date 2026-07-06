# 设计 — TUI 展示观感升级（Child B）

## 受影响文件

```
src/tui/theme.rs            ← B2 主题结构 + 槽位（核心重构点，控制爆炸半径）
src/tui/panels/stats.rs     ← B1 热力图网格 + B3 排行/进度条着色
src/tui/report_table.rs     ← B3 render_bar 三阶（若共用）
src/tui/app.rs / mod.rs     ← B4 新 Panel 枚举项 + 数据加载
src/tui/nav_bar.rs          ← B4 导航条标签
src/tui/panels/{blocks,sessions}.rs  ← B4 新面板（新文件）
```

## B1 热力图网格
- 数据：`payload.heatmap: Vec<HeatmapPoint>`（已按日、已补零）。
- 布局：列=周，行=星期几（0..6）。由 `HeatmapPoint.date` 推 weekday（chrono 已依赖）。
  首列按当日回推，右对齐到最新。宽度 = `min(周数, inner.width/1)`。
- 分档：预计算分位（P25/P50/P75/P99）→ 5 档；档位 → 主题 `heat[0..5]`。
  空档（无数据）用 `heat[0]`（muted）。
- 表头：顶行渲染月份缩写，`cell_len` 感知对齐（当前是 ASCII，中文暂不涉及）。
- 降级：`inner.height < 8` 时回退现有单行 strip（保底不 panic）。

## B2 主题系统（爆炸半径控制）
- 新增 `struct Theme { accent, positive, warn, error, muted, border_active,
  border_normal, row_alt_bg, bar_ok, bar_warn, bar_danger, heat:[Color;5],
  kpi:[Color;4], ... }`。
- 提供 `Theme::default_dark()`（= 当前 const 值，保证零观感回归）与
  `Theme::catppuccin_mocha()`（或 dracula）。
- **迁移策略**：`theme.rs` 现有 `pub fn header_style()` 等构造器改为读"当前主题"。
  最小侵入方案：进程内 `OnceCell/RwLock<Theme>` 或经 `AppState` 透传。优先 **AppState
  持有 `theme: Theme`**，构造器签名不变者用全局 `active_theme()` 读取，避免改上百处调用。
- 切换：input.rs 加 `Action::CycleTheme`（快捷键如 `t`）；`--theme <name>` 启动参数。
  持久化：若存在配置文件则写入，否则仅会话内（design 时确认配置模块）。

## B3 进度条/排行着色
- `render_bar(value, max, width)` → 加 `render_bar_graded(pct, width)`：按 pct 选
  `bar_ok/warn/danger`。占比/预算类调用新函数；纯量级柱状图保持单色（主题 accent）。
- 排行：Models/Cost/Source Mix 渲染时 `index==0` 用 accent+BOLD，其余 muted；
  过滤 `share <= 2%` 的尾项（对齐 token-tracker），并在末尾提示"+N more"。

## B4 新面板（可选）
- `Panel` 枚举加 `Blocks` / `Sessions`；`from_digit_char` 扩展；nav_bar 加标签。
- 数据：直接调 `Dashboard::*`（reports/session 已有），走现有 lazy-load + cache
  失效机制（`app.rs` 的 `Option<Result<T,String>>` 模式），**不新增 query**。
- 渲染复用 `report_table` 现成表格渲染，尽量零新造轮子。

## 兼容与回滚
- B2 默认主题 = 现值 → 视觉零回归；回滚 = 移除 Theme 结构、恢复 const 直引用。
- B1 有单行 strip 降级路径。
- B4 为纯增量面板，回滚 = 移除枚举项与文件。

## 测试策略
- 渲染快照/宽度矩阵：宽(≥84)/窄(<84)/极窄(<54) 下 B1 网格与新面板不溢出、不 panic。
- 分位分档纯函数单测；`render_bar_graded` 阈值边界（49/50/80/81%）单测。
- 主题切换：切换后关键面板颜色取自新主题（可对 style 断言）。
