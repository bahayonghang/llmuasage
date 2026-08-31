# Batch results — 08-31-deps-upgrade

Host: Windows 10 (PowerShell). Branch: `dev`. No rollbacks.

Residual risk (expected, out of scope): `npm --prefix docs audit` still reports vite ≤6.4.2 (high) and esbuild ≤0.24.2 (moderate) via vitepress 1.6.4. No vite/esbuild override was added.

## Batch 0 — GitHub Actions SHA

Files: `.github/workflows/ci.yml` (`uses:` pins only).

| Command | Exit |
| --- | --- |
| `python scripts/check-ci-gate.py --self-test` | 0 |
| `python scripts/check-ci-gate.py` | 0 |
| `just ci` | 0 |

`just ci` elapsed ~194s. Unchanged: toolchain strings `1.97.0` / `1.95`, Node 20, job name `CI gate`.

## Batch 1 — Cargo lockfile + patch alignment

Files: `Cargo.toml`, `Cargo.lock`.

| Command | Exit |
| --- | --- |
| `cargo update` | 0 |
| `cargo audit` | 0 |
| `cargo +1.95 check --locked --all-features` | 0 |
| `just ci` | 0 |

`just ci` elapsed ~278s. Confirmed in lock: `lru` 0.18.3, `chacha20` 0.10.2, `reqwest` still 0.12.28, direct `syn` still 2.0.119. Direct patches aligned: clap 4.6.6, rusqlite 0.40.2, thiserror 2.0.20, base64 0.23.1. `miniz_oxide` 0.8.9→0.9.1 and `zlib-rs` 0.6.7 did not break tests. `cargo audit` reported no vulnerabilities and no remaining lru/chacha20 warnings.

## Batch 2 — docs npm overrides

Files: `docs/package.json`, `docs/package-lock.json`.

| Command | Exit |
| --- | --- |
| `npm --prefix docs install` | 0 |
| `npm --prefix docs audit` | 1 (vite/esbuild residual only) |
| `npm --prefix docs run docs:build` | 0 |
| `just ci` | 0 |

`just ci` elapsed ~147s. Lock pins `nanoid` 3.3.18 and `postcss` 8.5.26. vitepress remains 1.6.4. Audit no longer reports nanoid or postcss.

## Batch 3 — reqwest 0.13

Files: `Cargo.toml`, `Cargo.lock`. No `src/subscription/**` edits (compile succeeded with existing `Client::builder` / `json()` / `StatusCode`).

| Command | Exit |
| --- | --- |
| `cargo update -p reqwest` | 0 |
| `cargo check --locked --all-features` (Windows smoke) | 0 |
| `python scripts/ci-rust.py` | 0 |
| `cargo +1.95 check --locked --all-features` | 0 |
| `just ci` | 0 |
| `cargo audit` (post-batch) | 0 |

`just ci` elapsed ~139s. Locked `reqwest` 0.13.4 with features `rustls` + `json` + `http2` and `default-features = false`. Windows aws-lc (`aws-lc-sys` 0.44.0) built successfully. `rust-version` still `1.95`; `rust-toolchain.toml` still `1.97.0`; syn still 2.x for architecture tests.
