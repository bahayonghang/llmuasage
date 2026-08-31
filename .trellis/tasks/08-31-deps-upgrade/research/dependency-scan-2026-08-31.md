# Dependency scan — 2026-08-31

Scan date: 2026-08-31. Toolchain under `rust-toolchain.toml`: rustc/cargo 1.97.0.
Latest installed stable: 1.98.0 (2026-08-18). Declared MSRV: 1.95.

## Ecosystems

| Ecosystem | Manifest | Lock | Notes |
| --- | --- | --- | --- |
| Cargo | `Cargo.toml` | `Cargo.lock` | Product crate + tests |
| npm | `docs/package.json` | `docs/package-lock.json` | VitePress docs only |
| GitHub Actions | `.github/workflows/ci.yml` | SHA pins | Dependabot weekly |
| Rust toolchain | `rust-toolchain.toml`, `justfile`, CI | channel `1.97.0` | MSRV remains 1.95 |
| Python (`scripts/`) | none | n/a | stdlib only |
| Dashboard JS (`scripts/*.mjs`) | none | n/a | Node builtins + `node:test` |

Dependabot already covers cargo, npm `/docs`, and github-actions weekly.

## Direct Cargo dependencies vs crates.io

Resolved versions taken from the `llmusage` package entry in `Cargo.lock`, not the first homonym in the lockfile (several crates exist at two majors).

| Crate | `Cargo.toml` | Locked (direct) | Latest stable | Class |
| --- | --- | --- | --- | --- |
| anyhow | 1.0.104 | 1.0.104 | 1.0.104 | current |
| axum | 0.8.9 | 0.8.9 | 0.8.9 | current (0.9 not released) |
| base64 | 0.23.0 | 0.23.0 | **0.23.1** | patch |
| chrono | 0.4.45 | 0.4.45 | 0.4.45 | current |
| chrono-tz | 0.10.4 | 0.10.4 | 0.10.4 | current |
| clap | 4.6.4 | 4.6.4 | **4.6.6** | patch |
| console | 0.16.4 | 0.16.4 | 0.16.4 | current |
| crossterm | 0.29.0 | 0.29.0 | 0.29.0 | current |
| dashmap | 6.2.1 | 6.2.1 | 6.2.1 | current |
| dirs | 6.0.0 | 6.0.0 | 6.0.0 | current |
| fs4 | 1.1.0 | 1.1.0 | 1.1.0 | current |
| iana-time-zone | 0.1.65 | 0.1.65 | 0.1.65 | current |
| indicatif | 0.18.6 | 0.18.6 | 0.18.6 | current |
| ratatui | 0.30.2 | 0.30.2 | 0.30.2 | current |
| reqwest | 0.12 | **0.12.28** | **0.13.4** | 0.x breaking |
| rusqlite | 0.40.1 | 0.40.1 | **0.40.2** | patch |
| serde | 1.0.229 | 1.0.229 | 1.0.229 | current |
| serde_json | 1.0.151 | 1.0.151 | 1.0.151 | current |
| sha2 | 0.11.0 | 0.11.0 | 0.11.0 | current |
| tempfile | 3.27.0 | 3.27.0 | 3.27.0 | current |
| thiserror | 2.0.19 | 2.0.19 | **2.0.20** | patch |
| tokio | 1.53.1 | 1.53.1 | 1.53.1 | current |
| tokio-util | 0.7.19 | 0.7.19 | 0.7.19 | current |
| toml_edit | 0.25.13 | 0.25.13 | 0.25.13 | current |
| tower-http | 0.7.0 | 0.7.0 | 0.7.0 | current (axum still pulls 0.6.11) |
| tracing | 0.1.44 | 0.1.44 | 0.1.44 | current |
| tracing-appender | 0.2.5 | 0.2.5 | 0.2.5 | current |
| tracing-subscriber | 0.3.23 | 0.3.23 | 0.3.23 | current |
| unicode-width | 0.2.2 | 0.2.2 | 0.2.2 | current |
| walkdir | 2.5.0 | 2.5.0 | 2.5.0 | current |
| zstd | 0.13.3 | 0.13.3 | 0.13.3 | current |
| windows-sys | 0.61.2 | 0.61.2 | 0.61.2 | current |
| proc-macro2 (dev) | 1.0.107 | 1.0.107 | 1.0.107 | current |
| proptest (dev) | 1.11.0 | 1.11.0 | 1.11.0 | current |
| syn (dev) | 2.0.107 | **2.0.119** | **3.0.4** | major (direct still on 2.x) |

