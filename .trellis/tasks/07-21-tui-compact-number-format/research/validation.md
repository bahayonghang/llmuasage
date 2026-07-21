# Validation

## Test-first evidence

- Red: `cargo test tui::format --lib` failed only because `stat_compact` was not yet defined.
- Green: formatter tests cover K/M/B/T, one-decimal trimming, cross-unit promotion, negatives,
  `i64::MAX`, and `i64::MIN`.

## Focused checks

- `cargo test tui:: --lib -- --test-threads=1`: passed, 69 passed and 2 existing local-data
  benchmarks ignored.
- `cargo test --test tui_panels_prop -- --test-threads=1`: passed, 21 passed.
- Screenshot-scale Overview rendering passed at 120x30 and 80x30.
- Daily, Hourly, Stats, Behavior, and Blocks render M/B-scale compact values; Models and Cost
  property tests generate values through 20B.
- Usage sync rendering still asserts exact grouped `8,000`; report-table tests retain grouped
  counts and two-decimal `K/M/B` output.

## Full gate

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test -- --test-threads=1`: passed with no failures; the main library ran 369 tests.
- `just ci`: passed, including Rust gates, Node checks, and VitePress build.
- `git diff --check`: passed.
