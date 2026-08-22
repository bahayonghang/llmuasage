# Technical Design

## Boundaries

本任务不改变数据库、Rust DTO 或 API schema。修复发生在现有浏览器投影的消费边界：

```text
run_log + source_sync_status + worker_lock
  -> Query::sync_command_center
  -> /api/dashboard.sync_command_center
  -> normalizeSyncCommandCenter
  -> renderHero + renderSyncCommandCenter
```

通用 `health.recent_failures` 与 `diagnostics.recent_failures` 继续用于历史运行诊断和洞察，不再驱动 hero 的当前同步 tone。

## Status Contract

- `renderHero` 从 `context.syncCommandCenter` 读取 `tone`、`headline_key`、`last_run` 与 `metrics.sources_ready/sources_total`。
- UI tone 仅接受 `good`、`warn`、`neutral`；未知值降级为 `neutral`。
- pill 使用简短的本地化状态词，详情/移动 summary 使用既有 `syncCenter.headline.*` 文案。
- 最新同步结果由结构化 `last_run.status` 映射成本地化文案；缺失时显示 `--`。
- hero render key 加入 `sync_command_center`，确保 worker lock、latest run 或 source readiness 改变时重新渲染。

## Sidebar Footer

- 保留 `#toggle-theme`、`#toggle-locale`、`#endpoint-host`、`#endpoint-sync`，避免破坏现有事件绑定。
- markup 增加轻量标签/分组结构，CSS 将两个控制按钮放进同一个 segmented surface，并提升 endpoint 的地址、状态点和时间层级。
- `renderHero` 是 endpoint 地址和最近同步时间的唯一数据渲染者。
- `updateSyncButton` 不再把 `snapshot.summary` 写入 endpoint。自动刷新和 job 错误仍进入现有加载状态、同步命令中心和按钮状态，不占用窄侧栏。

## Compatibility

- 旧 snapshot 没有 `sync_command_center` 时由现有 normalizer 生成 neutral/empty 状态。
- live 与 static shell 复用同一 embedded assets，不新增资产或 manifest 项。
- 720px 断点维持 endpoint 隐藏，按钮退化为图标/短标签；桌面 248px 侧栏不依赖水平滚动。
- public dashboard 不携带本地同步详情；现有空投影继续显示中性状态，不扩展敏感信息。

## Testing Strategy

- Rust embedded-asset tests 固定 shell 结构、hero 状态数据源、render-key 依赖与禁止 summary 写入 endpoint 的约束。
- 现有 query 测试继续证明 `serve` noise 不影响 sync command center，最新 sync 失败仍告警。
- Node syntax/render lifecycle 检查覆盖模块依赖与指纹变化。
- 本地浏览器在桌面宽屏和 720px 以下验证视觉、文案、无溢出及交互焦点。

## Rollback

改动只涉及静态资产、shell 模板、文档和测试，可按文件回退；没有数据迁移或持久化状态回滚。
