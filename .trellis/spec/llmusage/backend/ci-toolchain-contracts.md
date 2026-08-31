# CI And Toolchain Contracts

## 1. Scope / Trigger

Apply this contract when changing `Cargo.toml` dependencies or `rust-version`,
the pinned development toolchain, Rust CI commands, subprocess integration
tests, GitHub Actions SHA pins in `.github/workflows/ci.yml`, or `docs/` npm
lockfile / overrides.

## 2. Signatures

- Shared Rust gate: `python scripts/ci-rust.py`
- Full local gate: `just ci`
- MSRV proof: `cargo +<rust-version> check --locked --all-features`
- Required-check contract: `python scripts/check-ci-gate.py`
- Live protection probe: `python scripts/check-ci-gate.py --github-protection`
- Integration-test graph: `[package] autotests = false` plus explicit targets
  `api`, `architecture_dependencies`, `cli`, `query`, `remote`, `store`,
  `sync`, and `tui`.

## 3. Contracts

- `Cargo.toml package.rust-version` is the authoritative MSRV declaration.
- The GitHub Actions MSRV job must install that exact version and run the MSRV
  proof command above.
- The pinned `rust-toolchain.toml` may be newer than MSRV; it is the normal
  development toolchain, not a second compatibility claim.
- Local `just ci` and the three-platform Rust CI matrix must invoke
  `scripts/ci-rust.py` instead of maintaining duplicate Rust command lists.
- All dependency-sensitive CI commands use `--locked`; clippy and tests use
  `--all-features`.
- `cargo update` must keep the declared MSRV. Cargo 1.97+ will lock to the
  latest versions compatible with `package.rust-version`. After an update,
  align direct `Cargo.toml` patch numbers with the lockfile so the manifest
  is not stale.
- `0.x` breaking upgrades (for example reqwest 0.12 → 0.13) stay in a
  separate batch from an MSRV-compatible lockfile refresh. Do not reconstruct
  a mixed `Cargo.lock` by hand.
- reqwest 0.13 feature name is `rustls`, not `rustls-tls`. Keep
  `default-features = false` plus `json` and `http2` unless a later changelog
  says otherwise.
- Actions third-party steps stay SHA-pinned. Refresh `Swatinem/rust-cache`,
  `taiki-e/install-action`, and `dtolnay/rust-toolchain` together with a
  comment that names the tag. Do not change the `CI gate` job `name:`.
- docs npm: patch nested high CVEs with `overrides` (nanoid, postcss). Do not
  override `vite` / `esbuild` while VitePress 1.6.x depends on `vite ^5`.
  Residual vite/esbuild advisories on `docs:dev` are accepted until VitePress
  stable can pull vite ≥ 6.4.3.
- GitHub branch protection on `main` requires exactly one Actions check:
  `CI gate`. That string is the `ci-gate` job `name:` in
  `.github/workflows/ci.yml`. GitHub matches the job display name, not the
  job id.
- `ci-gate` must use `if: always()` and `needs` every other job in that
  workflow. A skipped required check blocks merge the same way a missing
  check does.
- Do not put matrix cell names such as `Rust (windows-latest)` or versioned
  names such as `MSRV (1.95)` in branch protection. Those names move when
  the matrix or MSRV changes.
- Integration tests live under domain directories and are discovered only
  through the eight explicit Cargo targets. Keep `architecture_dependencies`
  stable because CI addresses it by name. Before adding coverage during a
  layout migration, compare the old and new integration-test leaf-name
  multisets so newly added tests cannot hide a silently lost old test.

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Declared MSRV fails the locked all-features check | Block; raise `rust-version` or deliberately select compatible dependencies |
| Version immediately below declared MSRV passes | Lower the declaration and repeat the proof |
| Shared Rust gate fails locally or in one CI OS | Block; do not bypass that command in only one environment |
| `main` required check name is absent from workflow job names | Block merge; restore `CI gate` or update protection in the same change |
| A new CI job is omitted from `ci-gate.needs` | `python scripts/check-ci-gate.py` fails |
| `ci-gate` is skipped after a leaf job fails | Block; keep `if: always()` so the required check still reports |
| Subprocess test reports an OS error | Locate the exact failing operation; do not label it an environment failure without context |
| A test-layout change drops or duplicates an existing leaf name | Block before adding new tests; repair target/module wiring |
| `cargo audit` reports unsound/yanked that `cargo update` can absorb (e.g. lru via ratatui-core) | Absorb in the lockfile batch; do not leave a fixable warning |
| reqwest enabled with `rustls-tls` after 0.13 | Block compile; rename the feature to `rustls` |
| `npm --prefix docs audit` still reports nanoid or postcss high | Block; pin overrides to patched versions and refresh `docs/package-lock.json` |
| `npm --prefix docs audit` reports only vite/esbuild via vitepress 1.6.x | Accept; do not force VitePress 2 alpha or a vite 6 override |

