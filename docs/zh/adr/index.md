# ADR 中文入口

ADR 目前以英文工程记录为主，并复用仓库根目录 `CONTEXT.md` 中的领域术语。中文文档侧边栏保留本入口，确保中文读者可以到达所有设计决策。

如果你只是想了解当前系统结构，先读 [架构说明](../architecture/)；需要追溯为什么这样设计时再读 ADR。

## ADR 列表

- [0001 — SourceParser trait + registry](../../adr/0001-source-registry-and-parser-trait)
- [0002 — SyncShard as commit protocol](../../adr/0002-sync-shard-as-commit-protocol)
- [0003 — Store façade with borrowed views](../../adr/0003-store-facade-vs-substores)
- [0004 — Schema version migration runner](../../adr/0004-schema-version-migration-runner)
- [0005 — In-memory JobRegistry](../../adr/0005-job-registry-in-memory)
- [0006 — Source file state machine](../../adr/0006-source-file-state-machine)
- [0007 — Public LlmusageError surface](../../adr/0007-llmusage-error-surface)
- [0008 — Source Capability Registry and passive-reader gate](../../adr/0008-source-capability-registry)
- [0009 — Antigravity source cutover](../../adr/0009-antigravity-source-cutover)
