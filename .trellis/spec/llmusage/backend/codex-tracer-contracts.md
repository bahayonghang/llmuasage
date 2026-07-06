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
  ↓ parse_codex_jsonl_for_tracer()
CodexTracerEvent[] (44 fields)
  ↓ CodexTracerStore::upsert_events()
SQLite (codex_tracer_events table, 44 columns)
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

## 4. API Contracts

### Web Server

```rust
pub async fn serve_dashboard(
    db_path: PathBuf,
    port: u16,
    open_browser: bool,
) -> Result<()>
```

**Endpoints**:

| Route             | Method | Description                            |
| ----------------- | ------ | -------------------------------------- |
| `/`               | GET    | Main dashboard HTML with embedded data |
| `/api/calls`      | GET    | Query events with filters              |
| `/api/stats`      | GET    | Event count statistics                 |
| `/api/refresh`    | GET    | Re-parse JSONL files (placeholder)     |
| `/dashboard.js`   | GET    | Main dashboard JavaScript              |
| `/dashboard_*.js` | GET    | 18 other JavaScript modules            |

### GET /api/calls

**Query Parameters**:

- `model` (optional): Filter by model name
- `since` (optional): ISO 8601 timestamp
- `until` (optional): ISO 8601 timestamp
- `include_archived` (optional): boolean, default false
- `limit` (optional): integer

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

| Error           | Cause                               | User Message                                                                     |
| --------------- | ----------------------------------- | -------------------------------------------------------------------------------- |
| Codex not found | `$CODEX_HOME/rollout` doesn't exist | "Codex rollout directory not found: {path}\nPlease ensure Codex is installed..." |
| No events       | No JSONL files or all empty         | "No events found in {path}\nPlease ensure you have used Codex at least once."    |
| Parse error     | JSONL format invalid                | Warning logged, continue with other files                                        |
| Port in use     | Another server on same port         | "Failed to bind to {addr}"                                                       |
| Database error  | Disk full, permissions              | "Failed to open codex-tracer database"                                           |

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

**Not Yet Implemented** (requires real Codex data):

- End-to-end JSONL parsing
- Web server endpoint tests
- Browser automation tests

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

### Mistake 2: Thread Linking Before All Events Parsed

**Symptom**: `previous_record_id` and `next_record_id` are None.

**Cause**: Calling `link_previous_next_records()` before parsing all files.

**Fix**:

```rust
// Wrong
for file in files {
    let mut events = parse_file(file)?;
    link_previous_next_records(&mut events); // Links only within this file!
}

// Correct
let mut all_events = Vec::new();
for file in files {
    let events = parse_file(file)?;
    all_events.extend(events);
}
link_previous_next_records(&mut all_events); // Links across all files
```

### Mistake 3: Ignoring Parser State for Large Files

**Symptom**: Out of memory when parsing large JSONL files.

**Cause**: Using `parse_codex_jsonl_for_tracer()` which loads everything into memory.

**Fix**: Use `parse_codex_jsonl_with_state()` with chunking:

```rust
let mut state = FileParseState::new();
loop {
    let (events, new_state) = parse_codex_jsonl_with_state(path, Some(state))?;
    if events.is_empty() {
        break; // Done
    }
    store.upsert_events(&events)?;
    state = new_state;
}
```

---

## 10. Future Enhancements (Phase 6-7, P1)

### Phase 6: Advanced Features

- [ ] Thread summaries view (`/api/threads`)
- [ ] Call investigator (detailed single-call panel)
- [ ] Advanced filtering (search by cwd, thread name)

### Phase 7: Optimization

- [ ] Parallel parsing with rayon
- [ ] Benchmark with 10k+ events
- [ ] SQLite query optimization (EXPLAIN QUERY PLAN)
- [ ] README.md documentation
- [ ] User guide (docs/guide/codex-tracer.md)

---

## 11. Related Specs

- [Source Sync Contracts](./source-sync-contracts.md) - Main llmusage parser contracts
- Domain docs: `docs/agents/domain.md` - Platform onboarding
- Parser onboarding: `docs/agents/passive-parser-onboarding.md`
