# Dependency scan — 2026-09-03

Scan date: 2026-09-03. Toolchain under `rust-toolchain.toml`: rustc/cargo 1.97.0.
Latest installed stable: 1.98.0 (2026-08-18). Declared MSRV: 1.95.

This is a follow-up to archived task `08-31-deps-upgrade`. That round already
landed Actions rust-cache / rust-toolchain pins, an MSRV-compatible lockfile
refresh (including `lru` 0.18.3), docs npm `nanoid`/`postcss` overrides, and
reqwest 0.12 → 0.13.

## Ecosystems

| Ecosystem | Manifest | Lock | Notes |
| --- | --- | --- | --- |
| Cargo | `Cargo.toml` | `Cargo.lock` | Product crate + tests |
| npm | `docs/package.json` | `docs/package-lock.json` | VitePress docs only |
| GitHub Actions | `.github/workflows/ci.yml` | SHA pins | Dependabot weekly |
| Rust toolchain | `rust-toolchain.toml`, `justfile`, CI | channel `1.97.0` | MSRV remains 1.95 |
| Python (`scripts/`) | none | n/a | stdlib only |
| Dashboard JS (`scripts/*.mjs`) | none | n/a | Node builtins + `node:test` |

`ref/` is upstream/reference code and is out of this scan.

Dependabot already covers cargo, npm `/docs`, and github-actions weekly.

## Direct Cargo dependencies vs crates.io

Locked versions from `Cargo.lock` / `cargo metadata` resolve of package
`llmusage`. Latest from crates.io `max_stable_version` on 2026-09-03.

| Crate | `Cargo.toml` | Locked (direct) | Latest stable | Class |
| --- | --- | --- | --- | --- |
| anyhow | 1.0.104 | 1.0.104 | 1.0.104 | current |
| axum | 0.8.9 | 0.8.9 | 0.8.9 | current (0.9 not released) |
| base64 | 0.23.1 | 0.23.1 | 0.23.1 | current |
| chrono | 0.4.45 | 0.4.45 | 0.4.45 | current |
| chrono-tz | 0.10.4 | 0.10.4 | 0.10.4 | current |
| clap | 4.6.6 | 4.6.6 | 4.6.6 | current (no clap 5 stable) |
| console | 0.16.4 | 0.16.4 | 0.16.4 | current |
| crossterm | 0.29.0 | 0.29.0 | 0.29.0 | current |
| dashmap | 6.2.1 | 6.2.1 | 6.2.1 | current |
| dirs | 6.0.0 | 6.0.0 | 6.0.0 | current |
| fs4 | 1.1.0 | 1.1.0 | 1.1.0 | current |
| iana-time-zone | 0.1.65 | 0.1.65 | 0.1.65 | current |
| indicatif | 0.18.6 | 0.18.6 | 0.18.6 | current |
| ratatui | 0.30.2 | 0.30.2 | 0.30.2 | current |
| reqwest | 0.13 | 0.13.4 | 0.13.4 | current (0.13 done 08-31) |
| rusqlite | 0.40.2 | 0.40.2 | 0.40.2 | current |
| serde | 1.0.229 | 1.0.229 | 1.0.229 | current |
| serde_json | 1.0.151 | 1.0.151 | 1.0.151 | current |
| sha2 | 0.11.0 | 0.11.0 | 0.11.0 | current |
| tempfile | 3.27.0 | 3.27.0 | 3.27.0 | current |
| thiserror | 2.0.20 | 2.0.20 | 2.0.20 | current |
| tokio | 1.53.1 | 1.53.1 | 1.53.1 | current |
| tokio-util | 0.7.19 | 0.7.19 | 0.7.19 | current |
| toml_edit | 0.25.13 | 0.25.13+spec-1.1.0 | 0.25.13+spec-1.1.0 | current |
| tower-http | 0.7.0 | **0.7.0** (also transitive 0.6.11) | **0.7.1** | patch |
| tracing | 0.1.44 | 0.1.44 | 0.1.44 | current |
| tracing-appender | 0.2.5 | 0.2.5 | 0.2.5 | current |
| tracing-subscriber | 0.3.23 | 0.3.23 | 0.3.23 | current |
| unicode-width | 0.2.2 | 0.2.2 | 0.2.2 | current |
| walkdir | 2.5.0 | 2.5.0 | 2.5.0 | current |
| zstd | 0.13.3 | 0.13.3 | 0.13.3 | current |
| windows-sys | 0.61.2 | 0.61.2 (also transitive 0.52.0) | 0.61.2 | current |
| proc-macro2 (dev) | 1.0.107 | 1.0.107 | 1.0.107 | current |
| proptest (dev) | 1.11.0 | 1.11.0 | 1.11.0 | current |
| syn (dev) | 2.0.107 | **2.0.119** | **3.0.4** | major (direct still on 2.x) |

