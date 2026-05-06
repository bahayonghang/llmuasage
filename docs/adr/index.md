# Architecture Decision Records

Local design decisions that shape how llmusage is structured. Each ADR captures one decision: the context, the choice, the alternatives that were rejected, and a deletion-test argument for why the new shape is deeper than the previous one.

ADRs are append-only. To revise a past decision, write a new ADR that supersedes it; do not edit the old one beyond a `Superseded-by` header.

## Index

- [0001 — `SourceParser` trait + `sources::registered_*` registry](./0001-source-registry-and-parser-trait)
- [0002 — `SyncShard` as commit protocol](./0002-sync-shard-as-commit-protocol)
- [0003 — `Store` façade with 5 borrowed views](./0003-store-facade-vs-substores)

## Companion docs

- [`CONTEXT.md`](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md) — domain glossary at the repo root. Every ADR references its terms back into `CONTEXT.md`.
- [Architecture overview](../architecture/) — runtime layout and data flow.
