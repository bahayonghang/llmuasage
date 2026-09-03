# Codex Tracer Contracts

## Overview

Codex Tracer is a detailed Codex usage tracking and analysis module integrated into llmusage. It provides enhanced token accounting, thread tracking, and a dedicated web dashboard for analyzing Codex usage patterns.

**Module Path**: `src/commands/codex_tracer/`  
**Database**: `~/.llmusage/codex-tracer.db` (separate from main llmusage DB)  
**CLI Command**: `llmusage codex-tracer [OPTIONS]`

---

## Architecture

### Module Structure

```
src/commands/codex_tracer/
├── mod.rs              # CLI entry point, JSONL orchestration
├── ingest.rs           # bounded incremental ingestion and durable checkpoints
├── models.rs           # CodexTracerEvent (44 fields), ThreadSummary
├── parser.rs           # JSONL parser with state tracking
├── store.rs            # SQLite storage layer
├── dashboard.rs        # Static HTML generation
├── server.rs           # Axum web server + API endpoints
├── schema.sql          # Database schema (embedded)
└── dashboard/          # Frontend assets (6623 lines, MIT licensed)
    ├── dashboard_template.html
    ├── dashboard.css
    └── dashboard_*.js  # 19 JavaScript modules
```

### Convention: Embedded schema asset

**What**: `schema.sql` is a checked-in runtime asset, not a generated file.

**Why**: `store.rs` loads it with `include_str!("schema.sql")`, so the SQLite schema ships with the binary and stays reviewable as plain SQL.

**Example**:
```rust
let schema = include_str!("schema.sql");
conn.execute_batch(schema)?;
```

**Related**: Keep the schema file next to `store.rs` so the runtime contract and schema text move together.

### Data Flow

```
Codex JSONL files ($CODEX_HOME/rollout/*.jsonl)
  ↓ BoundedJsonlReader (4 MiB record ceiling)
Codex envelope record
  ↓ TracerRecordParser
CodexTracerEvent batches (default ceiling: 2,048)
  ↓ CodexTracerStore::commit_ingest_batch()
SQLite (codex_tracer_events table, 44 columns)
  ↕ tracer_file_state (durable byte boundary + parser state)
  ↓ CodexTracerStore::relink_threads()
  ↓ query_calls(&CallFilters)
  ↓ axum server (localhost:8765)
Web Dashboard (browser)
```

---

## 1. Data Model

### CodexTracerEvent (44 fields)

```rust
pub struct CodexTracerEvent {
    // Identity (4 fields)
    pub record_id: String,              // SHA256(session_id + thread_name + event_timestamp)
    pub session_id: Option<String>,     // Codex session UUID
    pub thread_name: Option<String>,    // Thread name or "main"
    pub session_updated_at: Option<String>, // Session timestamp

    // Thread Linking (4 fields)
    pub thread_key: Option<String>,     // Thread identifier for grouping
    pub thread_call_index: Option<i32>, // Call sequence number within thread
    pub previous_record_id: Option<String>, // Previous call in thread
    pub next_record_id: Option<String>,     // Next call in thread

    // Source Metadata (4 fields)
    pub thread_source: Option<String>,  // "main" or "background"
    pub event_timestamp: String,        // ISO 8601 timestamp
    pub source_file: String,            // JSONL file path
    pub line_number: i32,               // Line number in JSONL

    // Turn Context (6 fields)
    pub turn_id: Option<String>,        // Turn UUID
    pub turn_timestamp: Option<String>, // Turn start timestamp
    pub cwd: Option<String>,            // Working directory
    pub current_date: Option<String>,   // Date at turn time
    pub timezone: Option<String>,       // Timezone
    pub is_archived: bool,              // Whether session is archived

    // Model Configuration (3 fields)
    pub model: Option<String>,          // e.g., "claude-opus-4-8"
    pub effort: Option<String>,         // "low", "medium", "high", etc.
    pub model_context_window: Option<i32>, // Context window size

    // Call Initiator (3 fields)
    pub call_initiator: Option<String>, // "user", "agent", "tool", etc.
    pub call_initiator_reason: Option<String>, // Why the call was made
    pub call_initiator_confidence: Option<String>, // Confidence level

    // Agent Hierarchy (5 fields)
    pub subagent_type: Option<String>,  // e.g., "thread_spawn", "fork"
    pub agent_role: Option<String>,     // Agent role name
    pub agent_nickname: Option<String>, // Agent nickname
    pub parent_session_id: Option<String>, // Parent session UUID
    pub parent_thread_name: Option<String>, // Parent thread name
    pub parent_session_updated_at: Option<String>, // Parent timestamp

    // Token Accounting - Per-Call (6 fields)
    pub input_tokens: Option<i64>,      // Total input tokens
    pub cached_input_tokens: Option<i64>, // Cached portion of input
    pub uncached_input_tokens: Option<i64>, // Uncached portion (computed)
    pub output_tokens: Option<i64>,     // Total output tokens
    pub reasoning_output_tokens: Option<i64>, // Extended thinking tokens
    pub total_tokens: Option<i64>,      // input + output

    // Token Accounting - Cumulative (5 fields)
    pub cumulative_input_tokens: Option<i64>,
    pub cumulative_cached_input_tokens: Option<i64>,
    pub cumulative_output_tokens: Option<i64>,
    pub cumulative_reasoning_output_tokens: Option<i64>,
    pub cumulative_total_tokens: Option<i64>,

    // Computed Metrics (3 fields)
    pub cache_ratio: Option<f64>,       // cached / input
    pub reasoning_output_ratio: Option<f64>, // reasoning / output
    pub context_window_percent: Option<f64>, // cumulative / window
}
```