Lockfile already contains transitive `syn 1.0.109` and `syn 3.0.3` beside the architecture-test `syn 2.0.119`.

## `cargo update --dry-run` (MSRV 1.95 compatible)

Cargo 1.97 respects `package.rust-version = "1.95"` and would update 48 packages without touching reqwest 0.12 or syn 2. Notable members:

- **lru 0.18.1 → 0.18.3** (fixes RUSTSEC-2026-0253; pulled by `ratatui-core 0.1.2`)
- **chacha20 0.10.1 → 0.10.2** (clears cargo-audit yanked warning; pulled by `rand 0.10.2`)
- clap 4.6.4 → 4.6.6, rusqlite 0.40.1 → 0.40.2, thiserror 2.0.19 → 2.0.20, base64 0.23.0 → 0.23.1
- syn 3.0.3 → 3.0.4 (transitive only)
- miniz_oxide 0.8.9 → 0.9.1 plus new `zlib-rs` (0.x transitive; still MSRV-compatible)

Unchanged behind latest:

- `reqwest 0.12.28` (available 0.13.4)
- `syn 2.0.119` (available 3.0.4)

## Security — Cargo (`cargo audit` 0.22.2)

No denied vulnerabilities. Two allowed warnings:

| ID / kind | Crate | Path | Fix |
| --- | --- | --- | --- |
| RUSTSEC-2026-0253 unsound | lru 0.18.1 | ratatui → ratatui-core | `lru >= 0.18.2`; lockfile update to 0.18.3 |
| yanked warning | chacha20 0.10.1 | rand 0.10.2 | lockfile update to 0.10.2. crates.io currently lists 0.10.1 as not yanked; cargo-audit still warns |

Impact of lru unsound: UAF in `LruCache::pop()` only if a key `Drop` panics and `catch_unwind` is used. ratatui cache keys are not that pattern, but the patched crate is a compatible lockfile bump.

## Security — npm (`npm audit` in `docs/`)

vitepress 1.6.4 is the latest **stable** (2.0.0 is alpha.19 only). Lock:

- vitepress 1.6.4, vite 5.4.21 (latest 5.x), esbuild 0.21.5, nanoid 3.3.11, postcss 8.5.10

| Package | Severity | Range | Fix available | Notes |
| --- | --- | --- | --- | --- |
| nanoid | high | <3.3.18 | yes → 3.3.18 | infinite loop on negative/zero size |
| postcss | high | <=8.5.22 | yes → 8.5.26 | sourceMappingURL path traversal |
| vite | high/moderate | <=6.4.2 | **no on 5.x** | 5.4.21 is latest 5.x; vitepress 1.6.4 depends on `vite ^5.4.14` |
| esbuild | moderate | <=0.24.2 | no via vitepress 1.x | dev-server request spoofing |
| vitepress | moderate | via vite | no | wait for 1.6.5 or 2.x stable |

`docs:build` is static generation. vite/esbuild issues are primarily **dev-server** exposure (`just docs`), not the published HTML. nanoid/postcss still get patch overrides.

## GitHub Actions pins

| Action | Current | Latest | Class |
| --- | --- | --- | --- |
| actions/checkout | v7.0.1 `3d3c42e…` | v7.0.1 | current |
| actions/setup-node | v7.0.0 `8207627…` | v7.0.0 | current |
| Swatinem/rust-cache | comment v2.7.8 `9f151aca…` | **v2.9.2** commit `6323deb1…` | compatible minor (cache-key / Cargo V2 layout) |
| taiki-e/install-action | comment v2 `41049aa5…` | **v2.87.2** `1ed6d7be…` | compatible |
| dtolnay/rust-toolchain | `2c7215f1…` (2026-07-16, 1.97.1) | master `6c977a6c…` (2026-08-05) | compatible pin refresh |
| Node version input | 20 | 20 still valid; 22 is current LTS | deferred |

## Deprecated APIs

- No `#[allow(deprecated)]` in `src/`.
- Product deprecations (`llmusage tui`, `PricingCatalog::static_v1`) are not third-party crate APIs.
- reqwest 0.13 soft-deprecates TLS builder names we do not call. Feature `rustls-tls` **renames** to `rustls`.
- clap 4 remains latest stable (no clap 5 stable).

## Toolchain

- Dev pin 1.97.0 vs latest stable 1.98.0. MSRV 1.95 is independently proven in CI.
- `justfile` `install` hardcodes `cargo +1.97.0`.
- Bumping the pin requires `rust-toolchain.toml`, CI `toolchain:` inputs, and `justfile` together (ci-toolchain-contracts).
