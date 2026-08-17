# Runtime Log Contracts

## Scope

Apply this contract when changing `src/runtime/logging.rs`, the `logs` command,
or diagnostics/doctor fields that describe structured runtime logging.

## Storage Bounds

- The structured NDJSON writer rotates during the lifetime of one process; a
  restart is never required to trigger a size boundary.
- Production defaults are 10 MiB per shard, 30 MiB across retained shards, at
  most seven files, and a maximum age of seven days.
- The writer serializes rotation and retention maintenance. Active shards are
  never removed, and maintenance runs at rotation plus low-frequency byte
  intervals while writes continue.
- A formatted NDJSON event is indivisible: rotate before writing the event
  rather than splitting it across shards. One event larger than 10 MiB may
  occupy an oversized shard so both files remain parseable.
- An occupied or otherwise undeletable shard is retained and retried by later
  maintenance. Other eligible shards may still be removed so one failure does
  not make growth unbounded.
- Rotation and cleanup failures increment a nonrecursive maintenance counter.
  The failure path must not emit a tracing event or panic.

## Observability

- Keep `tracing_appender`'s lossy non-blocking writer and expose its cumulative
  dropped-line counter as `LogsRuntimeStatus.dropped_event_count`.
- `LogsRuntimeStatus` also exposes current-shard bytes, retained file count,
  total retained bytes, recent error entries, and maintenance failures.
- `diagnostics --json`, `logs --json`, and their human-facing doctor/logs
  consumers must preserve these counters. Public dashboard routes must not gain
  runtime-log access.
- After each source parse, if `ParseIssues::summary_text()` is present, the
  driver emits one `info!` event with `source`, the four class counts, and
  comma-joined sample reasons. It must not include record text, prompts,
  paths, or `error_message`.
- Default `LLMUSAGE_LOG=warn` does not persist this info event. That is
  intentional: skipped and other informational issues are not faults. Capture
  the event in a unit test. `llmusage logs --level info` sees it only when
  the file filter is info or finer. Do not raise the default file level.

## Tail Reads

- Enumerate shards newest first and read file contents backward in fixed-size
  blocks until the requested scan window is satisfied.
- Return parsed entries in chronological order, retaining only the newest
  requested matches across all shards.
- Preserve complete UTF-8 lines and a partial final line. A leading partial
  line created by a reverse seek is discarded.
- Tail work may inspect bounded shard metadata, but must not read all historical
  file contents when a small tail is requested.

## Required Tests

- One writer instance crosses multiple size boundaries and remains within its
  configured count and total-byte budgets.
- Crossing a size boundary never splits one NDJSON event between two shards.
- A full deterministic non-blocking queue increments the dropped counter.
- Multi-shard tails retain chronological order and demonstrate bounded bytes
  read from a much larger shard.
- A simulated occupied-file deletion records a maintenance error, continues,
  and succeeds on a later retry.
- A unit test captures the driver parse-issue info event and asserts source,
  class counts, and sample reasons without depending on the default warn
  file level.