### Computed Fields

```rust
impl CodexTracerEvent {
    /// Compute cache_ratio = cached_input / input_tokens
    pub fn compute_cache_ratio(&self) -> Option<f64> {
        match (self.cached_input_tokens, self.input_tokens) {
            (Some(cached), Some(input)) if input > 0 => {
                Some(cached as f64 / input as f64)
            }
            _ => None,
        }
    }

    /// Compute reasoning_output_ratio = reasoning_output / output_tokens
    pub fn compute_reasoning_output_ratio(&self) -> Option<f64> {
        match (self.reasoning_output_tokens, self.output_tokens) {
            (Some(reasoning), Some(output)) if output > 0 => {
                Some(reasoning as f64 / output as f64)
            }
            _ => None,
        }
    }

    /// Compute context_window_percent = cumulative_total / context_window
    pub fn compute_context_window_percent(&self) -> Option<f64> {
        match (self.cumulative_total_tokens, self.model_context_window) {
            (Some(cumulative), Some(window)) if window > 0 => {
                Some((cumulative as f64 / window as f64) * 100.0)
            }
            _ => None,
        }
    }

    /// Recompute all derived fields
    pub fn recompute_derived_fields(&mut self) {
        self.cache_ratio = self.compute_cache_ratio();
        self.reasoning_output_ratio = self.compute_reasoning_output_ratio();
        self.context_window_percent = self.compute_context_window_percent();
    }
}
```

---

## 2. Parser Contracts

### parse_codex_jsonl_for_tracer

```rust
pub fn parse_codex_jsonl_for_tracer(
    file_path: &Path
) -> Result<Vec<CodexTracerEvent>>
```

**Input**: Path to Codex JSONL file  
**Output**: Vector of parsed events  
**State**: Stateless (uses None for initial_state)

### parse_codex_jsonl_with_state (Incremental)

```rust
pub fn parse_codex_jsonl_with_state(
    file_path: &Path,
    initial_state: Option<FileParseState>,
) -> Result<(Vec<CodexTracerEvent>, FileParseState)>
```

**Input**:

- `file_path`: Path to JSONL file
- `initial_state`: Optional resume point (byte_offset, line_number, session_id, last_cumulative_total)

**Output**:

- Tuple of (events, final_state)
- `final_state` can be used to resume parsing later

**State Management**:

```rust
pub struct FileParseState {
    pub byte_offset: u64,           // Where to resume reading
    pub line_number: i32,           // Last processed line
    pub session_id: Option<String>, // Session context
    pub last_cumulative_total: i64, // Last cumulative token count
}
```

### Thread Linking

Thread linking happens automatically during parsing:

```rust
fn link_previous_next_records(events: &mut [CodexTracerEvent])
```

**Algorithm**:

1. Group events by `thread_key`
2. Sort each group by `event_timestamp`
3. Assign `thread_call_index` (0, 1, 2, ...)
4. Link `previous_record_id` and `next_record_id`

**Result**: Every event knows its position in the call sequence.

---

## 3. Storage Contracts

### CodexTracerStore

```rust
pub struct CodexTracerStore {
    conn: Connection, // rusqlite connection
}

impl CodexTracerStore {
    pub fn open(db_path: &Path) -> Result<Self>
    pub fn upsert_events(&mut self, events: &[CodexTracerEvent]) -> Result<usize>
    pub fn query_calls(&self, filters: &CallFilters) -> Result<Vec<CodexTracerEvent>>
    pub fn count_events(&self) -> Result<usize>
}
```

### CallFilters

```rust
pub struct CallFilters {
    pub model: Option<String>,          // Filter by model name
    pub since: Option<String>,          // ISO 8601 timestamp
    pub until: Option<String>,          // ISO 8601 timestamp
    pub include_archived: bool,         // Include archived sessions
    pub limit: Option<usize>,           // Max results
}
```

### Upsert Behavior

```rust
// Idempotent: INSERT OR REPLACE
store.upsert_events(&[event.clone()])?;
store.upsert_events(&[event])?; // Same event, no duplicate

let count = store.count_events()?;
assert_eq!(count, 1); // Still 1 event
```

**Key**: `record_id` (SHA256 hash) ensures idempotency.

---

## Scenario: Bounded incremental rollout ingestion

### 1. Scope / Trigger

Use this contract for CLI startup and `/api/refresh` whenever Codex rollout JSONL is imported into the dedicated Tracer database. The public collector functions remain compatibility wrappers; production orchestration must use the bounded engine.

### 2. Signatures

```rust
pub(crate) fn ingest_rollout_dir(
    store: &mut CodexTracerStore,
    rollout_dir: &Path,
    cancellation: &CancellationToken,
    options: CodexTracerIngestOptions,
) -> Result<CodexTracerIngestStats>

pub(crate) fn commit_ingest_batch(
    &mut self,
    source_file: &str,
    state: &TracerFileState,
    events: &[CodexTracerEvent],
    reset_file: bool,
) -> Result<usize>
```

The additive `tracer_file_state` table is keyed by a normalized-path SHA-256 hash and stores fingerprint, observed size/mtime, durable byte offset, durable line number, serialized parser state, and update time. It must be created idempotently when any pre-existing Tracer database opens.

### 3. Contracts

- `BoundedJsonlReader` owns framing, the 4 MiB maximum record size, byte offsets, durable newline classification, malformed JSON classification, and cancellation checks.
- `CodexEnvelopeRecord` exposes only structural `type`/`timestamp`/`payload`/`value` access. Main usage accounting and Tracer event/thread mapping remain consumer-owned.
- The default retained batch ceiling is 2,048 events. Tests may inject a smaller positive ceiling.
- File replay uses durable byte boundary plus fingerprint/size/mtime identity: unchanged reads zero records, append seeks directly to the prior durable offset, and replace resets only that file.
- Events and their matching file checkpoint commit in one SQLite transaction. A failed transaction cannot advance the checkpoint.
- Partial non-newline EOF is never durable. It must be retried from the prior complete record after append.
- Per-batch links may be provisional. After ingestion, `relink_threads()` uses the database-wide ordering to converge `previous_record_id`, `next_record_id`, and `thread_call_index` across batches and files.
- CLI options and successful `/api/refresh` keys remain stable: `ok`, `files_parsed`, `events_found`, `events_inserted`, and `errors`.
- Logs and benchmark evidence may include counts, durations, path hashes, and byte sizes, but must not print raw source paths or record content.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| unchanged fingerprint and size | skip file; `records_read=0`, no checkpoint write |
| same identity with larger file | seek to durable offset and parse only the appended bytes |
| truncate/replace | delete rows for that source and reset its state in the same transaction as the first replacement batch |
| malformed JSON | classify/skip consistently with the main parser; advance only at a complete durable record |
| record over 4 MiB | discard the bounded oversized record without growing an unbounded buffer |
| partial EOF | keep the preceding durable offset and retry the tail later |
| cancellation before commit | return without marking the pending batch successful |
| SQLite batch failure | roll back events, file reset, and checkpoint together |
| one file fails | count the error and continue other files; retain the failed file's prior durable state |

