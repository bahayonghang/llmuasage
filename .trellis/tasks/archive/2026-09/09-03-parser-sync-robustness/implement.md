# Implement: OpenCode/ZCode sync robustness

1. Change OpenCode open flags + busy timeout; tests for busy/missing DB.
2. Record malformed tool JSON.
3. Extend `SyncShard` or writer to persist OpenCode/ZCode cursor in `commit_shard`.
4. Remove post-page `save_*_cursor` on the success path (keep load APIs).
5. Run `cargo test --all-features --test sync -- --test-threads=1`.

Depends on ADR 0002 text update if shard shape changes.
