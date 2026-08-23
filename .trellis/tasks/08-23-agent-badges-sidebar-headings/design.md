# Design：官方 Agent Logo 徽章与侧边栏分组标题

## 1. Design intent and boundaries

该界面属于 **Operate** 模式：徽章首先帮助用户快速确认“当前筛选有多少来源产生数据、产品支持哪些 Agent”，品牌表达服务识别，不抢夺用量与状态数据的主视觉。此次是既有看板中的窄范围组件重设计，不替换整体视觉世界。

直接实现边界为：

- `src/web/shell.rs`：从权威来源注册表生成 Web 专用徽章目录，并保留现有来源 ID 数据属性。
- `src/web/assets/render/hero.js`：将来源元信息拆成计数摘要与独立徽章列表。
- `src/web/assets/{base,layout,components}.css`：徽章、换行和侧边栏标题样式。
- `src/web/assets/mod.rs`：把本地 SVG 纳入既有嵌入式 asset manifest。
- `src/web/assets/agent-logos/`：十个官方 Logo、一个未来来源 fallback 与出处清单。
- `src/web/mod.rs`、现有 Node 测试：结构、资产、安全、兼容和渲染回归。
- `DESIGN.md`：只更新 Agent 徽章与侧边栏分组标题契约。

不改变来源域模型、API、SQLite、同步、查询、导出数据形状和导航行为。

## 2. Source catalog and data flow

```text
SOURCE_DESCRIPTORS
  -> shell.rs Web badge catalog [{ id, display_name, logo_url }]
  -> safe application/json block in live/snapshot shared shell
  -> hero.js parse + validate
  -> count summary + semantic badge list

agent-logos/*.svg
  -> WebAsset manifest
  -> /assets/agent-logos/<stable_id>.svg
  -> live response + static export, with existing ETag/cache/compression behavior
```

### 2.1 Catalog contract

- 来源顺序、稳定 ID 和展示名始终来自 `registered_source_descriptors()`；不在 JavaScript 或 CSS 中复制十项来源清单。
- Web 层使用约定路径 `assets/agent-logos/<stable_id>.svg`。当前十项必须都有对应资产；未来新增来源如果尚无资产，则目录生成器指向 `assets/agent-logos/fallback.svg`。
- `shell.rs` 将目录序列化到 `<script type="application/json" id="source-badge-catalog">`。使用结构化 JSON 序列化并转义 HTML script 终止序列，禁止手工拼 JSON。
- 继续保留 `data-supported-sources`，避免破坏现有测试和潜在旧脚本；新渲染优先读取结构化目录，目录缺失或损坏时回退到稳定 ID 列表和通用 Logo。
- `OverviewPayload::source_count` 的真实语义是当前过滤范围内 `usage_bucket_30m` 的不同来源数。UI 文案明确为“当前筛选有数据 N / 已支持 M”，不使用“已就绪”或“在线”等状态词。

### 2.2 Rendering contract

- 时间元信息保持原结构；来源项增加语义类并独占 Hero 元信息下一行。
- 来源摘要先展示本地化计数，再渲染 `<ul class="agent-badge-list" role="list">`。
- 每项使用 `<li>` 包裹非交互 `.agent-badge`；Logo 为 `<img alt="" aria-hidden="true">`，因为相邻可见展示名已经提供文本等价。
- Logo 与展示名均使用可信目录值，并分别经过 URL/HTML 安全处理。徽章不设置 `tabindex`、按钮角色、点击光标或伪交互 hover。

## 3. Official asset admission and provenance

### 3.1 Authority order

每项资产按以下优先级选择：

1. 对应产品的官方组织仓库中直接发布的 SVG；
2. 对应产品官方站点或官方品牌资产页直接提供/内嵌的 SVG；
3. 官方组织明确发布的品牌标记 SVG。

不得使用 Simple Icons 等第三方集合、搜索结果镜像、百科文件、AI 生成图形或手工描摹。十个当前来源任一项缺少可核验的一手矢量资产时，实施在修改产品代码前停止并报告；`fallback.svg` 只服务未来未知来源。

### 3.2 Integrity and safety