### 5. Good/Base/Bad Cases

- Good: a 100k-event first import retains at most 2,048 events, commits durable batches, globally relinks threads, and a warm refresh reads zero records.
- Base: a one-line append resumes at the saved byte offset and writes only the new event.
- Bad: a truncated file deletes every Tracer row or advances its checkpoint before replacement events commit.

### 6. Tests Required

- Shared reader/envelope corpus: valid, non-usage, malformed, UTF-8, 10 MiB oversized, and partial EOF; assert byte/line durability and unchanged main-parser accounting fixtures.
- Ingestion integration: fresh, unchanged, append, replace, multi-file links, idempotent refresh, cancellation/resume, and clean-rebuild equality.
- Storage: old-schema open twice and injected batch failure; assert state/event/reset atomicity.
- Compatibility: compile legacy parser/store entry points, retain CLI help and API keys, generate the dedicated dashboard.
- Performance: separate-process release baseline/streaming harnesses; assert/record event high-water, rows, wall time, peak RSS, warm-read count, and live versus checkpointed SQLite sizes.

### 7. Wrong vs Correct

#### Wrong

```rust
let all_events = files
    .flat_map(parse_codex_jsonl_for_tracer)
    .collect::<Vec<_>>();
store.upsert_events(&all_events)?;
```

This retains the entire history, cannot durably resume append work, and cannot make event rows and cursor advancement atomic.

#### Correct

```rust
let stats = ingest_rollout_dir(
    &mut store,
    &rollout_dir,
    &cancellation,
    CodexTracerIngestOptions::default(),
)?;
```

The engine bounds retained events, commits the matching checkpoint with each batch, and performs database-wide thread convergence after import.

---

## 4. API Contracts

### Web Server

```rust
pub async fn serve_dashboard(
    db_path: PathBuf,
    port: u16,
    open_browser: bool,
) -> Result<()>
```

**Listener**: bind `127.0.0.1` only. This command has no public bind option.

**Endpoints**:

| Route             | Method | Description                                 |
| ----------------- | ------ | ------------------------------------------- |
| `/`               | GET    | Main dashboard HTML with embedded data      |
| `/api/calls`      | GET    | Query events with filters                   |
| `/api/stats`      | GET    | Event count statistics                      |
| `/api/refresh`    | POST   | Re-ingest rollout JSONL into the tracer DB  |
| `/dashboard.js`   | GET    | Main dashboard JavaScript                   |
| `/dashboard_*.js` | GET    | 18 other JavaScript modules                 |

GET `/api/refresh` is not mounted. The shipped router must return 405 and must not ingest.

### GET /api/calls

**Query Parameters**:

- `model` (optional): Filter by model name
- `since` (optional): ISO 8601 timestamp
- `until` (optional): ISO 8601 timestamp
- `include_archived` (optional): boolean, default false
- `limit` (optional): integer. Clamp to `LIST_QUERY_LIMIT_MAX` (500). Omitted, zero, negative, or oversized values use that cap.

SQL uses a bound `LIMIT ?`. Do not interpolate the limit into the statement text. `/` and static dashboard generation use a separate named budget, `INDEX_QUERY_LIMIT` (10_000).

**Response**:

```json
{
  "calls": [
    {
      "record_id": "abc123...",
      "session_id": "uuid",
      "model": "claude-opus-4-8",
      "input_tokens": 1000,
      "output_tokens": 500
      // ... all 44 fields
    }
  ],
  "count": 42
}
```

### GET /api/stats

**Response**:

```json
{
  "total_events": 1234
}
```

### POST /api/refresh

Re-ingest `$CODEX_HOME/rollout` through `ingest_rollout_dir`. GET must not run this path.

**Success**:

```json
{
  "ok": true,
  "files_parsed": 1,
  "events_found": 10,
  "events_inserted": 10,
  "errors": 0
}
```

