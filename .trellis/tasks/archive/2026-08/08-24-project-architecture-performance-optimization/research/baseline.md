# Review Baseline

## Repository State

- Date: 2026-08-24 (Asia/Shanghai)
- Branch/HEAD: `dev` / `d594214`
- Start state used for final planning: clean, `dev...origin/dev [ahead 4]`
- Product code changes made by this review: none

## Structural Inventory

| Surface | Total lines | First top-level `#[cfg(test)]` | Review interpretation |
| --- | ---: | ---: | --- |
| `src/query/mod.rs` | 7,137 | 4,199 | about 4,198 production lines; flagged |
| `src/web/mod.rs` | 6,097 | 78 | mostly colocated tests; not flagged by size |
| `src/query/reports.rs` | 3,123 | 2,366 | about 2,365 production lines; flagged for duplication |
| `src/store/migrations.rs` | 2,797 | 176 | mostly migration tests; not flagged by size |
| `src/store/sync_writer.rs` | 2,638 | 1,225 | commit protocol plus extensive tests |
| `src/commands/sync.rs` | 1,296 | 1,199 | CLI output and application engine mixed |
| `src/commands/codex_tracer/**` | 8,415 | mixed | independent vertical subsystem |

## Focused Verification

| Command | Result | Evidence boundary |
| --- | --- | --- |
| `cargo test daily_monthly_reports_ignore_large_event_backlog_without_project_filter -- --test-threads=1 --nocapture` | PASS, 1/1; test body 4.23 s | 100k synthetic backlog; includes fixture construction and query |
| `cargo test --test architecture_dependencies -- --test-threads=1` | PASS, 3/3; 0.05 s | proves only the currently encoded forbidden edges |
| `cargo test home_overview_under_80ms_with_seeded_10k_events -- --test-threads=1 --nocapture` | PASS, profile total 81.17 ms | current local test limit is 150 ms; synthetic 10k; plans reported three `SCAN usage_event` stages |

## Historical Performance Evidence Used for Candidate Ranking

- `07-11-dashboard-time-range-performance`: representative interactive API p95 142-239 ms and 15-66 KiB, within 400 ms / 128 KiB at that historical HEAD.
- `08-23-top-sessions-query-index-optimization`: range/filter matrix query p95 at most 242.43 ms on a temporary verified database copy.
- `07-20-sync-full-profiling`: historical cold import found writer dominance; later code changed, so it is not accepted as a current baseline.

These historical numbers are not re-labelled as current PASS. Current release/RSS/cold-cache/real-browser evidence remains `UNVERIFIED` until the owning child refreshes it.
