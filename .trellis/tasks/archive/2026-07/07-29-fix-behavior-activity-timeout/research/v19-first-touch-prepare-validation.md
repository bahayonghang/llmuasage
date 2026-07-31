# Schema v19 first-touch preparation

Captured: `2026-07-30T06:01:58Z`

## Manifest and binary

- Manifest: `target/tmp/activity-first-touch-v19/manifest.json`
- Format: `llmusage.activity-first-touch.v19.v1`, format version `2`
- Binary: `target/debug/llmusage.exe`, `llmusage 1.1.1`
- Binary SHA-256:
  `c985be4d8e92d8f2369aecdcfb070537de8ca3ed980aae2fe94095d9eb272fcd`
- Binary size: `30,743,552` bytes
- Git HEAD recorded by the manifest:
  `6902f4460d9ef24c934cc7e6aa3a338396249869`
- Manifest canonical SHA-256:
  `6d491939958468b5305b7d750a04b1d190b14d3df6f7864fc8036ec8160cd0c3`

## Snapshot evidence

- The live schema-v18 database was read through SQLite online backup. Its
  recorded size and mtime were identical before and after preparation.
- The pinned binary bootstrapped only the isolated first copy to schema v19.
  The harness stopped the server without issuing an Activity request,
  checkpointed the copy, then created the remaining four copies.
- Five ordered snapshots are present. Each is `1,160,073,216` bytes and each
  records schema version `19`, `PRAGMA quick_check=ok`, `178,795` events, and
  `174,569` turns.
- All five snapshots share SHA-256
  `938aea1340b9a4219156ed1587620df484ca34a328999a604047448772eff067`.
- Every snapshot records index columns `event_key,cost_with_cache_usd` and plan
  `SCAN usage_event USING COVERING INDEX idx_usage_event_activity_cost`.
- Task-owned files occupy `5,800,537,466` bytes. There are zero
  `v19-first-touch-consumed.json` markers and zero residual `llmusage.exe`
  processes.

The snapshot databases are now sealed. Do not hash, query, open, bootstrap, or
otherwise consume them before the next Windows boot.

## Post-reboot command

```powershell
python -B '.trellis/tasks/07-29-fix-behavior-activity-timeout/research/profile_activity_first_touch.py' run --manifest 'target/tmp/activity-first-touch-v19/manifest.json'
```
