# Implement（S3 diagnostics 与 full 查询路径）

前置：prd.md（R3.1–R3.5 / A3.1–A3.7）与 design.md（D3.1–D3.4）已审定。**先测量、后方案**：R3.3 的组合器形态由基线数据决定，允许结论为"不实施"并记录。

## Checklist

- [x] 基线（research/baseline.md）：stress fixture（数千 source_file + 25 model）下：diagnostics stat 次数/耗时、full scope 各 section 分解、双并发 full 行为；如有真实库只读副本（复制到 target/tmp 下使用，绝不写原库、不记录敏感路径），补一组对照。
- [x] R3.2 WebState 级 diagnostics 缓存：web handler 层短 TTL（值由基线定，30–60s）+ sync job 终态主动失效 + single-flight 并发去重；`Dashboard::diagnostics()` 与 home_overview 零改动。
- [x] R3.3 full scope live 组合器决策：顺序化远超 full p95 20% 退化门槛，按 design.md 记录“不实施”；保留既有 1s timeout/degraded 语义，未直接使用 `Dashboard::snapshot()`。
- [x] R3.4 compare N+1 → 单条 GROUP BY，输出逐字段等价。
- [x] R3.5 web 读连接 busy_timeout 1–2s（仅 web/API 读路径，sync 写连接不动）。
- [x] 测试：缓存失效矩阵（TTL 内命中/过期重算/sync 失效/single-flight）、80ms cold 测试 debug+release 各 3 连、降级回归、双并发 full、compare 等价、busy_timeout 路径。
- [x] 复测：benchmark 脚本对照基线，research/after.md。

## Suggested Implementation Order

1. 测量设施（stat 计数包装/tracing + stress fixture seed）→ 基线。
2. R3.5（最小、独立）→ R3.4（独立、有等价测试）。
3. R3.2 缓存（含失效矩阵测试）。
4. R3.3 组合器（依基线结论）+ 双并发对照。
5. 复测 + 文档。

## Validation Commands

```bash
cargo test --all-features --lib -- --test-threads=1
cargo test --all-features --release --lib -- home_overview_under_80ms  # 连跑3次
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
node scripts/benchmark-dashboard-range.mjs --url <fixture-or-copy> --iterations 5
```

## 边界

- 不改 schema、不改 sync 写路径、不改 full/core/interactive 响应形状；80ms cold 契约不得用作豁免的借口（禁 process cache 进 query 层）。
- 真实库只读副本：用文件复制后打开（或 SQLite immutable/ro 模式），禁止任何写；报告只记规模与耗时，不记路径与内容。
