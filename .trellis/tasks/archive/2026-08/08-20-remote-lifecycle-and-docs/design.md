# C4 技术设计

设计依据：父任务 `design.md` 第 5 节与第 9 节。本文件只记录父设计未覆盖的子任务级细节。

## 本轮联系成功的判定

`missing` 扫描与 lossy 风险排除都需要回答"这台主机在本轮是否被成功解析过"。判据不用 `host.last_contacted_at` 与墙上时钟比较，而用本轮 sync 在内存中维护的成功主机集合：

```rust
struct RemoteRunOutcome {
    contacted: BTreeSet<String>,   // host_id 成功导入且收到 trailer
    skipped:   BTreeMap<String, String>, // host_id -> 原因
}
```

理由：`last_contacted_at` 是持久化字段，与 `run_started_at` 的时间比较在同一秒内的连续 sync 上不可靠（`common/util.rs:16-26` 已为 `source_file` 状态机记录过同类问题）。内存集合没有分辨率问题。

`last_contacted_at` 仍然写入，但只用于 `source-status` 与 `remote list` 展示，不参与控制流。

## 扫描与守卫的接线

- `parsers/driver.rs` 的扫描（`parsers/driver.rs:121-128`）当前对每个 source 无条件执行。改为接收本轮 host 集合，对集合内的每台主机逐一扫描。本地主机始终在集合内。
- 该扫描已有一条既有豁免：`stats.last_error.is_some()` 时跳过，避免把不可读子树变成假 `missing`（`parsers/driver.rs:118-119`）。远端不可达属于同一类情形，处置方式保持一致。
- `lossy_rebuild_risks`（`commands/sync.rs:807-819`）增加 host 过滤参数。调用点有两处：`sync --rebuild` 的守卫与自动 token-accounting 修复的守卫（`commands/sync.rs:768`、`commands/sync.rs:789`）。两处都必须排除未联系成功的远端主机，否则 AC7b 与 AC7c 各失败一半。

## source-status 的主机状态

状态由 `host` 表字段与本轮结果共同推导，不新增持久化状态列：

| 状态 | 判据 |
|---|---|
| `never_contacted` | `host.last_contacted_at` 为 NULL |
| `unreachable` | `host.last_error` 非空 |
| `idle` | 已联系成功且 `last_error` 为空 |

`source-status` 是只读命令，走 `require_initialized()`（`write-fencing-contracts.md`），没有本轮 `RemoteRunOutcome`，因此只输出上表三态。`live`（本轮联系成功且有新事件）只出现在 sync 的 `RemoteHostFinished` / `--json-events`（AC7e），不进入 `source-status`（AC7d）。

## SyncEvent 扩展

新增三个变体，序列化 tag 与既有 `SourceStarted` / `SourceFinished` 风格一致（`parsers/mod.rs:50-130`）：

```rust
RemoteHostStarted  { host_id: String, label: String },
RemoteHostFinished { host_id: String, label: String, stats: Vec<SourceSyncStats> },
RemoteHostSkipped  { host_id: String, label: String, reason: String },
```

`SyncEvent` 是 `--json-events` 与 dashboard job 的公开契约（`source-sync-contracts.md`），新增变体属于向后兼容的追加；消费方对未知 tag 的处理需确认现有 dashboard job 前端不会因未知事件崩溃。

## 文档修正的具体位置

现有文档明确声明无远端数据通道，必须逐处修正而不是笼统追加一节：

| 位置 | 现状 | 处置 |
|---|---|---|
| `docs/index.md:21` | "No hooks, plugins, login, sync service, or remote usage API" | 保留 no login / no sync service / no remote usage API，说明 SSH 远端导入是用户自有主机之间的直连拉取 |
| `docs/safety/index.md:29` | 无上传队列、无远端用量 API 调用 | 同上，并说明传输内容只有规范化字段、方向是拉取、触发方是用户 |
| `docs/prd/llmusage-integration-prd-v1.1.md:774` | 不引入云端上传 / 远程聚合 | PRD 是历史文档，不改写；由新 ADR 记录决策演进 |
| `docs/reference/cli.md:288` | SSH 隧道访问 loopback dashboard | 保留该访问通道，并补充 SSH 也可作为数据通道 |
| `docs/dashboard/index.md`「Remote or SSH access」 | 当前示例是 `serve --public` | 保留 public 说明，并指向 CLI 页的 SSH 数据通道 |

新 ADR 编号取 `docs/adr/` 现有最大值加一（当前最大 0013），登记 `docs/adr/index.md`。ADR 需写明：与 ADR 0011 的关系、event_key 作用域变更（含同一产物在两台主机上计两条的后果）、否决的备选方案（父 design.md §8）。
