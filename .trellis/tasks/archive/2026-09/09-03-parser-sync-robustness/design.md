# Design: OpenCode/ZCode sync robustness

## Open flags

`Connection::open_with_flags(path, SQLITE_OPEN_READ_ONLY)` plus `busy_timeout(Duration::from_secs(5))` (or the reader timeout used by Antigravity). Do not write to the user's `opencode.db`.

Busy/open failure: set `stats.last_error`, do not refresh high-water cursor, do not `commit_shard` with empty reset of the whole DB identity.

## parse_issues

On tool-part JSON failure, `parse_issues.record(..., Malformed, ...)` with path hash, then continue other parts.

## Cursor atomicity

Preferred: put OpenCode/ZCode cursor fields on `SyncShard` (or source-specific cursor payload already committed inside `commit_shard`). If the shard protocol is file-cursor-only, add an optional `opencode_cursor` / `zcode_cursor` committed in the same Immediate transaction in `SyncRunWriter`.

Do not leave `save_opencode_cursor` as a second transaction on the success path.

## Tests

- Open flags / busy timeout unit test.
- Malformed part increments parse_issues.
- Failpoint or transactional test: events + cursor commit together.
