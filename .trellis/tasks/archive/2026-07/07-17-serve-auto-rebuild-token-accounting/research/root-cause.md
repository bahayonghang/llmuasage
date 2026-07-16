# Root Cause: Serve 缺少 token accounting 升级协调

## Reproduction Baseline

Focused contract command:

```bash
cargo test --test token_accounting_parity legacy_source_requires_guarded_explicit_rebuild_before_new_writes -- --exact --nocapture
```

结果：测试在 0.05s 内通过，确认 fixture 中“有历史行、无版本 2 标记”的源会拒绝普通 sync，且显式逐源 rebuild 后恢复写入。

## Findings

1. `0848fe8` 在 2026-07-16 引入 token accounting v2、legacy 检测和普通写入 guard，但没有修改 `src/commands/serve.rs`。
2. `serve` 当前只 bootstrap SQLite、绑定 Web 端口并打开浏览器；它没有调用任何 legacy repair。
3. marker 缺失不是 schema 损坏。设计明确规定：存在历史行但标记不是当前版本即 legacy；只有 parser/store sync 成功后才能写当前 marker。
4. Web JobRegistry 复用 `commands::sync::run_once_with_cancel`，因此没有绕过普通写入 guard。
5. 用户当前 `codex`、`claude`、`opencode` 都是 legacy，三者 lossy 风险均为 false。
6. 用户当前 `antigravity` 有 5 个 missing files、43 条 protected events，但它没有 parser capability，不是 token accounting repair 目标。
7. 无 source 的全源 rebuild 当前先用 parser registry 做 lossy 预检，随后调用无条件清空所有 usage 表的 `reset_usage_data()`，最后只运行 parser registry。因此 Antigravity 历史会被删除却不会被预检或重建。
8. `Store::reset_usage_data()` 的注释写明只删除可由 Codex/Claude/OpenCode 重建的数据，但 SQL 实际无 source 过滤，代码与契约不一致。

## Root Cause

主故障是升级编排缺口：数据口径变更正确地禁止 legacy/current 混写，但发布只提供了手动逐源修复命令，没有给常用的 `serve` 启动路径增加安全迁移协调器。

诊断同时发现 rebuild 删除范围漂移：无 source 的命令以 parser registry 定义可重建集合，却调用了删除所有来源的 Store 方法。该缺口会让 parserless 历史绕过 lossy guard 后丢失，必须与启动迁移一起修复。

## Constraints

- 不自动设置 `allow_lossy_rebuild=true`。
- 不把 parserless source 纳入 accounting marker 或 repair。
- 不把 token accounting repair 塞进 schema migration；它依赖外部源文件和 parser，不是纯 SQLite 事务迁移。
- 每个源独立 repair，允许无风险源先恢复；风险源保留旧历史和写入 guard。
- full rebuild 的预检、reset 和 parser fan-out 必须共享 parser registry；parserless 数据永不由该命令删除。
