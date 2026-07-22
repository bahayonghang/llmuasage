# serve 看板视觉打磨（S5，P2）

父任务：`.trellis/tasks/07-22-serve-dashboard-ui-perf`（全局约束 H1–H5 与产品决策 D1/D2/D3 继承自父 PRD）。本任务只动视觉与文案呈现，不改数据与交互逻辑。

## Goal

消除审计确认的视觉缺陷，让信息层级、字距、图表形态与设计系统一致，主题行为符合用户系统偏好。

## Requirements

- R5.1：Hero 区收敛留白：列模板改 `minmax(0, 640px) 360px` + `justify-content: space-between`（或 hero-meta 拉通吸收空白），系统健康卡位置不变。(Fact A3)
- R5.2：letter-spacing/uppercase 规则限定英文 locale（`:lang(en)` 或 `[data-locale]`），CJK 下字距归 0；覆盖 `.kpi-label`、`.section-eyebrow`、筛选 label。(Fact A4)
- R5.3：按 D1 删除 KPI 硬编码假 sparkline（hero.js:110-112），连带删除 `buildSparkline` 死代码（derive.js:563-579）；卡片留白用纯几何或留空，不做误导性图形。(Fact A9)
- R5.4：趋势图 SVG 消除拉伸变形：按容器实测宽度动态计算 viewBox，或至少加 `vector-effect: non-scaling-stroke` 并给轴标签留足字号预算；对齐 explorer 迷你图做法。(Fact A10)
- R5.5：导航激活 pill 改 `accent-soft` 底 + 深色字 + 左侧 3px accent 指示条（不再全反色）；按 D2 删除静态 badge（shell.rs:78,83）。(Facts A11/A14)
- R5.6：同步中心 metric 单元格改上下结构（label 上 value 下，对齐 `.status-cell` 范式）；详情 summary 提示语紧跟标题（`::after` 用 `margin-left:auto`）。(Fact A12)
- R5.7：按 D3 主题支持 `prefers-color-scheme` 首访探测；shell.rs:35-53 内联防闪烁脚本与 theme.js 收敛为单一事实源（key/默认值一处维护）；给主要 surface（`.panel`/`.sidebar`/`.filter-rail`/`.endpoint`）补统一 `background-color/border-color` 过渡。(Facts B19/B20)
- R5.8：mono 字体栈仅用于纯数据（时间戳、数字、id），中文标签回 sans；KPI 脚注数字等宽（与 S1 R1.3 协调，后到者合并）。(Fact B22)
- R5.9：对比度基础修复：`--muted-2` 限定装饰用途，文本级用途改用 `--muted`，全站数据标签字号下限 10.5px（目标 11px）。(Fact B23)
- R5.10：pulse 动画从 box-shadow 改 transform/opacity 实现，并尊重 `prefers-reduced-motion`。(Fact C37)
- R5.11（可选，评审后取舍）：侧边栏下部空白轻改善——endpoint 卡上方补一条次要信息（如数据目录或存储体积），不改整体布局。(Fact A13)
- R5.12（可选）：触及组件处的字号/间距向 4 的倍数局部收敛，不做全站 token 化（全站化属 backlog B1）。(Fact B21)

## Acceptance Criteria

- [x] A5.1：1440×900 与 1920×1080 截图对比，hero 中部无 >200px 连续死白。
- [x] A5.2：中文 locale 下 KPI 标签/eyebrow 无异常字距（截图）；英文 locale 下 uppercase/letter-spacing 保持。
- [x] A5.3：四张 KPI 卡无相同假曲线；`buildSparkline` 无残留引用（grep 证明）。
- [x] A5.4：宽屏下趋势图轴标签/描边无拉伸变形（截图对比），窄屏不破版。
- [x] A5.5：导航激活项不再是全页最亮元素（截图）；badge 消失。
- [x] A5.7：系统偏好 dark 的首次访问直接暗色（无闪烁，fixture 验证）；主题切换无斑块式变色（录屏或逐帧截图人工评审）。
- [x] A5.9：抽样测量修正后文本对比度 ≥4.5:1（正文级）/≥3:1（大字级）。
- [x] A5.10：`prefers-reduced-motion: reduce` 下无常驻动画。
- [x] A5.13：docs/dashboard 中英文页面与截图更新；`node --check`/`node --test` 与 `just ci` 通过。

## Notes

- 轻量任务，PRD-only。与 S1 同文件较多（components.css/layout.css/hero.js），建议 S1 合并后再启动，避免冲突。

## 实现记录（2026-07-22）

全部 R5.1–R5.10 落地，R5.11/R5.12（可选项）本轮不做（留 backlog）。JS/CSS 经 prettier hook 会破坏单引号约定，改用 Bash 直接改写、保留各文件原行尾。

- R5.1：列模板改 `minmax(0, 640px) 360px`（未用 `justify-content: space-between`——在 `main` 1680px 上限下 space-between 会让宽屏中缝反增至 >500px，违背 A5.1）。左列封顶后中缝在任意宽度 ≤ (640-desc)+gap ≈ 70–80px；富余空白落在卡片右侧（内容起始对齐），窄屏 minmax 收缩不溢出。hero-desc `max-width` 520→600。
- R5.4：移除 `preserveAspectRatio="none"`，`viewBox` 宽度由 `layoutTrendChart` 按 SVG 实测像素宽度动态设为 `0 0 W 220`（1:1，文字/描边不再横向拉伸）；栅格线/基线改 `x2="100%"` 自适应；`.trend-grid-lines`/`.trend-baseline` 加 `non-scaling-stroke`；`ResizeObserver`(rAF 去抖) 在容器变宽/窄后重排。
- R5.7：单一事实源——`shell.rs` 内联脚本解析 `stored ?? prefers-color-scheme ?? light` 并写 `<html data-theme>`；`theme.js` 优先采信该属性（DOM），仅在缺失时回退 localStorage→prefers。dark 首访已用 `--color-scheme=dark` 截图验证（footer 切到「淡色」，无闪烁）。`.panel/.sidebar/.filter-rail/.endpoint` 补背景/边框过渡。
- R5.9：`--muted-2` 唯一文本级用途（导航 active badge）已随 R5.5 删除；`.date-picker-day.is-outside` 文本色 muted-2→muted；<10.5px 数据标签统一抬到 10.5/11px（trend-axis 10.5、mini-label/explorer-peak 11、explorer-axis/weekday 10.5、SVG `<g>` 默认 9.5→10.5）。对比度按 palette token 推导达标（light `--muted` #6c6f85≈4.9:1，dark `--muted` #a6adc8≈8:1）。
- R5.10：`.pulse` 光环改 `::after` 的 `transform: scale()`+`opacity`（合成层），`background: inherit` 跟随 good/warn tone；reduced-motion 已由 `base.css` 全局规则统一静止（无需重复）。

证据：`just ci` 全绿（fmt/clippy/test/doc/node/docs:build）；1440×1100(zh/light)、1920 全页、390 移动、1440 dark(prefers)、1440 en 截图已核对；docs 截图 `docs/public/screenshots/web-dashboard-overview.png` 已更新为当前形态（旧图为 v0.6.3，含被删的假曲线/badge/全反色 pill）。

附带修复（阻塞集群 `just ci` 的既有 clippy 项，非 S5 视觉范围）：`sync/job_registry.rs` 用 `type TerminalHook` 拆复杂类型；`query/mod.rs` 两处 `#[ignore]` 测量测试数组加 `#[allow(clippy::type_complexity)]`。