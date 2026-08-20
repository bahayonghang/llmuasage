# ADR 0014 — SSH remote-host import

- Status: Accepted
- Date: 2026-08-20
- Related: ADR 0011 (passive-only synchronization); ADR 0002 (`SyncShard` commit protocol); ADR 0006 (`source_file` state machine)
- Related code: `src/remote/`, `src/commands/remote.rs`, `src/commands/sync.rs`, `src/store/host.rs`, `src/store/migrations.rs`
- Related terms: Host, SyncShard, SourceParser, event_key, WorkerLock

## Context

Users run the same coding CLIs on more than one machine. Each machine already
has local artifacts that llmusage can parse. There was no supported way to
bring those remote rows into one local database without copying session files
or mounting a remote filesystem.

ADR 0011 made passive parsing the only usage-import mechanism and removed
hooks, plugins, and any cloud upload path. Historical docs still said there
was no remote data channel. That statement was true for cloud APIs. It was
silent on a user-owned SSH pull between machines the user already controls.

_Contradicts ADR 0011 only in the narrow reading that `sync` walks parsers on
this machine alone. This ADR does not reopen hooks, plugins, or a remote
usage API. Passive parsing remains the only parse mechanism. The change is
the parse location: each host parses its own artifacts, then the local
`sync` imports normalized shards._

The historical product PRD (`docs/prd/llmusage-integration-prd-v1.1.md`)
still records the original “no cloud upload / no remote aggregation”
decision. That file is a historical document and is not rewritten here.

## Decision

Add a host dimension and an SSH pull of normalized `SyncShard` records.

- Register a remote with `llmusage remote add <label> <ssh-target>`. The
  handshake requires an equal shard protocol version. Schema version is
  diagnostic only.
- On the remote host, `llmusage sync --emit-shards` parses local artifacts
  with the registered parsers and writes NDJSON shards to stdout. It does
  not open the user database and does not take the user-database worker lock.
- On the local host, `llmusage sync` runs the local driver, then imports
  each `transport='ssh'` host through `RemoteImporter` and
  `SyncRunWriter::commit_shard` on a fenced Store. `llmusage remote sync
  [--host <label>]` uses the same importer and does not run the local
  driver.
- `event_key` (and related turn/tool keys) is scoped by host and source.
  The same artifact imported from two registered hosts is two events.
  Schema v23 prefixes existing rows with `local:`.
- Cost is computed from the local pricing catalog at `commit_shard`. Remote
  shards do not carry cost.
- One unreachable remote records `last_error`, emits
  `SyncEvent::RemoteHostSkipped`, and does not fail the process if local
  sync succeeded. Missing sweep and lossy-rebuild guards use an in-memory
  `RemoteRunOutcome.contacted` set for this run. They do not compare
  `last_contacted_at` to wall clock.
- `source-status` reports three read-only host states from persisted
  fields: `never_contacted`, `unreachable`, and `idle`. `live` appears only
  in sync events / `--json-events`.

SSH is the system `ssh` binary. The process inherits the user's ssh config,
ProxyJump, agent, and known_hosts.

## Consequences

- Parsing still happens only on the machine that owns the artifacts. The
  local machine does not parse remote files, mount sshfs, or copy prompt
  text. Shard `raw_records` are skipped on the wire.
- Users must install a compatible llmusage binary on each remote host and
  keep shard protocol versions equal.
- A missing remote file path is not a local-disk path. Uncontacted SSH
  hosts must not flip `source_file` rows to `missing` and must not block
  `sync --rebuild` or automatic token-accounting repair.
- Dashboard and CLI reports gain a host dimension (`hosts` payload,
  `--host <label>`). Source grouping is unchanged.

## Rejected alternatives

- **sshfs plus environment-variable overrides:** Claude has no environment
  override for its project root. Project attribution depends on a local
  work tree. SQLite over sshfs is not reliable.
- **Pull raw session files and parse them locally:** Project attribution is
  still wrong, and prompt text would be copied onto the local disk.
- **One database per host plus `ATTACH` at read time:** The read-layer
  change is as large as a host column, and schema/pricing versions can
  drift across files. Database files may also contain a raw archive.
- **Prefix `event_key` only for remote rows:** Lower migration risk, but
  host-scoped identity would then differ between local and remote rows.
- **Embed an SSH library such as `russh`:** The crate would have to own
  host-key checks, keys, and agent handling. The system `ssh` client
  already matches the documented SSH tunnel for the dashboard.

## Verification

- Injected `ShardSource` tests cover handshake, protocol mismatch, motd
  skip, trailer-missing watermark rules, and unreachable hosts.
- `llmusage sync` against an unreachable host completes local sync, exits
  successfully, emits `RemoteHostSkipped`, and leaves that host's
  `source_file` rows live.
- `sync --rebuild` and automatic token-accounting repair ignore missing
  rows on uncontacted SSH hosts.
- `source-status` returns `idle` / `unreachable` / `never_contacted` and
  does not print `live`.
- `remote sync --host <label>` imports only that host and does not run the
  local driver.
