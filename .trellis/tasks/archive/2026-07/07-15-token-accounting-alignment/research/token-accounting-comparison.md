# Token Accounting Comparison

## Scope And Snapshots

- llmusage branch: `dev` at the working tree inspected on 2026-07-15.
- ccusage reference: `ref/repo/ccusage` at
  `ba99c0d09b6db9fd64a6187751e8b88a019f991a`.
- tokscale reference: `ref/repo/tokscale` at
  `bfb16de8917058b0c307bb617f7ee9d72320df31`.
- Comparable parser-backed llmusage sources are Claude, Codex, and OpenCode.
  Antigravity is registered as an integration but is absent from
  `registered_parsers()` (`src/registry.rs:23-28`).

## Executive Finding

The defect is not that every cache token must be excluded from every total.
Current ccusage includes cache read and cache creation once in Claude/OpenCode
total usage. The concrete llmusage defect is double counting caused by treating
provider subchannels as independent additive channels after the parser has
already received an inclusive upstream total.

The clearest failure is Codex:

1. Codex `input_tokens` includes cached input and `output_tokens` includes the
   reasoning subchannel.
2. ccusage subtracts `cached_input_tokens` for the displayed non-cached Input,
   keeps Cache Read separate, and preserves upstream `total_tokens`.
3. llmusage leaves explicit `cached_input_tokens` inside Input, then report
   queries add Cache Read and Reasoning again.

For a Codex record with raw input `100`, cached input `40`, output `30`,
reasoning `10`, and upstream total `130`:

| Implementation | Input | Cache Read | Output | Reasoning | Total |
| --- | ---: | ---: | ---: | ---: | ---: |
| ccusage | 60 | 40 | 30 | 10 | 130 |
| tokscale | 60 | 40 | 30 | 10 | 140 via generic component sum |
| current llmusage report | 100 | 40 | 30 | 10 | 180 |
| target llmusage | 60 | 40 | 30 | 10 | 130 |

ccusage is therefore the required authority when ccusage and tokscale disagree.

## Common Token Contract

| Channel | Target meaning | Additive in normalized total? |
| --- | --- | --- |
| `input_tokens` | Non-cached prompt/input only | Yes |
| `cache_creation_tokens` | Cache write/create prompt input | Yes, once |
| `cache_read_tokens` | Cached prompt input reused by the request | Yes, once |
| `output_tokens` | Provider-reported output channel | Yes |
| `reasoning_output_tokens` | Diagnostic reasoning subchannel | Source-aware; never added blindly |
| `total_tokens` | Parser-owned normalized total, preferably an authoritative upstream total | Persist and sum directly |

The current universal sum in `src/query/reports.rs:223-238` violates this
contract. Query code must not reconstruct total usage from all visible fields.

## Claude

### Field Mapping

All three implementations agree on the primary Claude channels:

- `input_tokens` -> non-cached input.
- `cache_creation_input_tokens` -> cache creation/write.
- `cache_read_input_tokens` -> cache read.
- `output_tokens` -> output.

llmusage maps these at `src/parsers/claude.rs:406-432`. ccusage owns the raw
shape at `ref/repo/ccusage/rust/crates/ccusage/src/types.rs:28-47` and totals
the four channels at `types.rs:76-90`. tokscale constructs the same four
channels with reasoning fixed at zero at
`ref/repo/tokscale/crates/tokscale-core/src/sessions/claudecode.rs:675-688`.

### Differences

- llmusage accepts `reasoning_output_tokens` / `thinking_output_tokens` and
  adds it to fallback total (`src/parsers/claude.rs:422-432`). Neither reference
  implementation adds a separate Claude reasoning channel. Until a real
  fixture proves it is disjoint from `output_tokens`, it must be diagnostic and
  non-additive.
- llmusage identifies events by file fingerprint and byte offset
  (`src/parsers/claude.rs:371-372`). It does not deduplicate Claude streaming
  repeats by message/request identity.
- ccusage deduplicates across loaded files using `message.id + requestId`,
  prefers non-sidechain rows, and otherwise keeps the row with the larger token
  total (`ref/repo/ccusage/rust/crates/ccusage/src/adapter/claude/mod.rs:106-123`
  and `:216-290`).
- tokscale also deduplicates streaming repeats and merges per-channel maxima
  (`ref/repo/tokscale/crates/tokscale-core/src/sessions/claudecode.rs:603-689`
  and `:864-879`).

### Target

- Preserve the four primary channels.
- Do not add Claude reasoning to total without fixture-backed proof that it is
  disjoint.
- Add deterministic message/request deduplication, including sidechain replay
  preference and streaming max-merge behavior.

## Codex

### ccusage Baseline

- ccusage reads `last_token_usage` first and falls back to a delta of
  `total_token_usage` (`ref/repo/ccusage/rust/crates/ccusage/src/adapter/codex/parser.rs:269-309`).
- It clamps cached input to raw input.
- JSON and tables expose non-cached input as
  `input_tokens.saturating_sub(cached_input_tokens)` while preserving upstream
  total (`ref/repo/ccusage/rust/crates/ccusage/src/adapter/codex/report.rs:48-94`).
- Reasoning is displayed separately but is not added to `total_tokens`.
- Aggregation deduplicates copied events across files using timestamp, resolved
  model, token tuple, and report scope
  (`ref/repo/ccusage/rust/crates/ccusage/src/adapter/codex/aggregate.rs:21-38`
  and `:229-282`).

