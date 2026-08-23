# Codex Tracer 100k synthetic ingestion evidence

## Scope and environment

- Date: 2026-08-24
- Baseline commit: `d594214`
- OS: Windows 11 Pro 10.0.26200 x86_64
- Toolchain: `rustc 1.97.0`, `cargo 1.97.0`
- Corpus: deterministic synthetic JSONL with 100,000 usage events; corpus generation is excluded from the measured import interval.
- Privacy boundary: no user rollout, path, session/thread identifier, or event content was read or recorded.

The two ignored release harnesses are run in separate processes so Windows peak working set is attributable to one import mode:

```powershell
cargo test --release --lib commands::codex_tracer::ingest::tests::codex_tracer_100k_collector_baseline -- --ignored --exact --nocapture
cargo test --release --lib commands::codex_tracer::ingest::tests::codex_tracer_100k_streaming -- --ignored --exact --nocapture
```

## Stable three-run comparison

| Mode | wall ms (3 runs) | p50 | observed max | peak RSS bytes (3 runs) | median RSS | retained-event high-water |
| --- | --- | ---: | ---: | --- | ---: | ---: |
| collector baseline | 2649, 2604, 2578 | 2604 | 2649 | 138604544, 138559488, 138661888 | 138604544 | 100000 |
| bounded streaming | 2849, 2814, 2811 | 2814 | 2849 | 26177536, 25899008, 25968640 | 25968640 | 2048 |

- p50 wall delta: `+8.1%`; observed-max wall delta: `+7.6%`.
- median peak-RSS reduction: `81.3%`.
- retained-event high-water reduction: `97.95%`; the streaming value equals the configured 2,048-event batch ceiling.
- an unchanged warm refresh reported `warm_ms=0`, `records_read=0`, and `rows_written=0`.

The 2,048 default replaces the planning-time 1,000-event guess: it keeps the measured wall regression inside the 10% acceptance budget while retaining an order-of-magnitude memory margin.

## SQLite size evidence

A later reporting run measured the live database plus WAL/SHM separately from the checkpointed size:

| Mode | live database + WAL/SHM bytes | final checkpointed bytes |
| --- | ---: | ---: |
| collector baseline | 49999752 | 49512448 |
| bounded streaming | 122086432 | 50376704 |

The streaming run temporarily carries more WAL because each durable batch is committed independently. After connection close/checkpoint, its database is 1.7% larger than the collector baseline. Live-WAL and final-file sizes must therefore not be conflated.

## Verification boundary

Synthetic before/after evidence is **VERIFIED** by the checked-in ignored harnesses. Representative user-copy p50/p95/RSS and native-browser behavior are **UNVERIFIED** because no explicit authorization to copy or inspect real Codex rollout data was granted.
