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
