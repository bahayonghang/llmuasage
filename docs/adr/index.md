# Architecture Decision Records

Local design decisions that shape how llmusage is structured. Each ADR captures one decision: context, chosen design, rejected alternatives, consequences, and verification.

ADRs are engineering records, not end-user tutorials. Start with [Architecture](../architecture/) for the current system map, then open ADRs when you need the design rationale.

## Index

- [0001 — `SourceParser` trait + `registry::registered_*` registry](./0001-source-registry-and-parser-trait)
- [0002 — `SyncShard` as commit protocol](./0002-sync-shard-as-commit-protocol)
- [0003 — `Store` façade with borrowed views](./0003-store-facade-vs-substores)
- [0004 — Schema version migration runner](./0004-schema-version-migration-runner)
- [0005 — In-memory job registry](./0005-job-registry-in-memory)
- [0006 — Source file state machine](./0006-source-file-state-machine)
- [0007 — Public `LlmusageError` surface](./0007-llmusage-error-surface)

## Companion docs

- [`CONTEXT.md`](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md) — repo-level domain glossary.
- [Architecture overview](../architecture/) — current runtime, sync, query, dashboard, and migration map.
- [PRD archive](../prd/) — historical plans and audits.
