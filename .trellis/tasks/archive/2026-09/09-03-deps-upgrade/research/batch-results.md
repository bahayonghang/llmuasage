# Batch results

## Batch 0 — GitHub Actions SHA

- Date: 2026-09-03
- Change: `taiki-e/install-action` `1ed6d7be…` # v2.87.2 → `e67fa11c4b9316fa714ddf0abed07a0c3143b95b` # v2.87.4 (arch-gate + security)
- Unchanged: checkout, setup-node, rust-cache, rust-toolchain, Node 20, job names, MSRV 1.95
- `python scripts/check-ci-gate.py --self-test`: exit 0
- `python scripts/check-ci-gate.py`: exit 0
- First `just ci`: rust/js passed; `docs:build` failed because this checkout had no `docs/node_modules` (`vitepress` not on PATH). Not caused by the SHA pin.
- `npm ci --prefix docs` then `just ci`: exit 0 (176s)

## Batch 1 — Cargo lockfile + tower-http 0.7.1

- Date: 2026-09-03
- `cargo update` (MSRV 1.95): 10 packages
  - tower-http 0.7.0 → 0.7.1 (direct; `Cargo.toml` aligned)
  - aws-lc-sys 0.44.0 → 0.45.0
  - aws-lc-rs 1.18.0 → 1.18.1
  - lru 0.18.3 → 0.18.4
  - smallvec 1.15.2 → 1.16.0
  - async-compression 0.4.43 → 0.4.44
  - compression-codecs 0.4.38 → 0.4.39
  - compression-core 0.4.32 → 0.4.33
  - libredox 0.1.21 → 0.1.23
  - mio 1.2.2 → 1.2.3
- Unchanged: reqwest 0.13.4, syn 2.0.119 (direct), rust-version 1.95, rust-toolchain 1.97.0
- `cargo audit --no-fetch --stale`: 0 vulnerabilities (advisory fetch failed on GitHub TLS; used cached DB from 2026-09-02)
- `cargo +1.95 check --locked --all-features` with `CARGO_TARGET_DIR=target/msrv-1.95`: exit 0 (84s, aws-lc-sys 0.45 built)
- `just ci`: exit 0 (310s)

No rollback.