Lockfile already contains transitive `syn 1.0.109` and `syn 3.0.4` beside the
architecture-test `syn 2.0.119`.

## `cargo update --dry-run` (MSRV 1.95 compatible)

Cargo 1.97 respects `package.rust-version = "1.95"` and would update 10
packages without touching syn 2 or reqwest 0.13.4:

| Package | From | To | Notes |
| --- | --- | --- | --- |
| async-compression | 0.4.43 | 0.4.44 | transitive |
| aws-lc-rs | 1.18.0 | 1.18.1 | reqwest TLS closure |
| aws-lc-sys | 0.44.0 | **0.45.0** | 0.x native sys crate |
| compression-codecs | 0.4.38 | 0.4.39 | transitive |
| compression-core | 0.4.32 | 0.4.33 | transitive |
| libredox | 0.1.21 | 0.1.23 | transitive |
| lru | 0.18.3 | 0.18.4 | ratatui-core; 08-31 already cleared RUSTSEC-2026-0253 at 0.18.3 |
| mio | 1.2.2 | 1.2.3 | transitive |
| smallvec | 1.15.2 | 1.16.0 | 1.x minor |
| tower-http | 0.7.0 | 0.7.1 | **direct** patch |

Unchanged behind latest (MSRV or semver constraint):

- `generic-array 0.14.7` (available 0.14.9)
- `matchit 0.8.4` (available 0.8.6)
- `syn 2.0.119` (available 3.0.4)

`aws-lc-sys` 0.44 → 0.45 is a 0.x bump. Spec forbids reconstructing a mixed
lockfile by hand, so it rides with `cargo update`. Windows already compiled
aws-lc for reqwest 0.13 on 08-31; this is still the rollback trigger for the
Cargo batch.

## tower-http 0.7.1

Release 2026-08-31. Direct usage in this repo is
`tower_http::compression::CompressionLayer` (`src/web/mod.rs`). `Cargo.toml`
also enables the `fs` feature, but `ServeDir` / `ServeFile` are not called.

0.7.1 `ServeDir::try_call` I/O-error behavioral change does not apply.
Decompression-body fixes do not apply (`compression-gzip` / `compression-br`
only). Align `Cargo.toml` to `0.7.1` after `cargo update`.

## Security — Cargo (`cargo audit` 2026-09-03)

Advisory DB last-updated 2026-09-02. 422 crate dependencies.

- Denied vulnerabilities: **0**
- Informational warnings (unsound / yanked / unmaintained): **none**

08-31 `lru` unsound and `chacha20` yanked warnings are gone.

## Security — npm (`npm audit` in `docs/`)

vitepress 1.6.4 is still the latest **stable** (2.0.0 remains alpha.19 only).
Lock (with existing overrides):

- vitepress 1.6.4, vite 5.4.21 (latest 5.x), esbuild 0.21.5
- nanoid 3.3.18, postcss 8.5.26 (overrides already applied)

