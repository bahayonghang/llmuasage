# ccexplorer Data Storage Guidelines

ccexplorer does not use a database. The current data store is the Claude Code
filesystem JSONL tree plus an in-memory aggregation dictionary.

## Data Flow

The local pattern is:

1. `cli.py` resolves the project root, defaulting to `DEFAULT_PROJECTS_ROOT`.
2. `iter_session_files()` discovers JSONL session files.
3. `aggregate()` walks the files and calls `process_session()` for each path.
4. `process_session()` reads JSONL lines, skips non-cost-bearing turns, and adds
   token/cost buckets to the running aggregate.
5. `web.py` serializes the final `AggregatedRow.to_dict()` rows into HTML.

Reference files:

- `ref/ccexplorer/ccexplorer/data.py`
- `ref/ccexplorer/ccexplorer/web.py`
- `ref/ccexplorer/tests/test_data.py`
- `ref/ccexplorer/tests/test_web.py`

## Aggregation Rules

- Treat each JSONL file as an append-only source of session events.
- Keep aggregation sparse: zero-cost or synthetic rows are skipped in
  `process_session()`.
- Keep cost components separate (`input`, `output`, `cache_read`, cache writes)
  until `AggregatedRow.cost` or `to_dict()` needs a total.
- Preserve the project/session/model/tool grouping already tested in
  `test_data.py`.

## If Persistence Is Added Later

Adding SQLite or another persistent cache would be a design change, not a local
cleanup. It would need explicit migration, invalidation, and regression tests
that prove HTML output stays identical to direct JSONL aggregation.

## Avoid

- Do not call this file a database-backed app in future specs.
- Do not persist derived totals without a source-file invalidation story.
- Do not make the renderer or CLI own aggregation state.