### HTTP error bodies

JSON failures use a stable `code` plus a short `message`. HTML/plain failures for `/` use the same `code` and `message`. Do not include `detail`, filesystem paths, or `rusqlite`/`SQLITE` text. Log the cause with `tracing`.

```json
{
  "error": {
    "code": "codex_not_found",
    "message": "Codex rollout directory not found"
  }
}
```

---

## 5. CLI Contracts

### Command

```bash
llmusage codex-tracer [OPTIONS]
```

**Options**:

- `--port <PORT>`: Web server port (default: 8765)
- `--no-open`: Don't automatically open browser
- `--rebuild`: Delete and rebuild database from JSONL

**Environment**:

- `$CODEX_HOME`: Codex installation directory
- Default: `~/.codex`

**Behavior**:

1. Check database: `~/.llmusage/codex-tracer.db`
2. If empty or `--rebuild`:
   - Find JSONL files in `$CODEX_HOME/rollout/`
   - Parse all `*.jsonl` files
   - Insert events into database
3. Start web server on specified port
4. Open browser (unless `--no-open`)
5. Block serving requests

**Exit Conditions**:

- Ctrl+C (graceful shutdown)
- Server error
- Port already in use

---

## 6. Error Handling

### Common Errors

| Error           | Cause                               | CLI / log                                                                          | HTTP body                                              |
| --------------- | ----------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------ |
| Codex not found | `$CODEX_HOME/rollout` doesn't exist | CLI may print the path. HTTP logs the path with `tracing`.                         | `codex_not_found` / "Codex rollout directory not found" |
| No events       | No JSONL files or all empty         | CLI may print the path.                                                            | Not an HTTP error; refresh returns counts.             |
| Parse error     | JSONL format invalid                | Warning logged, continue with other files                                          | Counted in refresh `errors`; no path in JSON.          |
| Port in use     | Another server on same port         | "Failed to bind to {addr}"                                                         | Process does not start.                                |
| Database error  | Disk full, permissions, query fail  | Structured log with the cause                                                      | `internal_error` plus a short static message           |

### Error Recovery

```rust
// Parser: Skip invalid lines, continue processing
for (line_number, line) in reader.lines().enumerate() {
    let line = line.context("Failed to read line")?;
    let envelope: Value = match serde_json::from_str(&line) {
        Ok(v) => v,
        Err(_) => continue, // Skip invalid JSON
    };
    // ...
}

// File iteration: Log errors, continue with next file
for entry in walkdir::WalkDir::new(&rollout_dir) {
    match parse_codex_jsonl_for_tracer(path) {
        Ok(events) => all_events.extend(events),
        Err(err) => {
            tracing::warn!(file = %path.display(), error = %err);
            error_count += 1;
        }
    }
}
```

---

## 7. Testing Strategy

### Unit Tests (15 total)

**models.rs** (3 tests):

- `test_codex_tracer_event_computed_fields` - Computed field logic
- `test_recompute_derived_fields` - Field recalculation
- `test_zero_token_edge_cases` - Division by zero handling

**store.rs** (4 tests):

- `test_store_open_and_init` - Database initialization
- `test_upsert_and_query_events` - CRUD operations
- `test_query_with_filters` - Filter functionality
- `test_idempotent_upsert` - INSERT OR REPLACE behavior

**parser.rs** (6 tests):

- `test_generate_record_id` - SHA256 hash generation
- `test_compute_thread_key` - Thread key computation
- `test_extract_session_id_from_path` - UUID extraction
- `test_is_archived_path` - Archived session detection
- `test_link_previous_next_records` - Thread linking
- `test_file_parse_state` - Incremental parsing state

**dashboard.rs** (2 tests):

- `test_escape_html` - XSS prevention
- `test_generate_dashboard_basic` - Dashboard generation

### Integration Tests

Implemented synthetic integration coverage includes bounded JSONL parsing, incremental file-state replay, rollback/cancellation, and static dashboard generation. Native browser automation with representative user data remains `UNVERIFIED` unless separately authorized.

---

## 8. Design Decisions

### Decision 1: Separate Database

