# Write Fencing Contracts

## Scenario: Generation-Fenced SQLite Mutations

### 1. Scope / Trigger

- Trigger: any code that migrates schema or mutates usage, cursor, source-file,
  sync-status, run-log, trigger-worker, integration, pricing, or meta state.
- The global `worker_lock` lease is the single coordinator for bootstrap,
  sync, catalog, hook workers, and standalone Store mutation APIs.

### 2. Signatures

- `WorkerLock::fenced_store() -> Store` creates a Store clone carrying a private
  `WritePermit { lock_name, owner_id, generation, lost }`.
- `Store::write_transaction(...)` uses `BEGIN IMMEDIATE`, validates the permit,
  runs the mutation, validates again, then commits.
- `Store::require_initialized()` checks an existing schema without migration or
  pricing recomputation.
- `SyncRunWriter::commit_shard(SyncShard)` remains the only parser shard write
  protocol. Parser and CLI types never carry owner IDs or generations.

### 3. Contracts

- The first acquisition may idempotently create only the latest minimal
  `worker_lock` coordination table. Full bootstrap and every migration happen
  after acquisition under the resulting permit. Coordination-table creation
  and legacy column repair run in one `BEGIN IMMEDIATE` transaction so
  concurrent first acquisitions cannot race duplicate `ALTER TABLE` calls.
- A permit is valid only while the row matches lock name, owner ID, generation,
  and an unexpired lease. Its fields are private and callers cannot reconstruct
  it from `WorkerLockMeta`.
- Heartbeat `LockLost` atomically marks every permit clone lost and terminates
  the heartbeat thread. An expired lease cannot be refreshed back to life; the
  next heartbeat or mutation returns `LlmusageError::LockLost`.
- Sync CLI and JobRegistry acquire first, derive a fenced Store, bootstrap, and
  pass that Store through run-log, parser driver, shard writer, and status writes.
- Catalog apply/reset/snapshot and standalone Store mutation APIs own one
  operation guard or reuse an existing fenced Store.
- Hook `trigger_state` signal upsert is the sole control-plane exception: it may
  write while another worker owns the data permit so catch-up signals are not
  dropped. Hook worker start/finish, sync, cursor, and run-log writes are fenced.
- Read-only report/status/doctor/TUI/catalog-status commands call
  `require_initialized()` and must not run migration or pricing recomputation.

### 4. Validation & Error Matrix

- Owner or generation mismatch -> `LlmusageError::LockLost`, transaction rolls back.
- Missing lock row or expired lease -> `LlmusageError::LockLost`, transaction rolls back.
- Heartbeat observes stolen generation -> mark lost, stop refreshing, next write fails.
- Heartbeat observes its own expired lease -> `LockLost`; never extend the expired row.
- Non-expired holder blocks acquisition past timeout -> `LlmusageError::LockBusy`.
- SQLite `BUSY` / `LOCKED` during coordination is a missed acquisition attempt:
  blocking callers retry until timeout, while non-blocking hook callers skip.
- Missing or stale schema on read-only entry -> `LlmusageError::NotInitialized`.
- A schema newer than the binary -> `LlmusageError::SchemaTooNew`.
- Fresh database acquisition -> create only coordination schema, acquire, then migrate.

### 5. Good/Base/Bad Cases

- Good: A acquires, pauses, B steals after expiry, then A's next shard commit
  fails before inserting any event.
- Base: A heartbeat renews normally; every transaction validates and commits.
- Good: a hook signal is recorded while a sync worker is busy; the signal is
  visible to the current or next hook worker without bypassing usage fencing.
- Bad: call `bootstrap()` before acquiring the sync operation lock.
- Bad: pass the original unfenced Store to a JobRegistry executor after lock acquisition.
- Bad: add another control-plane exception for cursor, status, run-log, or catalog data.

### 6. Tests Required

- Deterministic steal test: expire A, acquire B, assert A refresh and mutation
  return `LockLost` while B refresh succeeds.
- Expired-refresh test: expire A without a thief, assert refresh returns
  `LockLost` and leaves the persisted expiry unchanged.
- Coordination-upgrade test: concurrent acquisition against a legacy
  `worker_lock` table completes without duplicate-column errors.
- Stale writer test: construct A writer, steal with B, assert `commit_shard`
  fails and the event count remains zero.
- JobRegistry test: executor asserts the received Store carries a valid permit.
- Bootstrap/migration tests: fresh and legacy databases reach latest schema;
  isolated migration failure rolls back.
- Read-only initialization test: missing DB returns `NotInitialized` and no DB
  or `meta` table is created.
- Run `python scripts/ci-rust.py`, then `just ci`.

### 7. Wrong vs Correct

#### Wrong

```rust
store.bootstrap()?;
let lock = store.acquire_worker_lock_with(timeout, HolderKind::Cli)?;
run_once_locked(&store, ...).await?;
```

#### Correct

```rust
let lock = store.acquire_worker_lock_with(timeout, HolderKind::Cli)?;
let fenced_store = lock.fenced_store();
let heartbeat = lock.start_default_heartbeat();
fenced_store.bootstrap()?;
run_once_locked(&fenced_store, ...).await?;
drop(heartbeat);
drop(lock);
```
