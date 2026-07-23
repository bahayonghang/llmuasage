# serve diagnostics 与 full 查询路径优化（S3，P1）

父任务：`.trellis/tasks/07-22-serve-dashboard-ui-perf`（全局约束 H1–H5 继承自父 PRD）。技术方案见同目录 `design.md`。**本任务先测量、后方案**，测量结果可能调整实现顺序。

## Goal

消除每次页面加载/范围切换/自动刷新都重复支付的 source_file 全表 stat 成本；压平 full scope 的连接扇出与并发竞争，同时**保留**现有逐 section 降级能力；顺带消除 compare 的 N+1。

## Requirements

- R3.1（测量前置）：建立可重复基线——用代表性真实数据库的只读副本（不修改用户库）测量：diagnostics 的 stat 次数与耗时、full scope 各 section 耗时、双并发 full 请求的行为（是否排队/级联超时）；用 stress fixture（数千 source_file 记录、25+ model）复现 stat 风暴与 N+1。基线数据写入任务 research/。
- R3.2（diagnostics 缓存）：在 **WebState 层**（web 请求边界）为 diagnostics 结果加短 TTL 缓存（建议 30–60s，实现时定）+ sync job 完成时主动失效 + 同请求内复用。`Dashboard::diagnostics()` 本身保持 cold read 语义不变，**不得**把 process cache 放进 query 层——`home_overview` 直接调用它（src/query/home_overview.rs:180），cold home overview 契约（80ms 预算、禁止 process cache）必须不受影响。TTL 存在的理由：外部文件删除不经过本进程 sync，仅靠"sync 完成失效"无法感知。(Fact D39，按评审修正)
- R3.3（full scope 组合器）：设计并实现 live 专用组合器——core + secondary 复用**同一连接**顺序执行（或 core 先行 + secondary 同连接顺序），permit 占用降为 1；**保留** behavior 四 section 的 1s 超时与逐 section 降级语义（src/web/mod.rs:41,902 的 `load_behavior_api`/`degraded_*` 行为等效保留）。禁止直接替换为 `Dashboard::snapshot()`（src/query/mod.rs:2332 是导出用、顺序、任一错误整体失败）。实施前后各测一次双并发 full 请求对比。(Fact D40，按评审修正)
- R3.4（N+1，独立条目）：`compare_model_candidates`（src/query/mod.rs:2479-2491）改为一条 `GROUP BY model` 查询，结果按 model 内存匹配；输出逐字段等价。(Fact D43)
- R3.5（超时语义）：web 读连接的 busy_timeout 从 30s 降到 1–2s（store/connection.rs:21-36 的 web 使用路径，注意不牵动 sync 写路径连接），让锁等待快速进入既有 5s 超时/degraded 流程。(Fact D42-局部)

## Acceptance Criteria

- [x] A3.1：research/ 内有基线报告（stat 次数/耗时、full scope 分解、双并发行为），含数据集规模说明。
- [x] A3.2：stress fixture 证据：TTL 窗口内第二次 dashboard 加载 stat 次数为 0；TTL 过期后外部删除的文件能被正确判 missing；sync job 完成后缓存立即失效；`/api/diagnostics` 与 dashboard 路径共享同一份缓存。
- [x] A3.3：cold 契约回归——`home_overview_under_80ms_with_seeded_10k_events` 在 debug 与 release 各连续 3 次通过（非 CI 容差），证明 query 层无 process cache。
- [x] A3.4：执行 R3.3 的测量门槛：若单连接顺序方案使 full p95 退化 >20%，记录“不实施”及双并发 residual；单 section 故障时其余 section 正常、该 section 返回 degraded 形状（回归测试）。
- [x] A3.5：compare 候选查询次数从 N+1 降为常数（测试断言查询次数或 VM-step 量级），`/api/compare` 输出与改前逐字段等价（等价测试）。
- [x] A3.6：锁占用场景下 web 读请求在 ~2s 内进入超时/degraded 路径（测试或实测证明）。
- [x] A3.7：`scripts/benchmark-dashboard-range.mjs` 对只读副本复测，interactive 改善与 full 的“不实施”决策及 residual 均有记录；`just ci` 通过。

## Notes

- 复杂任务：启动前需本 PRD + design.md + implement.md（Phase 1 补齐 implement.md）齐备。
- 改动面：`src/web/mod.rs`、`src/query/mod.rs`、`src/store/connection.rs`、测试。不改 schema、不改 sync 写路径、不改 full/core 响应形状。