### tokscale Cross-check

- tokscale clamps cache to input and stores non-cached input as
  `input - cached` (`ref/repo/tokscale/crates/tokscale-core/src/sessions/codex.rs:164-174`).
- It has stronger cumulative-snapshot handling for stale regressions and fork
  replay (`sessions/codex.rs:484-557`).
- Its generic `TokenBreakdown::total()` adds reasoning
  (`ref/repo/tokscale/crates/tokscale-core/src/lib.rs:217-234`), so its total can
  exceed ccusage when Codex output already includes reasoning. This is a known
  reference disagreement; ccusage wins.

### Current llmusage Gaps

- `cached_input_tokens`, `cache_read_tokens`, and
  `cache_read_input_tokens` all enter one "explicit" branch that leaves raw
  input unchanged (`src/parsers/codex.rs:508-534`). The actual Codex
  `cached_input_tokens` field is inclusive and must be subtracted.
- The parser preserves upstream total, but reports discard it and re-add all
  components (`src/query/reports.rs:352-359`).
- Log and explorer queries contain additional component-sum formulas, while
  other dashboard paths sum persisted bucket totals. The same database can
  therefore show different totals on different surfaces.
- Event keys contain path/fingerprint/offset
  (`src/parsers/codex.rs:431-445`), so copied/archive/fork replays are not
  deduplicated like ccusage.

### Target

- Classify raw cache fields by semantics instead of one alias bucket.
- Normalize Codex input to non-cached input and clamp malformed cache values.
- Preserve source-authoritative request total; reasoning remains a displayed
  subchannel and is not additive.
- Match ccusage copied-event deduplication while retaining llmusage's cursor and
  persistent-store architecture.
- Port tokscale's stale-regression/fork fixtures where they cover cases ccusage
  does not explicitly model.

## OpenCode

### Field Mapping

- llmusage maps `tokens.input`, `tokens.output`, `tokens.cache.write`,
  `tokens.cache.read`, and `tokens.reasoning` directly
  (`src/parsers/opencode.rs:390-428`).
- tokscale uses the same five channels
  (`ref/repo/tokscale/crates/tokscale-core/src/sessions/opencode.rs:181-193`
  and `:295-331`).
- ccusage maps the four billing channels and reads `tokens.total`
  (`ref/repo/ccusage/rust/crates/ccusage/src/adapter/opencode/parser.rs:44-54`
  and `:81-97`). Any positive gap between known components and upstream total
  is retained by `apply_total_token_fallback`.

### Current llmusage Gap

llmusage ignores `tokens.total` and always rebuilds total from the five visible
channels. This commonly matches tokscale, but it can diverge from ccusage when
OpenCode's schema adds an unclassified channel, omits a component, or changes
whether reasoning is included in output.

### Target

- Keep cache write/read separate from input.
- Read and preserve `tokens.total` as the authoritative total when present.
- Use a documented fallback only when total is absent or invalid.
- Keep positive provider-reported OpenCode cost authoritative where the current
  store contract permits it; otherwise recompute from normalized channels with
  the active llmusage catalog.

## Query, Cost, And Display Impact

The correction is cross-layer:

```text
raw source -> parser normalization/dedup -> usage_event.total_tokens
           -> usage_bucket_30m.total_tokens -> query payloads -> CLI/TUI/Web
                                      -> persisted source-aware cost
```

Required cleanup targets include:

- `src/query/reports.rs`: replace `TokenComponents::total_tokens()` with the
  persisted parser-owned total.
- `src/query/logs.rs`: stop rebuilding total from all channels.
- `src/query/explorer.rs` and `src/query/mod.rs`: audit mixed formulas and use
  one persisted-total contract.
- `src/web/assets/data/derive.js` and model renderers: do not add reasoning to
  output share if output is already inclusive.
- `src/query/pricing.rs`: preserve the existing reasoning policy, but feed it
  corrected non-cached input/cache channels. Codex currently risks charging
  cached input at both input and cache-read rates.

## Historical Data And Cursor Impact

- Existing rows cannot be repaired reliably with SQL alone. A row does not
  record whether its input field came from inclusive `cached_input_tokens` or
  an already-exclusive alias, and duplicate Claude/Codex rows require source
  identity evidence.
- Current `source_cursor` has fingerprint/offset/last-total state but no parser
  or token-semantics version (`src/store/mod.rs:39-76`). Unchanged artifacts
  therefore stay on old semantics after a binary upgrade.
- `sync --rebuild` already deletes rebuildable source rows/cursors and has an
  explicit lossy-rebuild guard (`src/commands/sync.rs:487-527`). This is the
  only currently trustworthy full correction path.
- The earlier tokscale-informed design already required parser and
  token-semantics cache versions and invalidation
  (`.trellis/tasks/archive/2026-06/06-12-tokscale-collection-tui-migration/design.md:69-80`),
  but the current cursor schema has not implemented them.

## Research Conclusion

The implementation must be source-aware. A single component-sum helper cannot
produce ccusage-compatible results for all three platforms. The smallest
correct architecture is:

1. Normalize channels and total in each parser.
2. Persist and aggregate `total_tokens` directly.
3. Treat reasoning as source-specific metadata, not universally additive.
4. Add reference-compatible deduplication before results reach SQLite buckets.
5. Version token semantics and require a safe, explicit rebuild of legacy rows
   unless the user approves a different migration policy.
