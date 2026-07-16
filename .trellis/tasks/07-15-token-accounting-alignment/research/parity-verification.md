# Token Accounting Parity Verification

## Reference snapshots

- ccusage: `ba99c0d09b6db9fd64a6187751e8b88a019f991a`
- tokscale: `bfb16de8917058b0c307bb617f7ee9d72320df31`

## Comparable fixture results

`tests/token_accounting_parity.rs` runs the normalized llmusage path through
real source discovery, parser sync, SQLite events/buckets, pricing, Dashboard,
and report queries.

| Source | Input | Cache create | Cache read | Output | Reasoning | Total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Claude streaming/sidechain winner | 20 | 0 | 5 | 10 | 0 | 35 |
| Codex copied event | 60 | 0 | 40 | 30 | 10 | 130 |
| OpenCode authoritative total | 100 | 40 | 20 | 30 | 7 | 250 |

The Codex cost assertion reads the persisted fixed rate and independently
checks corrected input/cache/output channels with absolute error `<= 1e-9`.
Event, bucket, Dashboard, and report totals all equal `415`.

## Commands and results

```powershell
rtk cargo test --test token_accounting_parity -- --test-threads=1
# 2 passed

rtk cargo test --manifest-path ref/repo/ccusage/rust/Cargo.toml -p ccusage
# 359 passed

rtk cargo test --manifest-path ref/repo/tokscale/Cargo.toml -p tokscale-core sessions::codex::tests:: -- --test-threads=1
# 56 passed

rtk cargo test --manifest-path ref/repo/tokscale/Cargo.toml -p tokscale-core deduplication_ -- --test-threads=1
# 10 passed
```

The full tokscale-core suite on this Windows host reported `1131 passed`, `49
failed`, and `1 ignored`. Failures were in Windows/XDG path expectations,
process-global cache isolation/permissions, scanner discovery against the real
home, and two cc-mirror provider attribution assertions. The focused Codex and
Claude deduplication/token slices above pass.

## Intentional reference differences

- ccusage is authoritative when tokscale adds reasoning to a total whose output
  already contains reasoning.
- OpenCode fallback total excludes diagnostic reasoning, matching ccusage.
- llmusage persists parser-owned total and costs; reference live pricing is not
  used as a CI oracle.

## Implementation reconciliation choice

Claude changes trigger a full current-inventory parse from byte zero in one
existing `SyncShard`. Candidates are deduped before the existing atomic
reset/event/cursor commit. This provides streaming replacement, sidechain
winner selection, and cross-file ownership correctness without adding a second
store upsert protocol. Unchanged hot syncs remain skipped.