| Package | Severity | Range | Fix available | Notes |
| --- | --- | --- | --- | --- |
| vite | high | <=6.4.2 | **no on 5.x** | GHSA-4w7w-66w2-5vf9, GHSA-v6wh-96g9-6wx3, GHSA-fx2h-pf6j-xcff |
| esbuild | moderate | <=0.24.2 | no via vitepress 1.x | GHSA-67mh-4wv8-2f99 dev-server request spoofing |
| vitepress | moderate | via vite | no | wait for 1.6.5 or 2.x stable |

`docs:build` is static generation. vite/esbuild issues are primarily
**dev-server** exposure (`just docs`), not the published HTML. nanoid/postcss
highs from 08-31 remain patched. Do not add a vite/esbuild override.

npm `node_modules` is not present in this checkout; audit used the lockfile
and registry. `npm ls` without install reports unmet vitepress, which is
expected.

## GitHub Actions pins

| Action | Current | Latest | Class |
| --- | --- | --- | --- |
| actions/checkout | v7.0.1 `3d3c42e5…` | v7.0.1 | current |
| actions/setup-node | v7.0.0 `82076278…` | v7.0.0 | current |
| Swatinem/rust-cache | v2.9.2 `6323deb1…` | v2.9.2 | current |
| taiki-e/install-action | comment v2.87.2 `1ed6d7be…` | **v2.87.4** `e67fa11c4b9316fa714ddf0abed07a0c3143b95b` | compatible patch |
| dtolnay/rust-toolchain | `6c977a6c…` (2026-08-05) | master still `6c977a6c…` | current |
| Node version input | 20 | 20 still valid | deferred |

install-action SHAs from `git ls-remote`:

- v2.87.2 `1ed6d7be6168f6c9046541087ff549b6bc581fdf` (current)
- v2.87.3 `0758d235715de2f3551eacc980d9ae8fce9342c3`
- v2.87.4 `e67fa11c4b9316fa714ddf0abed07a0c3143b95b` (target)

## Deprecated APIs

- No `#[allow(deprecated)]` in `src/`.
- Product deprecations (`llmusage tui`, `PricingCatalog::static_v1`) are not
  third-party crate APIs. Out of this task.
- clap 4 remains latest stable.
- reqwest 0.13 already on feature name `rustls`.

## Toolchain

- Dev pin 1.97.0 vs latest stable 1.98.0. MSRV 1.95 is independently proven in CI.
- `justfile` `install` hardcodes `cargo +1.97.0`.
- Bumping the pin requires `rust-toolchain.toml`, CI `toolchain:` inputs, and
  `justfile` together (ci-toolchain-contracts). Not part of a lockfile refresh.

## Classification for this round

### Safe to upgrade (in-scope)

1. GitHub Actions: install-action 2.87.2 → 2.87.4.
2. Cargo MSRV-compatible `cargo update` plus `tower-http` 0.7.0 → 0.7.1 in
   `Cargo.toml`. Includes transitive `aws-lc-sys` 0.45.0.

### Security

- Cargo: none outstanding.
- npm: residual vite/esbuild/vitepress on the docs **dev server** only;
  no compatible fix on VitePress 1.6.4.

### Breaking / deferred

- syn 2.0.119 → 3.0.4 (architecture AST walker).
- rust-toolchain 1.97.0 → 1.98.0.
- VitePress 2.0.0-alpha.19 / vite 8 / esbuild 0.28 override.
- CI Node 20 → 22.
- MSRV 1.95 → 1.96+.
- Unreleased majors: axum 0.9, clap 5, rusqlite 0.41, dashmap 7, dirs 7,
  ratatui 0.31, zstd 0.14, windows-sys 0.62, toml_edit 0.26,
  tracing-subscriber 0.4, fs4 2, console 0.17, crossterm 0.30, indicatif 0.19.

## Scan commands

```text
rustc --version
cargo update --dry-run --verbose --locked
cargo audit
cargo metadata --format-version 1 --locked
npm --prefix docs audit
npm view vitepress version
git ls-remote https://github.com/taiki-e/install-action.git refs/tags/v2.87.4
gh release list --repo actions/checkout --limit 5
```
