# TUI sync 后台化设计

## 拓扑

`run_dashboard` 捕获当前 Tokio `Handle`，事件循环持有一个 TUI 专用 `JobRegistry`、可选 active job id 和事件接收器。`x` 调用 `try_start`；registry 在现有 runtime 上 spawn，同步事件通过容量 128 的 receiver 到达 UI，快照作为可轮询权威状态。

## 状态与文案

- Idle -> Running：创建 job，footer 显示结构化 `SyncEvent` 的阶段/来源/计数。
- Running + `x`：请求取消，进入 Cancelling；不创建第二个 job。
- Completed：沿用 `Sync complete: {inserted} inserted, {stored} stored`，失效面板缓存并刷新当前面板。
- Failed/Cancelled：显示一等终态；worker lock 冲突按失败文案显示，不 panic。

进度格式化为纯函数，避免 footer 解释 wire payload。退出时对 active job 调用 cancel，并最多轮询 500ms；超时直接退出。writer 分片事务保证已提交分片有效，未提交分片回滚，进程内 registry 不承诺任务跨进程存续。

## 兼容与回滚

不改 `SyncOptions`、`SyncEvent`、JobRegistry 或 web 路径。删除 TUI 内 nested runtime 和 `block_on`。回滚点为单个子任务提交。
