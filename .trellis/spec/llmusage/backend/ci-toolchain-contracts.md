# CI And Toolchain Contracts

## 1. Scope / Trigger

Apply this contract when changing `Cargo.toml` dependencies or `rust-version`,
the pinned development toolchain, Rust CI commands, or subprocess integration
tests.

## 2. Signatures

- Shared Rust gate: `python scripts/ci-rust.py`
- Full local gate: `just ci`
- MSRV proof: `cargo +<rust-version> check --locked --all-features`

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

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Declared MSRV fails the locked all-features check | Block; raise `rust-version` or deliberately select compatible dependencies |
| Version immediately below declared MSRV passes | Lower the declaration and repeat the proof |
| Shared Rust gate fails locally or in one CI OS | Block; do not bypass that command in only one environment |
| Subprocess test reports an OS error | Locate the exact failing operation; do not label it an environment failure without context |

## 5. Good / Base / Bad Cases

- Good: the declared version passes from a clean target and the immediately
  lower candidate has a recorded dependency/compiler failure.
- Base: the pinned development toolchain passes the same shared Rust gate.
- Bad: `Cargo.toml` claims an old MSRV while CI silently tests a newer version,
  or local and CI gates use different argument sets.

## 6. Tests Required

- Run the MSRV proof with an isolated `CARGO_TARGET_DIR` after dependency
  updates.
- Run `python scripts/ci-rust.py` before committing Rust changes.
- Subprocess regression tests must assert the executable exists and attach
  spawn context; they must consume current public runtime paths/readers rather
  than stale compatibility fields.

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

## Scenario: AST-Enforced Layer Dependencies

### 1. Scope / Trigger

- Trigger: adding or changing a forbidden Rust dependency direction between
  application/domain layers and outer adapters.
- ARCH-002 currently forbids every `src/sync/** -> crate::commands/**`
  dependency, including aliases and relative paths.

### 2. Signatures

- Local and Actions gate:
  `cargo test --locked --all-features --test architecture_dependencies`
- Violation output: `<source-file>:<line> depends on forbidden target <path>`.

### 3. Contracts

- Parse every Rust file in the protected layer with `syn`; do not enforce the
  boundary with a grep for one spelling.
- Resolve `crate`, `self`, `super`, `use crate as <alias>`, and
  `extern crate self as <alias>` paths before comparing the dependency target.
- Concrete executors belong to outer adapters or composition roots. The sync
  layer owns only the stable executor trait and typed request/result contracts.
- Parser dependencies used only by the architecture test remain
  `dev-dependencies`.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Protected file parses and resolves to `crate::commands/**` | Fail with file, line, and resolved target |
| Protected file cannot be parsed or read | Fail the architecture test; never silently skip it |
| Alias or relative path resolves to the forbidden layer | Fail exactly like a fully qualified path |
| Dependency resolves outside the forbidden layer | Pass without a violation |

### 5. Good / Base / Bad Cases

- Good: Web/TUI composition roots inject `CommandSyncExecutor` into
  `JobRegistry::new`, while `src/sync` imports only the executor port.
- Base: a legitimate `crate::sync/**` dependency passes the fixture gate.
- Bad: `grep -r "use crate::commands" src/sync` passes while a fully qualified,
  aliased, or `super` path still reaches `commands`.

### 6. Tests Required

- Violation fixtures for `use`, fully qualified paths, imported aliases,
  nested modules, crate-root aliases, and relative `self`/`super` paths.
- One valid dependency-graph fixture that produces no violation.
- A live scan of `src/sync` asserting the violation list is empty.
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