- `agent-logos/ATTRIBUTION.md` 每行记录稳定 ID、显示名、官方页面/仓库、精确资产 URL、固定提交或版本、获取日期、上游许可证、商标注意事项、本地文件和 SHA-256。
- SVG 必须是静态自包含图形：禁止 `<script>`、`foreignObject`、事件处理器、外部 `href`/`xlink:href`、远程字体、data URL 和动画。
- 保持官方 path、viewBox、比例与颜色；只允许删除不影响图形的编辑器元数据/注释，不能重绘、拉伸、裁切或把 Logo 改成 llmusage 配色。
- 优先选择官方提供的单标记而非完整 wordmark，避免长徽章和错误联合品牌。若官方只提供 light/dark 两版，目录允许记录主题变体；否则使用中性 Logo 承载底板保证对比度，不反色或滤镜改造品牌图形。
- 单个 SVG 建议不超过 20 KiB，十项与 fallback 总计不超过 160 KiB；超出时先寻找官方精简标记，不通过引入构建依赖压缩。

## 4. Visual component design

### 4.1 Agent badge

- 28px 左右的紧凑矩形徽章，8px 圆角、1px 中性 hairline、`surface`/`surface-2` 低对比底色；不使用全彩填充胶囊。
- Logo 置于 18px 方形承载区，完整显示且 `object-fit: contain`；展示名使用 10.5–11px mono/label 字体。品牌色只存在于官方 Logo 自身。
- 徽章之间使用约 6px 间距并允许换行。没有阴影、发光、浮起动画或缩放 hover。
- 来源项在 `.hero-meta` 中 `flex-basis: 100%`；计数摘要与徽章组形成上下两级，避免十个 Logo 与生成时间挤在同一行。

### 4.2 Sidebar group heading

- `.nav-label` 改为 rail heading：短 accent 起始线、标题文字、向右延伸的中性 hairline，形成清晰的组分隔而不是按钮或胶囊。
- 标题保持 10–10.5px、600 字重和克制字距；中文不强制 uppercase，英文保持现有 uppercase 语义。
- 标题颜色高于纯装饰线、低于导航文字；激活导航仍是侧栏唯一高强调交互态。
- 不加入 Logo、独立图标、编号或新的文案，保留三个现有 `id`、i18n key 和 `aria-labelledby`。

## 5. Theme, localization, and responsive behavior

- Light/dark 使用同一几何结构；徽章底板、边框和文字来自语义 token。官方 Logo 不通过 CSS filter 强制反色。
- 中文与英文只改变可见文案；展示名使用注册表官方英文名，不翻译品牌名。
- `>1100px` 保持两列 Hero；十个徽章在左列自然形成一至三行，不影响右侧状态卡宽度。
- `<=1100px` Hero 已变单列，徽章组继续换行；`<=720px` 保持现有 `.nav-label { display:none }`，徽章不得造成页面横向滚动。
- 不增加连续动画；现有 `prefers-reduced-motion` 契约不变。

## 6. Compatibility and rollback

- live 与 snapshot 使用同一 HTML shell、目录和资产清单；旧数据快照不需要新增字段。
- 结构化目录解析失败时回退稳定 ID + fallback，页面不能因单个 Logo 缺失阻断 Hero 其他数据。
- 资产请求沿用现有 ETag、`Cache-Control: no-cache` 与压缩逻辑；SVG `Content-Type` 为 `image/svg+xml`。
- 回滚时可整体移除目录 JSON、徽章 markup/CSS 和 `agent-logos` manifest 项，恢复 `supportedSourcesLabel()` 斜杠文本；无需数据迁移。

## 7. Verification design

- Rust：目录覆盖注册表全部十项且顺序一致；展示名进入安全 JSON；未来未知来源走 fallback；live/snapshot 均含目录；每个 SVG 都可由 manifest/HTTP/静态导出取得。
- SVG 安全测试：逐项拒绝 script、事件属性、`foreignObject`、外部引用、data URL，检查 viewBox 与体积预算，并核对出处清单包含所有文件。
- Node：目录解析、损坏目录 fallback、HTML 转义、十项渲染、计数文案和非交互 markup。
- 视觉：同一夹具检查 desktop 1280/1440/1920 的 light/dark × zh/en，并检查一个 `<=720px` 视口；一次成批缺陷修复后最多一次确认轮。
- 完整门禁：聚焦测试先行，最后 `just ci`。

## 8. Trade-offs

- 选择十个本地官方 SVG 会增加二进制体积和静态请求数，但换来准确的品牌辨识、离线可用与可审计来源；体积预算、ETag 与压缩限制成本。
- 不把 Logo 放进领域 `SourceDescriptor`，避免 Web 展示资产污染来源域模型；代价是 Web 层按稳定 ID 约定资产路径，但来源枚举仍只有注册表一个真源。
- 不在本任务统一日志、来源分布和同步中心徽章，避免组件重设计膨胀；资产目录与 token 可供后续复用。