## 5. Good / Base / Bad Cases

- Good: the declared version passes from a clean target and the immediately
  lower candidate has a recorded dependency/compiler failure.
- Good: a move-only checkpoint preserves every existing integration leaf
  exactly once before new coverage increases the count.
- Base: the pinned development toolchain passes the same shared Rust gate.
- Bad: `Cargo.toml` claims an old MSRV while CI silently tests a newer version,
  or local and CI gates use different argument sets.
- Bad: branch protection still requires `Rust and docs` after that job name
  is removed, so pull requests stay `BLOCKED` while every current job is green.
- Bad: move `tests/foo.rs` into `tests/sync/foo.rs` while relying on Cargo's
  root auto-discovery, then treat newly added tests as proof that no old test
  disappeared.

## 6. Tests Required

- Run the MSRV proof with an isolated `CARGO_TARGET_DIR` after dependency
  updates.
- Run `python scripts/ci-rust.py` before committing Rust changes.
- Run `python scripts/check-ci-gate.py --self-test` and
  `python scripts/check-ci-gate.py` before committing workflow or required-check
  changes. Run `--github-protection` after changing `main` protection.
- Subprocess regression tests must assert the executable exists and attach
  spawn context; they must consume current public runtime paths/readers rather
  than stale compatibility fields.
- Test-graph changes must run `cargo metadata --no-deps --format-version 1`,
  list all eight explicit targets, reconcile the pre-move leaf-name multiset,
  then run every target locked/all-features/single-threaded.

## 7. Wrong vs Correct

### Wrong

```yaml
name: MSRV (1.89)
# Cargo.toml still says rust-version = "1.85"
```

### Correct

```yaml
name: MSRV (1.95)
run: cargo check --locked --all-features
```

`Cargo.toml` must declare the same `1.95`, and `1.95` must be established by
running the command rather than inferred from direct dependency metadata.

For the integration-test graph:

### Wrong

```toml
# Files moved below tests/sync/, but Cargo still relies on auto-discovery.
```

### Correct

```toml
[package]
autotests = false

[[test]]
name = "sync"
path = "tests/sync/main.rs"
```

Every other domain target follows the same explicit pattern, and the move-only
leaf-name reconciliation runs before additive coverage.

### Wrong

```yaml
# branch protection requires "Rust and docs"
jobs:
  rust:
    name: Rust (${{ matrix.os }})
  docs-and-js:
    name: Docs and dashboard JS
```

### Correct

```yaml
ci-gate:
  name: CI gate
  if: always()
  needs: [rust, msrv, docs-and-js, arch-gate, security]
```

`main` required checks must be exactly `CI gate`.

## Scenario: Dependency and lockfile upgrades

### 1. Scope / Trigger

- Trigger: bumping crate versions, refreshing `Cargo.lock`, changing docs npm
  overrides, or retargeting Actions SHA pins.
- Keep MSRV (`rust-version`) and the development toolchain pin as two
  independent claims. A lockfile refresh is not permission to raise either.

### 2. Signatures

- Compatible refresh: `cargo update` then `cargo +<rust-version> check --locked --all-features` with an isolated `CARGO_TARGET_DIR`
- Security: `cargo audit`
- Docs: `npm --prefix docs install` after override edits, then `npm --prefix docs audit` and `npm --prefix docs run docs:build`
- Full gate: `just ci`

### 3. Contracts

- Direct runtime reqwest stays `default-features = false` with explicit TLS.
  After 0.13 the TLS feature is `rustls`.
- Architecture tests keep `syn` 2 (`full`, `visit`) until a dedicated syn 3
  batch rewrites `tests/architecture/main.rs`. Transitive syn 3 in the lock
  is allowed.
- `docs/package.json` may add `overrides` for nested patched majors; it must
  not add a vite/esbuild override to silence advisories VitePress 1.6 cannot
  take.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Lockfile refresh fails MSRV check | Roll back `Cargo.toml` + `Cargo.lock`; do not raise MSRV to paper over it |
| reqwest 0.13 fails aws-lc native build on one OS | Roll back only the reqwest closure; keep the compatible lockfile batch |
| Override pins drift from `docs/package-lock.json` | Re-run `npm --prefix docs install` and commit both files |

### 5. Good / Base / Bad Cases

- Good: `cargo update` moves `lru` 0.18.1 → 0.18.3 to clear RUSTSEC unsound
  without bumping ratatui.
- Base: clap/rusqlite/thiserror/base64 patch numbers in `Cargo.toml` match
  the lockfile after the refresh.
- Bad: enabling reqwest `rustls-tls` on 0.13, or `npm override` of `vite` to
  8.x while VitePress still declares `vite ^5`.

