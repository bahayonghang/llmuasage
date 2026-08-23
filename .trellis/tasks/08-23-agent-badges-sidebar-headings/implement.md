# Implement：官方 Agent Logo 徽章与侧边栏分组标题

## Preconditions

- [ ] 用户已审阅并明确批准最新的 PRD、Design 和本实施清单；批准前不得运行 `task.py start`。
- [ ] 阅读 `research/official-agent-logo-assets.md`、`.trellis/spec/llmusage/backend/web-server-contracts.md` 与 `dashboard-performance-contracts.md`。
- [ ] 工作区除本任务规划目录外无不明改动；保留所有用户拥有的无关变更。
- [ ] 十个来源的一手官方矢量资产均已定位并满足许可/商标与安全准入；任一缺失时停在资产门，不先改产品代码。

## 1. Admit and record official SVG assets

- [ ] 为十个稳定 ID 建立 `src/web/assets/agent-logos/<stable_id>.svg`，另建未来来源用 `fallback.svg`；只从研究表中的一手来源取得。
- [ ] 新建 `src/web/assets/agent-logos/ATTRIBUTION.md`，记录官方来源、固定版本/提交、获取日期、许可证/商标、SHA-256 和本地文件名。
- [ ] 逐项审查 SVG：自包含、无脚本/事件/`foreignObject`/外部引用/data URL/动画，保持官方 path、viewBox、比例和颜色。
- [ ] 核对单文件与总量预算；不要引入 SVGO、图标包或其他新依赖。

**Gate A — asset admission**

- [ ] 十个现有来源均为官方 Logo，出处与校验摘要完备；否则停止并报告缺失项。

## 2. Extend the embedded asset pipeline

- [ ] 在 `src/web/assets/mod.rs` 将十个 Logo 与 fallback 加入 `ASSET_MANIFEST`，统一 `image/svg+xml`；保持既有 ETag、cache、压缩和查找行为。
- [ ] 补 Rust 测试验证 manifest 唯一路径、HTTP 200/Content-Type/ETag、静态导出覆盖，以及 SVG 安全和出处清单完整性。
- [ ] 如固定数组长度造成机械脆弱，只在该文件内采用最小改法；不要扩展成新的通用资产框架。

**Gate B — pipeline**

- [ ] 聚焦运行 Web asset tests，证明每个 Logo 在 live 服务和导出链路均可离线取得。

## 3. Add the registry-driven Web badge catalog

- [ ] 在 `src/web/shell.rs` 从 `registered_source_descriptors()` 生成 `{id, display_name, logo_url}` 目录并安全序列化到 `application/json` script block。
- [ ] 保留现有 `data-supported-sources` 兼容属性；当前来源使用约定 SVG 路径，未来缺失资产使用 fallback。
- [ ] 补 live/snapshot shell 测试：十项顺序与注册表一致、展示名存在、JSON 安全、目录缺失不会影响旧属性。

**Gate C — catalog**

- [ ] Rust 聚焦测试通过，且没有修改 `SourceDescriptor`、API 或数据 payload。

## 4. Render semantic Agent badges

- [ ] 重构 `src/web/assets/render/hero.js`：保留生成时间和最近同步；来源项渲染本地化计数摘要与语义列表，每项含装饰性官方 Logo、可见展示名和稳定 `data-source`。
- [ ] 抽出可用 Node 测试覆盖的纯解析/渲染函数；覆盖十项、空目录、损坏 JSON、未知来源 fallback、长名称与 HTML 转义。
- [ ] 在 `src/web/assets/copy.js` 增加中英文“当前筛选有数据 N / 已支持 M”文案，不使用就绪/在线状态术语。
- [ ] 徽章保持非交互；不得加入点击筛选、tooltip 菜单、`tabindex` 或按钮角色。

**Gate D — behavior**

- [ ] Node 聚焦测试与 `node --check` 通过，Hero 的其他状态卡渲染不回归。

## 5. Implement badge and sidebar-title styling

- [ ] 在 `layout.css` 让来源元信息独占一行、徽章组可换行，并保持 1100/720px 现有布局断点。
- [ ] 在 `components.css` 实现统一徽章底板、Logo 承载区、展示名、间距和 fallback；官方 Logo 不被拉伸、裁切、滤镜改色。
- [ ] 将 `.nav-label` 实现为短 accent 起始线 + 标题 + 中性延伸线的 rail heading；保留现有三组语义和 `<=720px` 隐藏规则。
- [ ] 只在需要时补充 `base.css` 语义 token；不得将十个品牌色扩散为页面装饰色。
- [ ] 定点更新 `DESIGN.md` 的 Agent 徽章和侧栏标题条款，不处理无关历史视觉冲突。

**Gate E — static checks**

- [ ] CSS 无横向溢出诱因、无伪交互、无连续动画；ZH/EN 与 light/dark 共享同一结构。

## 6. Automated validation

- [ ] `rtk node --check src/web/assets/render/hero.js`
- [ ] 运行新增/相关 Node tests。
- [ ] `rtk cargo test web -- --test-threads=1`
- [ ] `rtk cargo test --test local_flow -- --test-threads=1`
- [ ] `rtk cargo fmt --check`
- [ ] `rtk git diff --check`

## 7. Bounded visual verification

- [ ] 用仓库文档看板夹具启动本地服务，使用同一数据集成批截图：1280、1440、1920 桌面宽度下的 light/dark × zh/en，以及一个 `<=720px` 窄屏。
- [ ] 第一轮一次性检查：十个 Logo 正确、官方比例、徽章换行、计数语义、状态卡不被挤压、侧栏标题层级、英文长度、窄屏无横向滚动、标题隐藏、非交互观感。
- [ ] 批量修复发现的问题后最多进行一次确认截图轮；未取得的人工视觉证据标为 `UNVERIFIED`，不得用 DOM 测试替代。

## 8. Full quality gate and rollback check

- [ ] `rtk just ci`
- [ ] 检查 live 与 snapshot/export 均不含远程 Logo URL，所有 Logo 请求均走本地 `/assets/`。
- [ ] 检查最终 diff 只包含任务规划、相关 Web 代码/测试、Logo/出处资产与定点 `DESIGN.md` 更新。
- [ ] 验证回滚点：移除 catalog、徽章 markup/CSS 与 manifest Logo 项后可恢复旧斜杠文本，不涉及数据迁移。

## 9. Finish gates

- [ ] 使用 `trellis-check` 做规范、需求、测试与视觉证据复核。
- [ ] 评估是否需要通过 `trellis-update-spec` 补充 Web SVG asset contract；仅记录真正可复用的新契约。
- [ ] 按仓库规范生成窄范围中文 emoji Conventional Commit；不 push、不创建 PR，除非用户另行授权。
