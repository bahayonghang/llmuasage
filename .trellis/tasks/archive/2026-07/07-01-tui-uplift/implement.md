# 执行计划 — TUI 展示观感升级（Child B）

> 本轮只出规划；以下为待批准后的执行顺序。每步含验证与回滚点。

## 前置确认（gate 0）

- [ ] 通读 `src/tui/theme.rs`、`stats.rs`、`report_table.rs`、`app.rs`、`input.rs`、
      `nav_bar.rs`，确认颜色引用面与 `Panel` 扩展点
- [ ] 确认是否有配置模块可持久化主题（决定 B2 切换是否落盘）

## 步骤 1 — B2 主题骨架（先做，后续步骤复用槽位）

- [ ] `theme.rs`：定义 `Theme` 结构 + `default_dark()`（= 现值）+ `active_theme()` 访问器
- [ ] 现有 `*_style()` 构造器改读 `active_theme()`，**保持默认观感像素级不变**
- [ ] 单测/目视：默认主题下所有面板与改前一致
- 验证：`cargo test` + `llmusage tui` 逐面板目视对比
- 回滚：恢复 const 直引用

## 步骤 2 — B2 第二套主题 + 切换

- [ ] 加 `catppuccin_mocha()`（或 dracula）
- [ ] `input.rs` `Action::CycleTheme` + 快捷键；`--theme` 启动参数；（可选）落盘
- 验证：运行时切换即时生效、无重启
- 回滚：移除第二主题与切换 action

## 步骤 3 — B1 热力图网格

- [ ] `stats.rs`：`render_contribution` 升级为 7×N 网格 + 分位分档（用主题 heat[5]）
- [ ] 月份表头 + 窄屏/低高降级回单行 strip
- [ ] 单测：分位分档纯函数；空数据降级
- 验证：宽/窄/极窄 + 空数据矩阵目视
- 回滚：还原单行 strip 实现

## 步骤 4 — B3 分级着色与排行

- [ ] `render_bar_graded(pct,width)` 三阶；占比/预算类切换调用
- [ ] Models/Cost/Source Mix：榜首高亮 + ≤2% 长尾过滤 + "+N more"
- [ ] 单测：阈值边界 49/50/80/81%
- 验证：目视三面板
- 回滚：恢复单色 render_bar 调用

## 步骤 5 —（可选）B4 Blocks/Sessions 面板

- [ ] `Panel` 枚举 + `from_digit_char` + nav_bar 标签
- [ ] 新面板文件，复用 `Dashboard::*`（reports/session）+ 现有 cache/刷新，零新 query
- [ ] `report_table` 复用渲染
- 验证：数字键跳转、刷新、窄屏
- 回滚：移除枚举项与文件

## 收尾（gate final）

- [ ] `cargo fmt --check` && `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test -- --test-threads=1`
- [ ] `task.py finish` → parent 汇总

## 审查门

- 步骤 1（主题骨架）完成后暂停：确认"默认观感零回归 + 迁移策略"获认可，再推进 2–5。
