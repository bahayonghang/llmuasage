# Design

## Target Layout

建议布局可按实际耦合微调，但每个模块必须对应一个用户可识别查询能力：

```text
query/
  mod.rs                 declarations, re-exports, Dashboard construction
  overview.rs            totals/context/trends and payloads
  breakdowns.rs          model/source/host/project/cost
  behavior.rs            activity/tools/optimize
  compare.rs             model comparison
  diagnostics.rs         health/diagnostics/sync command center
  snapshot.rs            full/core/interactive composition
  filter.rs, timezone.rs, pricing.rs
  explorer.rs, home_overview.rs, heatmap.rs, hour_of_week.rs,
  logs.rs, top_sessions.rs, reports.rs, inventory.rs
```

若 `behavior.rs` 仍超过 1,000 production lines，按 activity/tools/optimize 继续拆，而不是放宽边界。

## Visibility

- Public DTO 与方法保持原名，由 `query/mod.rs` 和 `lib.rs` re-export。
- Feature-local helpers 为 private；跨 sibling 共享才提升为 `pub(super)` 并放到最窄共同 ancestor。
- `Dashboard` 的 `conn`/`store` 不新增 clone/Arc；子模块中的 `impl Dashboard` 复用同一结构体与 connection。

## Migration Method

按 feature 一次迁移一个闭包：DTO → impl methods → helpers → tests。每步仅移动/修正可见性，不重写 SQL。使用 before/after symbol inventory、serde snapshot 和 statement trace 防止遗漏。

## Architecture Tests

复用 AST visitor 识别 query 层 forbidden imports。另加 symbol ownership fixture 或 source inventory，防止同一 public DTO/loader 在 root 和 feature 模块各保留一份实现。

## Performance

结构移动不引入并发。用现有 dashboard range harness 对同一 temporary representative copy 采样；若本轮没有授权/可用 copy，只能交付 synthetic parity，representative gate 保持 `UNVERIFIED`，不能归档 child。

## Rollback

每个 feature move 独立 commit/checkpoint。出现回归时只回退该 feature 的 re-export/module move，不回退已验证 feature。

