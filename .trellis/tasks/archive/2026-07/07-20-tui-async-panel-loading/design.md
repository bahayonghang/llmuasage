# TUI 异步面板加载设计

## 请求与执行

新增 loader，Request = panel + cloned filter + time window + generation。UI 每次首次加载/失效/筛选/窗口变化递增 generation，设置 loading 元数据后立即返回事件循环。后台任务通过当前 Tokio handle `spawn_blocking`，每个工作单元打开新 `Dashboard`；并发由容量与 semaphore 同时限制。

结果走有界 std mpsc，由 Tick/事件处理前 `try_recv` 排水。Result 只有 generation、请求快照与类型化 panel payload 全部匹配时才能写入。新请求主动调用旧查询的 SQLite interrupt handle；退出只 interrupt，不等待阻塞线程。

## loading 与并行

冷加载保持 payload `None`，使现有 loading 分支成为可达首帧；刷新保留旧 payload并设置 `refreshing`，形成 stale-while-refresh。Stats 与 Behavior 拆成独立连接的并行子查询，汇合成原 payload；source breakdown 的 per-source last-event 查询保持不变。

## 兼容

Panel 缓存仍惰性加载，切走再切回不会重查。错误仍落入对应 `Result::Err`。不改 SQL、payload 或 web helper 行为。
