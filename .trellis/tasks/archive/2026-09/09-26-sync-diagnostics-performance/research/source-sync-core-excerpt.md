# Source Sync contract excerpt

Authoritative source: `.trellis/spec/llmusage/backend/source-sync-contracts.md`.
SHA-256 at implementation refresh: `80f4d0a2245c000044ed5773cb2993dae946ad6acda12fc8cf7acd24aeb62eff`.
Quoted source lines follow. This is a research snapshot; current source takes precedence if the source changes. Read omitted sections when the implementation scope needs them.

## Source lines 1-360

L1: # llmusage Source Sync Contracts
L2: 
L3: ## Scenario: Parser Stats And Monitor-Only Platforms
L4: 
L5: ### 1. Scope / Trigger
L6: 
L7: - Trigger: changes to `SourceKind`, parser `SourceSyncStats`, `source-status`,
L8:   `sync` summaries, `Dashboard::sync_command_center`, or TUI source/sync panels.
L9: - Source parser changes are cross-layer: parser output flows through sync driver
L10:   status, SQLite-derived query payloads, CLI summaries, docs, and TUI rendering.
L11: - Platform monitoring is not parsing. A platform can be detected and shown as
L12:   monitor-only without adding a stable `SourceKind` or importing token rows.
L13: - Usage import is passive-only. Source descriptors do not carry hook/plugin
L14:   activation or integration capabilities, and `init` never installs them.
L15: 
L16: ### 2. Signatures
L17: 
L18: - Parser runtime stats: `SourceSyncStats { files_processed, changed_files,
L19:   skipped_files, events_emitted, stored_events }`.
L20: - Query/TUI payload: `SyncSourcePayload { files_processed, changed_files,
L21:   skipped_files, stored_events, malformed_lines, oversized_lines,
L22:   skipped_lines, accounting_anomaly_lines, ... }`. Counters only; parse-issue
L23:   samples never enter interactive dashboard JSON.
L24: - Store status rows persist existing source status columns. Do not add a schema
L25:   migration for derived skipped counts unless a consumer needs historical
L26:   skipped totals independent of the latest source sync status.
L27: - Schema v15 adds `source_cursor.last_part_rowid` and
L28:   `(source, source_path_hash)` indexes on `usage_turn` and `usage_tool_call`.
L29:   OpenCode owns the part cursor; file-backed sources continue using `FileCursor`.
L30: - Schema v22 adds `source_cursor.last_skipped_at` and
L31:   `source_cursor.last_skipped_ids_json`. ZCode owns the skip watermark;
L32:   file-backed sources and OpenCode do not read or write these columns. Do not
L33:   reuse `last_processed_ids_json` or `last_total_json` for skip diagnostics.
L34: - Stable passive parser ids include `kimi_code` for
L35:   `~/.kimi-code/sessions/**/wire.jsonl`, `pi` for `~/.pi/agent/sessions` (or
L36:   comma-separated `PI_AGENT_DIR`), and `omp` for `~/.omp/agent/sessions`.
L37:   Overlapping canonical paths belong to `pi`. Grok Build uses `grok`
L38:   for direct sidecars under `~/.grok/sessions/*/*/` or `GROK_HOME/sessions`.
L39: - Registered passive parsers are Codex, Claude, OpenCode, Antigravity CLI/IDE, Kimi
L40:   Code, Pi, Oh My Pi, Grok Build, ZCode, and DeepSeek Harness.
L41: - Monitor descriptors live outside parser promotion and report detection status,
L42:   candidate roots, and parser availability.
L43: 
L44: ### 3. Contracts
L45: 
L46: - `skipped_files` means known source artifacts that were seen but not reparsed
L47:   because their fingerprint/cursor state did not require importing events.
L48: - `files_processed` counts source artifacts considered by the parser for that
L49:   run, not rows committed to `usage_event`.
L50: - `changed_files` counts artifacts that produced new or refreshed parser work.
L51: - Public `write_ms` retains its existing elapsed-time boundary. Ordinary shards
L52:   start the timer after host-prefix preparation and behavior deduplication,
L53:   and stop after commit. Antigravity product `write_ms` covers apply plus
L54:   marker writes; shared group reset, BEGIN, and commit are outside those
L55:   per-product timers. Private profiling reports complete transaction time
L56:   separately. `parse_ms` is a residual elapsed value, not parser CPU time;
L57:   `scanned_bytes` is reader-specific logical input accounting, not disk I/O.
L58: - Claude logical dedupe is scoped to the first directory below
L59:   `~/.claude/projects`. If any file in a project changes, replay every current
L60:   JSONL in that project, but do not replay other projects or reset missing
L61:   historical paths. Projects parse independently; outputs from one bounded
L62:   parallel batch may share one atomic `SyncShard` commit.
L63: - Codex remains file-cursor incremental: unchanged files are metadata-only and
L64:   append work reads only bytes after the stored offset.
L65: - Kimi Code imports only explicit `type=usage.record` plus
L66:   `usageScope=turn` rows. It preserves the raw model id and maps non-cached
L67:   input, cache read, cache creation, and output as separate channels.
L68: - Pi and Oh My Pi share one parse implementation and register two sources.
L69:   `pi` lists `PI_AGENT_DIR` or `~/.pi/agent/sessions`. `omp` lists
L70:   `~/.omp/agent/sessions` and skips a candidate when its canonical path
L71:   overlaps a Pi root (equal / ancestor / descendant). Pi wins; unconflicted
L72:   `.omp` files remain. Skip notes stay on the listing for diagnostics and
L73:   must not set `SourceSyncStats.last_error`, so the missing-file sweep still
L74:   runs. Both sources use the ordinary append/reparse `FileCursor` state
L75:   machine. Assistant usage keeps the upstream total authoritative and
L76:   reasoning diagnostic-only. `event_key` is `{source}:{hash}`.
L77: - Grok Build discovery uses exactly two `read_dir` levels for
L78:   `sessions/<workspace>/<session>` and only joins the whitelisted direct
L79:   sidecars `updates.jsonl`, `signals.json`, `summary.json`, and optional
L80:   `events.jsonl`; it never uses recursive `WalkDir` discovery. Any sidecar
L81:   change reparses the complete session and resets one shared session path hash.
L82:   A tracked missing sidecar preserves prior events and lets the ordinary
L83:   source-file sweep plus lossy-rebuild guard own recovery until it returns.
L84:   The primary token path is each `sessionUpdate == "turn_completed"` record
L85:   with a usable `params.update.usage` object (one event per record). Sessions
L86:   with no usage keep the cumulative `_meta.totalTokens` plus signals
L87:   reconciliation fallback. Do not mix the two paths in one session.
L88: - OpenCode database replacement detection uses persisted message anchors
L89:   `(last_time_created, last_processed_ids)`. Preserve all cursors when every
L90:   anchor exists; if any anchor disappeared, reset message and part cursors.
L91:   File size, mtime, head signatures, and Windows creation time are not database
L92:   generation identities.
L93: - OpenCode tool parts use a persisted `last_part_rowid`. Read pages only inside
L94:   `(last_part_rowid, MAX(rowid)]`, advance after the closed range completes,
L95:   and leave the cursor unchanged on cancellation/failure. A missing `part`
L96:   table degrades to no tool rows.
L97: - Writer reset paths must be set-oriented where cardinality amplifies work:
L98:   behavior deletes use a temporary path-key table, and reset bucket pricing is
L99:   recomputed with one source-range event scan joined to a temporary bucket-key
L100:   table. Never issue one source-range event scan per touched bucket.
L101: - An event reset with one distinct path keeps the original SQL plan and
L102:   statement preparation. Repeated entries of that path do not enable adaptive
L103:   selection. Skip selection counts and unused path-index statements.
L104:   A reset with multiple distinct paths counts host/source candidates once
L105:   per batch. Each path probe uses the existing covering source/path index and stops at
L106:   the remaining host/source row count. Force the path index only when that
L107:   probe finds fewer candidates; otherwise use the default SQL plan. Subtract
L108:   actual deleted rows after each path. A zero host count skips the probe but
L109:   retains the aggregate/delete protocol. All selection work stays in the
L110:   fenced transaction and inside WRITE; no durable index or cache is added.
L111: - `stored_events` is the committed event count after store dedupe and reset
L112:   behavior; it can be lower than parser-emitted raw events.
L113: - Sync request validation has one owner: `ValidatedSyncRequest`. CLI, Web, and
L114:   public `JobRegistry::try_start` must return `unknown_source`,
L115:   `invalid_recent_days`, or `invalid_parallelism` before creating work.
L116: - Sync execution has one application owner: `src/sync/engine.rs` selects
L117:   parsers and owns rebuild/repair, remote import/sweep, status persistence,
L118:   and `SyncSummary` assembly. `src/sync/default.rs` owns
L119:   `DefaultSyncExecutor` and `JobRegistry::default()` composition.
L120: - `commands::sync` owns transport-only behavior: CLI bootstrap/lease timing,
L121:   Ctrl-C, human/NDJSON rendering, summary formatting, emit-shards, and public
L122:   compatibility delegates. The historical `CommandSyncExecutor` name is a
L123:   re-export of `sync::DefaultSyncExecutor`; it must not regain an impl body.
L124: - Web, TUI, and other non-command consumers construct `JobRegistry::default()`
L125:   or inject a `SyncExecutor` from `crate::sync`; they never depend on
L126:   `crate::commands::sync`.
L127: - A `recent_days` run uses one UTC cutoff. File-backed sources must filter by
L128:   normalized event time when metadata cannot safely exclude a file. OpenCode
L129:   must apply the cutoff in its SQLite page queries. Bounded runs may reuse an
L130:   existing full-history cursor as a lower bound, but must not advance that
L131:   cursor or execute whole-file resets; a later full sync must still recover
L132:   window-excluded history.
L133: - Monitor-only platforms must surface as diagnostics/status entries with token
L134:   quality labels, not as parser-backed usage, until sanitized fixtures and token
L135:   semantics exist.
L136: - A persisted source descriptor without a parser must surface as
L137:   `historical_only`, never `passive_ready` or `passive_no_data`. Historical
L138:   events remain queryable and dashboard filters remain valid, but sync writes
L139:   no new events.
L140: - Antigravity CLI (`antigravity`) and IDE (`antigravity_ide`) share a native
L141:   SQLite decoder. Discover only `.gemini/antigravity-cli/conversations/*.db`
L142:   and `.gemini/antigravity-ide/conversations/*.db`, under `GEMINI_CLI_HOME`
L143:   when set. `.pb`, old/backup roots, credentials, RPC and transcript bodies
L144:   are outside this reader. Verified trajectory source metadata determines
L145:   product ownership (17 CLI, 1 IDE), even when a DB is copied across roots.
L146:   Resolve identities across both roots before applying source filters.
L147: - Native generation usage can aggregate all `retry_infos`: prefer usable
L148:   attempt records and deduplicate generation/step mirrors. Do not sum the
L149:   aggregate with attempts or merge unrelated requests by token amounts/time.
L150:   Only typed generation/step timestamps are request time; file mtime and
L151:   ChatStartMetadata context-window bytes are never time fallbacks.
L152: - Antigravity replays one group per product. Read SQLite in read-only
L153:   transactions, observe DB and WAL changes, and stage complete group output
L154:   before a fenced commit. Missing/unreadable/malformed members or cancellation
L155:   preserve prior group events and cursors. Cross-product ownership changes
L156:   require an unbounded run selecting both products, committed together.
L157:   Bounded imports never reset historical groups or advance full cursors.
L158: - Antigravity checks tracked-input coverage after both-root discovery and
L159:   bounded fingerprinting. If every selected product is blocked, return before
L160:   opening usage tables or decoding observations. Discovery and fingerprint
L161:   costs remain; do not claim zero I/O. Partial blocking retains cross-root
L162:   decoding and native ownership/identity resolution. A failure limited to an
L163:   unselected historical product is not sufficient to block selected products.
L164:   An inaccessible root with unknown contents may contain either product and
L165:   blocks both groups conservatively.
L166: - `source_files::list_matching_files` retains initial root metadata errors
L167:   in `SourceFileListing.errors`. Only `NotFound` produces an empty successful
L168:   listing; other errors and non-directory roots remain failures. A successful
L169:   post-discovery metadata probe cannot erase a discovery-time failure. New
L170:   root-error summaries do not include the private root path.
L171: - A tracked local path absent from discovery is not necessarily missing.
L172:   Distinguish physical absence, existing input outside current discovery, and
L173:   metadata/access failure. Existing legacy JSON and changed-root paths retain
L174:   their history. Do not probe remote paths on the local filesystem. Failed
L175:   coverage checks may persist source diagnostics with counts and observation
L176:   time, but do not advance usage cursors or successful source-file observations.
L177:   `--allow-lossy-rebuild` does not waive discovery or access failures.
L178: - Antigravity explicit rebuild skips the engine's pre-reset. One writer
L179:   transaction replaces selected host/source attributed parser rows, cursors
L180:   and accounting markers. Empty/NULL-path hook rows remain queryable and
L181:   counted, with a retained-history warning. Missing-file loss requires
L182:   `--allow-lossy-rebuild`; unreadable input never becomes an accepted empty
L183:   snapshot. Empty recognized usage tables form an empty session; a database
L184:   with neither `gen_metadata` nor `steps` fails. Steps
L185:   alone remain usable when `gen_metadata` is absent.
L186: - ZCode reads `~/.zcode/cli/db/db.sqlite` `model_usage` completed rows with a
L187:   `completed_at` high-water cursor. Unfinished `error`/`cancelled` rows are
L188:   counted as `skipped` against both that completed watermark and a separate
L189:   skip watermark (`last_skipped_at` + `last_skipped_ids`). A row is reported
L190:   only when it is new relative to both watermarks. A full uncancelled run
L191:   advances the skip watermark to this batch's newest unfinished
L192:   `completed_at` and the ids at that timestamp. A bounded run may reuse both
L193:   cursors as lower bounds but must not advance either. Cancellation must not
L194:   persist the skip watermark, including after a completed page save. Missing
L195:   completed anchors reset both watermarks. Unfinished rows are never imported
L196:   as `UsageEvent`.
L197: - DeepSeek Harness discovers `$DSH_HOME` (default `~/.dsh`) `sessions/` at any
L198:   depth for files named exactly `session.jsonl` or `session.jsonl.zstd`.
L199:   Compression is dispatched by zstd frame magic. An unbounded fingerprint
L200:   change replays the session family; a bounded run must not reset or advance
L201:   cursors.
L202: - `sync --rebuild --source <source>` must reject a persisted source without a
L203:   registered passive parser even when `--allow-lossy-rebuild` is present. It
L204:   must never delete historical-only events that no parser can reconstruct.
L205: - Sync emits `BootstrapStarted` before lock acquisition for immediate feedback,
L206:   then emits `LockWaiting` / `LockAcquired`; migration and pricing progress run
L207:   only after acquisition on the fenced Store. Existing migration events keep
L208:   their names and meaning; embedded pricing upgrades add
L209:   `pricing_upgrade_started`, `pricing_upgrade_progress`,
L210:   `pricing_bucket_reconcile_started`, and `pricing_upgrade_finished` before
L211:   parser source events.
L212: - Safe legacy accounting repair adds
L213:   `token_accounting_repair_started` before targeted resets and
L214:   `token_accounting_repair_finished` only after writer, marker, and source
L215:   status success. These are additive lifecycle events shared by human stderr,
L216:   NDJSON, TUI, and Web jobs; failure/cancellation remains terminal through the
L217:   existing events.
L218: - After the local parser driver, `llmusage sync` serially imports each
L219:   `transport='ssh'` host through `RemoteImporter` and `commit_shard` on the
L220:   same fenced Store. One host failure records `last_error`, emits
L221:   `RemoteHostSkipped`, and continues. Process exit stays success when local
L222:   sync succeeded. `remote sync [--host <label>]` uses the same importer and
L223:   does not run the local driver.
L224: - Remote shard Header (`SHARD_PROTOCOL_VERSION` 2) carries
L225:   `source_accounting_versions` for every source in that stream. `sync
L226:   --emit-shards` lists the registry parsers selected for the run. The
L227:   decoder owns wire/structure version. The importer validates token
L228:   accounting after Header and before the first related shard commit. Same
L229:   wire protocol does not mean the same token semantics. Old protocol
L230:   streams fail with an explicit mismatch; do not guess accounting from
L231:   `llmusage_version` or schema version. Schema version stays diagnostic.
L232: - Missing or unequal per-source accounting versions refuse before the first
L233:   related shard commit. Do not change existing events or watermarks for that
L234:   host/source. The error names the source and the upgrade action. A shard
L235:   whose source is absent from the Header is refused before that commit.
L236: - Persist certified remote history in the existing `meta` table as
L237:   `token_accounting_version.<host_id>.<source>`. Do not add a schema and do
L238:   not use the local global marker as remote truth. `source-status` for a
L239:   remote host uses that host/source evidence. No evidence stays `unknown`,
L240:   not local-marker `current`.
L241: - If a remote source already has rows and the historical marker is unknown
L242:   or incompatible, refuse a current-version incremental mix-in. Tell the
L243:   caller a full restore is required (not implemented here). Keep old rows
L244:   and watermarks. Do not auto-backfill historical markers.
L245: - Establish a host/source marker only when the source was empty (Pi rows
L246:   that the first remote Omp shard will reset count as empty), the request
L247:   has no `since`, the trailer succeeds, and that source has no parse error
L248:   (`ParseIssues::total()` and `last_error`). Matching existing markers may
L249:   increment. Mid-stream failure does not advance marker or watermark.
L250: - Additive sync events `remote_host_started`, `remote_host_finished`, and
L251:   `remote_host_skipped` are public `--json-events` / job tags. Dashboard job
L252:   UI must tolerate unknown event tags.
L253: - Missing sweep runs only for hosts in this run's in-memory `contacted` set,
L254:   plus local always. Do not compare `host.last_contacted_at` to wall clock.
L255:   `stats.last_error.is_some()` still skips the sweep for that source.
L256: - `source-status` is read-only (`require_initialized()`). Host lifecycle is
L257:   derived from persisted fields only: `never_contacted` when
L258:   `last_contacted_at` is NULL, `unreachable` when `last_error` is non-empty,
L259:   otherwise `idle`. `live` appears only in sync events, never in
L260:   `source-status`. `source_sync_status` rows are keyed by `(host_id, source)`.
L261: - Pricing started/progress events carry source/target catalog versions and
L262:   processed/total event counts. Reconcile/finished events carry bucket counts;
L263:   finished also carries deleted orphan count and elapsed milliseconds.
L264: - Human stderr and `sync --json-events` consume one bootstrap-to-sync mapping.
L265:   Human output may replace a TTY line but must end lines at reconcile/finished
L266:   boundaries. JSON mode keeps stdout NDJSON-only and treats pricing variants as
L267:   additive. No-op or pinned catalog bootstrap emits no pricing variants.
L268: - Bootstrap callback delivery must not persist progress or alter migration,
L269:   pricing activation, lock acquisition, failure, or cancellation semantics.
L270: - `Dashboard::sync_command_center` headline and reason describe one ordinary
L271:   sync signal. Busy lock wins, then the newest usage-import `run_log` row with
L272:   `status == "failed"`, then an empty-status state, then ready. Lossy rebuild
L273:   risk is retained only as structured `safety` facts and never selects the
L274:   ordinary-sync warning tone or headline. Recovered `aborted` usage-import
L275:   rows do not select the failed headline after a later successful
L276:   usage-import run. Live job overlay may replace keys for a running, failed,
L277:   cancelled, or completed foreground job; completed overlays use the ready/good
L278:   keys until the refreshed payload is rendered.
L279: - Command-center `last_run` and `safety.recent_failures` read the last N
L280:   usage-import commands (`sync`, `sync --rebuild`, `hook-run`). They must not
L281:   use a mixed `serve`/other-command window. `safety.recent_failures` counts
L282:   `status == "failed"` rows in that window. `RunRecord::counts_as_failure`
L283:   remains the doctor/health predicate, includes aborted recovery, and excludes
L284:   user-cancelled (`cancelled`) runs. The store owns one stale-recovery entry
L285:   point for all three usage-import commands; CLI and JobRegistry must use it.
L286: - Human progress rendering lives in `src/commands/sync_progress.rs` behind one
L287:   event entry and one copy source (`human_progress_line`). TTY stderr renders
L288:   indicatif bars (OpenCode is a spinner because its `files_scanned` counts
L289:   rows, not files; Codex/Claude use determinate bars whose length and position
L290:   both count files planned for replay in the current run). File-backed parser
L291:   workers increment one relaxed atomic counter per completed file; the async
L292:   parser side samples at no more than 5 Hz, emits a boundary snapshot before
L293:   commit, and refreshes committed record counts after commit. A full TTY bar
L294:   shows the commit phase until `SourceFinished`; non-TTY or any non-empty
L295:   `LLMUSAGE_PROGRESS` falls back to plain lines and must never emit ANSI
L296:   escapes. Progress stays on stderr, the `Sync finished` summary table stays
L297:   on stdout, and renderer
L298:   teardown is owned by a command-level RAII guard so early `?` returns,
L299:   failures, and Ctrl-C cancellation all leave a clean terminal. CLI Ctrl-C
L300:   cancels through `run_once_with_cancel`'s token; a ctrl-c task that clones
L301:   the event sender must be aborted and awaited before the reporter channel is
L302:   relied on to close.
L303: - The interactive TUI is a synchronous renderer running inside the process
L304:   Tokio runtime. It must submit sync work through the in-process `JobRegistry`;
L305:   it must never create a nested runtime or call `block_on` from the render
L306:   thread. A second sync action requests cancellation instead of spawning a
L307:   second job. TUI exit cancels an active job and waits only for a documented,
L308:   bounded interval before restoring the terminal.
L309: - The human `Sync finished` block is an aligned table (files/changed/skipped/
L310:   seen/committed/stored plus human-readable bytes and parse/write durations)
L311:   rendered by the pure `format_summary_lines_with_basenames` (tests may call
L312:   `format_summary_lines`, which is the same formatter with an empty path map);
L313:   coloring is stdout-TTY-only and applied after width computation. It ends with a `TOTAL` row aggregated from
L314:   per-source stats. `SourceFinished` closes live stderr progress without
L315:   emitting a second permanent success sentence; failures and cancellation
L316:   remain diagnostic lines. Narrow rendering may truncate only the display
L317:   label, never numeric cells. `SyncEvent`/`SourceSyncStats` wire shapes are
L318:   unaffected by display changes.
L319: 
L320: ### 4. Validation & Error Matrix
L321: 
L322: - Missing sanitized fixture -> keep platform monitor-only and document the gap.
L323: - Unknown token semantics -> keep token quality as unsupported/unknown and do
L324:   not compute costs.
L325: - Second unchanged sync -> `skipped_files > 0`, `changed_files == 0`, and
L326:   imported usage remains available.
L327: - Source rewrite or fingerprint change -> artifact leaves skipped state and the
L328:   focused regression must show refreshed parser/store visibility.
L329: - OpenCode growth with all message anchors present -> keep message and part
L330:   high-waters; database replacement with a missing anchor -> reset both.
L331: - OpenCode `part` table absent -> message sync succeeds and part cursor does not
L332:   advance.
L333: - Existing JSON without `skipped_files` -> serde default must load as `0`.
L334: - Omitted source -> `SyncSourceSelection::All`; an unknown source string ->
L335:   `unknown_source` before a job id or sync worker is created.
L336: - `recent_days` outside `1..=3650` -> `invalid_recent_days`; parser parallelism
L337:   outside `1..=32` -> `invalid_parallelism`. Do not clamp either value.
L338: - A bounded run succeeds -> mark `recent_completed_at` and emit
L339:   `RecentReady` only after every requested parser stage and status write
L340:   completes; cancellation/failure emits neither completion signal.
L341: - A command compatibility entry is called -> delegate to the matching
L342:   `sync::engine` public function with the same arguments and output; do not
L343:   duplicate validation, locking, parser selection, or run-log state.
L344: - Remote Header missing or unequal `source_accounting_versions` -> refuse
L345:   before the first related shard commit; keep that host/source events and
L346:   watermark; name the source and the upgrade action.
L347: - Remote source has rows and historical host/source marker is unknown or
L348:   not current -> refuse current-version incremental mix-in; require a full
L349:   restore (not implemented); keep old rows and watermarks.
L350: - Remote `source-status` with no host/source marker -> `unknown`, never
L351:   local-marker `current`.
L352: - Antigravity selected products all blocked by coverage -> zero usage decoder
L353:   calls and no committed/replayed usage; preserve events, buckets, inventory,
L354:   cursors, and accounting markers. Record actual preflight elapsed time.
L355: - Existing tracked input absent from current discovery -> explain the coverage
L356:   gap without claiming physical deletion; preserve the affected history.
L357: - Antigravity root discovery/access failure -> preserve both product groups,
L358:   including when only the other product was selected. Explicit lossy rebuild
L359:   must not turn access failure into an empty successful snapshot.
L360: 
