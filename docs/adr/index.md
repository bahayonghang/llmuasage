# Architecture Decision Records

Local design decisions that shape how llmusage is structured. Each ADR captures one decision: the context, the choice, the alternatives that were rejected, and a deletion-test argument for why the new shape is deeper than the previous one.

ADRs are append-only. To revise a past decision, write a new ADR that supersedes it; do not edit the old one beyond a `Superseded-by` header.

## Index

- [0001 — `SourceParser` trait + `registry::registered_*` registry](./0001-source-registry-and-parser-trait)
- [0002 — `SyncShard` as commit protocol](./0002-sync-shard-as-commit-protocol)
- [0003 — `Store` façade with borrowed views](./0003-store-facade-vs-substores)
- [0004 — Schema version migration runner](./0004-schema-version-migration-runner)
- [0005 — In-memory job registry](./0005-job-registry-in-memory)
- [0006 — Source file state machine](./0006-source-file-state-machine)
- [0007 — Public `LlmusageError` surface](./0007-llmusage-error-surface)

## Companion docs

- [`CONTEXT.md`](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md) — domain glossary at the repo root. Every ADR references its terms back into `CONTEXT.md`.
- [Architecture overview](../architecture/) — runtime layout and data flow.
