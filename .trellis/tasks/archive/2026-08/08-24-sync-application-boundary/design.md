# Design

## Target Modules

```text
src/sync/
  executor.rs       trait + BoxFuture
  engine.rs         run once/locked pipeline, rebuild/repair/remote lifecycle
  default.rs        DefaultSyncExecutor + JobRegistry::default
  job_registry.rs   admission, polling, cancellation
  types.rs          request/event/summary contracts

src/commands/sync.rs
  CLI bootstrap + reporter + summary + compatibility wrappers/re-export
```

`engine` may depend on registry/parsers/store/remote/domain/runtime typed context. It may not depend on commands, web or tui. `default` composes engine; adapters may inject test executors through the existing trait.

## Compatibility Flow

```text
commands::sync::run_once_with_cancel(...)
  -> sync::engine::run_once_with_cancel(...)

commands::sync::CommandSyncExecutor
  -> re-export/type alias of sync::DefaultSyncExecutor

JobRegistry::default()
  -> JobRegistry::new(Arc::new(DefaultSyncExecutor))
```

Public wrappers keep signatures and doc intent. No wrapper may contain parser selection, SQL, rebuild decision or remote sweep branches.

## Architecture Gate

Extend the existing `syn` visitor from a two-edge blacklist to an explicit set of forbidden dependencies/impl ownership. Fixtures cover `use`, fully-qualified path, alias, nested module and impl target. The test remains platform-neutral and does not parse generated/dependency directories.

## Validation

Before moving code, capture event sequence and summary from representative synthetic full/recent/rebuild/remote fixtures. After each move, compare outputs and persisted rows. CLI renderer tests remain attached to commands; engine tests live under sync/integration seams.

## Rollback

Land engine extraction before switching adapters; keep wrappers until all consumers move. If the move fails, revert consumer switch first, then default composition, then engine file move; no schema/data rollback is needed.