**Context**: Should codex-tracer use the main llmusage database or a separate one?

**Options**:

1. Shared database - Reuse existing infrastructure
2. Separate database - Isolated schema and queries

**Decision**: Separate database (`codex-tracer.db`)

**Rationale**:

- Different schema requirements (44 fields vs. 30-min buckets)
- Independent evolution (codex-tracer can change schema without affecting llmusage)
- Performance isolation (heavy queries don't slow down main app)
- Easy to rebuild/delete without affecting main data

**Trade-off**: Data duplication (some events in both databases).

### Decision 2: Pure Rust Implementation

**Context**: Should we wrap the Python codex-usage-tracker or rewrite in Rust?

**Options**:

1. Python wrapper - Fast to implement
2. Pure Rust rewrite - More work upfront

**Decision**: Pure Rust rewrite

**Rationale**:

- Matches llmusage architecture (single binary)
- No Python runtime dependency
- Better performance (native, no IPC)
- Full control over schema and queries
- Can reuse llmusage infrastructure (rusqlite, axum, etc.)

**Implementation Time**: ~8 hours for MVP (Phase 1-5).

### Decision 3: Frontend Asset Embedding

**Context**: How to serve dashboard HTML/JS/CSS?

**Options**:

1. External files - User needs to copy assets
2. Embedded assets - All in binary via `include_str!()`

**Decision**: Embedded assets

**Rationale**:

- Single binary deployment (no asset copying)
- Assets can't get out of sync with binary
- Slightly larger binary (~300KB), but worth it for UX

**License Compliance**: MIT license permits embedding with attribution.

---

## 9. Common Mistakes

### Mistake 1: Forgetting to Recompute Derived Fields

**Symptom**: `cache_ratio`, `reasoning_output_ratio`, and `context_window_percent` are None even when tokens are present.

**Cause**: Not calling `recompute_derived_fields()` after setting token fields.

**Fix**:

```rust
let mut event = CodexTracerEvent {
    input_tokens: Some(1000),
    cached_input_tokens: Some(600),
    // ...
    cache_ratio: None, // Still None!
    ..Default::default()
};

event.recompute_derived_fields(); // Must call this!
assert_eq!(event.cache_ratio, Some(0.6));
```

### Mistake 2: Treating Per-Batch Thread Links as Final

**Symptom**: `previous_record_id` and `next_record_id` are None.

**Cause**: Persisting batch-local links without database-wide convergence.

**Fix**:

```rust
// Wrong: links only the retained batch.
link_previous_next_records(&mut batch);
store.upsert_events(&batch)?;

// Correct: batch-local work is followed by database-wide convergence.
store.commit_ingest_batch(source_file, &state, &batch, reset_file)?;
store.relink_threads()?;
```

### Mistake 3: Using the Collector Wrapper in Production

**Symptom**: Out of memory when parsing large JSONL files.

**Cause**: `parse_codex_jsonl_for_tracer()` and `parse_codex_jsonl_with_state()` intentionally return a compatibility `Vec`; parser state alone does not impose a batch ceiling.

**Fix**: Route CLI and refresh imports through `ingest_rollout_dir()`:

```rust
ingest_rollout_dir(
    &mut store,
    &rollout_dir,
    &cancellation,
    CodexTracerIngestOptions::default(),
)?;
```

---

## 10. Future Enhancements (Phase 6-7, P1)

### Phase 6: Advanced Features

- [ ] Thread summaries view (`/api/threads`)
- [ ] Call investigator (detailed single-call panel)
- [ ] Advanced filtering (search by cwd, thread name)

### Phase 7: Optimization

- [ ] Parallel parsing with rayon
- [x] Benchmark with 100k synthetic events (see the bounded-ingestion task evidence)
- [ ] SQLite query optimization (EXPLAIN QUERY PLAN)
- [ ] README.md documentation
- [ ] User guide (docs/guide/codex-tracer.md)

---

## 11. Related Specs

- [Source Sync Contracts](./source-sync-contracts.md) - Main llmusage parser contracts
- Domain docs: `docs/agents/domain.md` - Platform onboarding
- Parser onboarding: `docs/agents/passive-parser-onboarding.md`