### 6. Tests Required

- Isolated MSRV `cargo +<rust-version> check --locked --all-features`
- `cargo audit` with no denied vulnerabilities
- `python scripts/ci-rust.py` after any Rust lock or manifest change
- `npm --prefix docs run docs:build` after docs lock or override changes

### 7. Wrong vs Correct

#### Wrong

```toml
reqwest = { version = "0.13", default-features = false, features = ["rustls-tls", "json", "http2"] }
```

#### Correct

```toml
reqwest = { version = "0.13", default-features = false, features = ["rustls", "json", "http2"] }
```

## Scenario: AST-Enforced Layer Dependencies

### 1. Scope / Trigger

- Trigger: adding or changing a forbidden Rust dependency direction between
  application/domain layers and outer adapters.
- ARCH-002 forbids every `src/sync/**` and `src/remote/**` dependency on
  `crate::commands/**`, including aliases and relative paths.
- ARCH-003 forbids every non-command production layer from depending on
  `crate::commands::sync/**`.
- ARCH-004 forbids `src/commands/**` from implementing traits or concrete
  types owned by `crate::sync/**`.

### 2. Signatures

- Local and Actions gate:
  `cargo test --locked --all-features --test architecture_dependencies`
- Violation output: `<source-file>:<line> depends on forbidden target <path>`.
- Canonical default: `sync::DefaultSyncExecutor`; compatibility alias:
  `commands::sync::CommandSyncExecutor`.

### 3. Contracts

- Parse every Rust file in the protected layer with `syn`; do not enforce the
  boundary with a grep for one spelling.
- Resolve `crate`, `self`, `super`, `use crate as <alias>`, and
  `extern crate self as <alias>` paths before comparing the dependency target.
- The sync application layer owns the executor trait, `DefaultSyncExecutor`,
  the engine, and `JobRegistry::default()`. Outer adapters may inject test or
  product-specific executors but cannot own the canonical implementation.
- `commands::sync` may re-export sync-owned types and delegate public wrappers;
  it cannot provide impl blocks for sync-owned traits/types.
- Web/TUI and other non-command consumers use `crate::sync` directly. A
  compatibility import through `commands::sync` is allowed only to external
  callers and compile fixtures, not production layering.
- Parser dependencies used only by the architecture test remain
  `dev-dependencies`.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Protected file parses and resolves to `crate::commands/**` | Fail with file, line, and resolved target |
| Protected file cannot be parsed or read | Fail the architecture test; never silently skip it |
| Alias or relative path resolves to the forbidden layer | Fail exactly like a fully qualified path |
| Dependency resolves outside the forbidden layer | Pass without a violation |
| Web/TUI imports `commands::sync` | Fail ARCH-003 even if the sync layer itself stays clean |
| `commands` implements `SyncExecutor` or `Default for JobRegistry` | Fail ARCH-004 with the owned trait/type path |
| `commands` delegates to `crate::sync` or re-exports the compatibility name | Pass |

### 5. Good / Base / Bad Cases

- Good: Web/TUI call `JobRegistry::default()` or inject a sync-owned/test
  executor without importing commands.
- Base: a legitimate `crate::sync/**` dependency passes the fixture gate.
- Bad: `grep -r "use crate::commands" src/sync` passes while a fully qualified,
  aliased, or `super` path still reaches `commands`.
- Bad: moving the engine into `sync` but leaving `impl SyncExecutor for
  CommandSyncExecutor` or `impl Default for JobRegistry` in commands.

### 6. Tests Required

- Violation fixtures for `use`, fully qualified paths, imported aliases,
  nested modules, crate-root aliases, and relative `self`/`super` paths.
- One valid dependency-graph fixture that produces no violation.
- A live scan of `src/sync` asserting the violation list is empty.
- Negative fixtures for a non-command `commands::sync` dependency, a
  sync-owned trait impl in commands, a sync-owned type impl in commands, and
  an imported trait alias used by an impl.
- Live scans asserting non-command production code has no command-sync edge
  and commands has no sync-owned impl target.
- Every violation fixture must assert the resolved target and a nonzero source
  line; the live gate must print all violations, not only the first.

### 7. Wrong vs Correct

#### Wrong

```yaml
run: grep -r "use crate::commands" src/sync/
```

#### Correct

```yaml
run: cargo test --locked --all-features --test architecture_dependencies
```

For default composition:

#### Wrong

```rust
impl Default for crate::sync::JobRegistry {
    fn default() -> Self { Self::new(Arc::new(CommandSyncExecutor)) }
}
```

#### Correct

```rust
// src/sync/default.rs
impl Default for JobRegistry {
    fn default() -> Self { Self::new(Arc::new(DefaultSyncExecutor)) }
}
```
