# 同步 Job 输入与 recent_days 契约闭环

## Goal

让 CLI/Web/公开 Rust API 对同步参数使用同一套 typed validation，并兑现 `recent_days` 的 bounded import 语义。

## Confirmed Evidence

- `src/sync/job_registry.rs:175` 明示 `recent_days` 仅保存。
- `src/parsers/driver.rs:127` 在完整 parse 后才标记 `RecentReady`。
- `JobRegistry::try_start` 是公开入口但不验证 options。
- unknown source 在 `src/sync/job_registry.rs:374` 变成 `None`，随后代表所有 parser。
- ADR-0005 定义 JobRegistry 为进程内编排层，不应重新解释 transport 字符串。

## Requirements

- 定义共享 `ValidatedSyncRequest`；CLI、Web、JobRegistry public API 都必须经过同一构造/校验路径。
- unknown source、parallelism 越界、非法 recent_days 返回稳定 typed error，不得 clamp 或退化为全量。
- `recent_days` 在文件发现/读取阶段做安全裁剪；无法仅凭文件时间排除的来源必须读取并按事件时间过滤，不能漏记窗口内事件。
- `RecentReady` 表示 bounded recent stage 真正完成，不在全量解析后补发虚假里程碑。
- 文档、CLI help、API payload 与实际执行一致。

## Acceptance Criteria

- [ ] public `try_start` 对 unknown source 和非法 options 直接失败且不创建 job。
- [ ] `recent_days=N` 不导入窗口外事件，并保留窗口内事件；测试包含旧文件被追加新事件。
- [ ] instrumentation 证明 eligible source 不执行无必要的全历史 parse。
- [ ] `RecentReady` 时序与 bounded stage 一致。
- [ ] CLI/Web/Rust API 错误码与合法路径行为一致。

## Out of Scope

- 不改变 JobRegistry 的内存态生命周期，不新增持久化 job 表。

