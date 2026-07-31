# 诊断：行为分析 Activity 冷启动超时（v2，修正）

> v1 诊断的两处错误已修正：(1) "逐行往返"描述错误——现行实现是单条 SQLite statement
> 的流式读取；(2) "Tools 用纯 SQL GROUP BY 所以不超时"错误——Tools 同样在 Rust 归约
> （`tool_attribution_rows`，src/query/mod.rs:1540）。v1 提出的 JOIN 聚合方案即被
> 07-28 淘汰的 `legacy_activity_breakdown`（src/query/mod.rs:1460），已否决。

## 现象

2026-07-29 截图：行为分析页 Activity 卡片 `dashboard query exceeded 3000 ms timeout`，同页 Tools 正常。d8fb8d4（07-28 任务）之后仍复现。

## 已知事实

- 现行 `activity_breakdown`（src/query/mod.rs:1370）：全量 usage_event 成本投影 → HashMap，过滤 usage_turn 流式投影 → Rust 顺序聚合。这是 07-28 实测选定的最快形态（热 0.64s 直连 SQL；对照 JOIN 聚合 2.996s）。
- 审阅复测（v18 真实库，只读，2026-07-29）：
  - 现行形态等价模拟：热 790–906 ms —— 达标。
  - v1 提议 JOIN SQL：冷 7888 ms，热 2252–2274 ms —— 更慢，方向否决。
  - 未预热 v17 备份：现行路径首轮 **5866 ms** —— 冷态越过 3s 预算。
- 契约（dashboard-performance-contracts.md §3）：1d 三样本中位数 <1s、all 每样本 <3s；序列化结果一致（SQL SUM 浮点求和顺序不同即违约）。

## 诊断结论

**冷文件缓存放大与 3 秒硬中断共同主导**。Step 1D 的配对第二次请求不能视为完整暖态：
第一次请求在 `usage_event` 全量成本投影完成前被 interrupt；第二次重复该投影后首次进入
仍冷的 `usage_turn` 全量投影，再次越过总截止。两份 `/J` 无缓冲代表副本均呈现前两次
超时、第三次恢复 normalized 的阶梯。完整证据见
[`warm-timeout-diagnosis.md`](./warm-timeout-diagnosis.md)。

次要待排除：并发二时许可等待挤占 3s 预算（07-28 干净复现中 semaphore_wait_ms=0，但需在冷态并发下复核）。

## 修复方向（递进，见 design.md）

D1 紧凑覆盖索引压缩冷读页数 → D2 bounded 请求批量成本读取 → D3 成本投影进 usage_turn（最后手段）。全程保留现行 reducer 作为序列化 oracle。

## 2026-07-29 Step 1A 结果与修订

`research/baseline.md` 的 copy-backed-cold 矩阵没有复现超时：`all` 首轮代理冷态为
1.14-1.42 秒，暖态为 1.12-1.56 秒，60 个请求均 normalized，许可等待为 0。
由于 Windows 页缓存未被驱逐，复制本身可能预热文件；因此该结果既不能确认，也不能
推翻真实冷 I/O 假设。

用户选择不越过证据门。v3 改为重启前准备五份隔离快照，重启后每份只执行一组
first-touch `all` 与配对暖态请求。只有冷样本中位数大于 3 秒、五个暖样本全部小于
3 秒且许可/锁等待不是主因时，才进入 D1。

## 2026-07-29 Step 1B 独立检查发现

现行 `load_via_dashboard_with_timeout` 在三条超时分支中 interrupt 后裸
`drop(task)`。这保留了 PERF-002 的硬响应截止和 blocking task 自持 permit，但没有
supervisor 或真实完成信号；timeout timing 在后台 closure 结束前已经写出。因此同进程
warm 可能与 first-touch orphan query 重叠，不能作为冷 I/O 因果证据。

用户选择不降级为双进程近似配对，而是先补齐产品生命周期：后台 supervisor 接管并
await JoinHandle，以 query ID 输出 settled 信号、保留硬响应截止和 permit 所有权；
harness 收到匹配 settled 后才允许 warm。该修复是取证前置门，不授权提前实施 D1/D2。

## 2026-07-30 Step 1D 回诊断

五组 first-touch 与配对第二次请求都在 3 秒超时，因此机械结论保持 `NO-GO D1/D2`。
现有 query-ID 日志证明十个 orphan 均在 interrupt 后 13-37 ms 内 settled，排除了旧任务
与配对请求重叠。只读分段显示首次完整 event 投影 4982.40 ms、首次完整 all-range turn
投影 2134.97 ms；完成后分别降到 348-653 ms 与 294-305 ms。两个独立无缓冲副本的
连续 HTTP 请求均为“前两次超时、第三次成功”，确认配对第二次仍处于不完整预热状态。

下一门槛是用户批准新的验收协议与方案设计；现有结论不解锁 D1/D2，也不授权生产改动。
