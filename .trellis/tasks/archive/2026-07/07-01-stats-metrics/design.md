# 设计 — Token 统计口径增强（Child A）

## 边界与数据流

```
parsers/{claude,codex,opencode,antigravity}.rs   ← 采集 used tokens（已有）+ model
        │  (A1: 需补 model_max 映射)
        ▼
domain/models.rs (UsageTokens / UsageEvent)       ← A1 新增派生 context_percent
        │
store/migrations.rs (v13 → v14)                   ← A1 若持久化则加列/或查询期计算
        │
query/mod.rs (Dashboard)                          ← A1/A5/A3 新增/扩展查询方法
        │
tui/panels/stats.rs, overview.rs                  ← 展示（A1 峰值/均值, A5 longest）
```

## A1 context window 利用率

### 关键决策（design 定稿项）
1. **used 口径**：优先"最近一次请求的 input+cache_read+cache_creation"（≈ 提示侧
   上下文），不含历史累计。需在实现时对单一 parser（Claude）验证字段可得性，
   其余 parser 尽力而为，缺失则 `None`。
2. **model_max 来源**：新增 `model_context_window(model) -> Option<u32>` 映射。
   - 复用 `src/query/pricing_catalog.rs` 结构：在 `PricingEntry` 或平行 catalog 中
     加 `context_window` 字段（静态 `pricing/static-v1.json` 补该键）。
   - 未收录模型走与 pricing 相同的 prefix/suffix 匹配 → `None` 降级。
3. **持久化 vs 计算期**：MVP 选**查询期计算**（不改 schema），只在 query 层用已有
   token 字段 + catalog 算 percent，避免迁移成本。若后续需要历史趋势再上迁移。

### 契约
- `query`：新增 `context_pressure(filter) -> ContextPressurePayload { peak_percent,
  avg_percent, sample_count, unpriced_count }`。
- 无 model_max 的事件计入 `unpriced_count`，不拉低均值（分母只含可算样本）。

## A5 longest streak
- 纯函数：`fn longest_streak(heatmap: &[HeatmapPoint]) -> usize`，与
  `current_streak`（`stats.rs:366`）并列。遍历按日期升序，累计连续 `event_count>0`
  段取最大。
- 展示：`stats.rs:132` 那行 `current streak {n}d` 改为 `{current}/{longest}d`。

## A3 session active vs span（条件实现）
- 先查 `session.rs` + `reports.rs` 的 `duration_minutes`（`reports.rs:125` 是 block
  级，非 session 级）。
- 若 session 无 gap-capped active：新增纯函数按事件时间序列，`ACTIVE_GAP_CAP=30min`
  累加 gap≤cap 的间隔；span=末-首。放 `query` 层，输出 `active_minutes` /
  `span_minutes`，session 报告与（可选）TUI 展示 `active/span`。

## 兼容性与回滚
- A1 走查询期计算 → 无 schema 变更，回滚 = 移除 query 方法与展示行。
- A5/A3 为纯增量函数，回滚独立。
- 全部对现有数值零影响（只新增字段/展示）。

## 测试策略
- 单测：`longest_streak` 边界；`context_pressure` 的 unpriced 降级；`active_minutes`
  的 30min gap 切分。
- 渲染：Health 面板宽/窄布局下新字段不溢出（复用现有 narrow 逻辑）。
