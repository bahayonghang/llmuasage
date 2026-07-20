# TUI 样式统一设计

## 语言与主题

界面文案统一英文。阶段二维护旧->新文案清单；不修改中文 README。所有颜色经 `Theme` 语义槽位，新增 input/output/cache-read/cache-write、selection 与 surface 槽位。默认 dark 槽位取现有硬编码值，保证阶段一视觉不变。

新增 Graphite 与 Lagoon 两套主题。`NO_COLOR`/`LLMUSAGE_NO_COLOR` 返回无 styling；truecolor 不可用时 RGB 经集中映射降级到 ANSI16。这两个机制独立，默认环境行为不变。

## 格式化与守护

新增 `tui::format`，集中 number、compact token、cost、percentage，先以现有调用点输出为回归契约。阶段一只做槽位/格式化重构并保存 TestBackend 基线；阶段二统一文案，样式 cell 必须相同。源码守护测试扫描 `src/tui` 面板，禁止面板直接构造 `Color::*`。

## 交互边界

保留 dead row highlight style，最终由 interaction 任务用 selection 槽位接线；本任务不实现选中状态。
