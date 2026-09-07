# Isolated cargo-semver-checks install

Date: 2026-09-07

Installed only under the scratch tree. Did not change the user global
toolchain or `CARGO_HOME` config. Binary is not committed.

- Crate: `cargo-semver-checks v0.50.0`
- Executable: `cargo-semver-checks.exe`
- `--root`: `C:\Users\lyh\AppData\Local\Temp\grok-goal-2de7d21a862e\implementer\semver-workflow\tools`
- Host rustc/cargo from `rust-toolchain.toml`: cargo 1.97.0 (c980f4866 2026-06-30)
- Install command: `cargo install cargo-semver-checks --locked --root <scratch>/tools`
- Install exit: 0
- rustup note: default toolchain implicitly overridden by the repo
  `rust-toolchain.toml` (`1.97.0-x86_64-pc-windows-msvc`)
- Tool lockfile warning during install: `chacha20 v0.10.1` is yanked in
  crates.io; this is the tool's own lockfile, not this repo's `Cargo.lock`

`cargo semver-checks --help` (exit 0) lists `--baseline-rev <REV>` and
`--baseline-version <X.Y.Z>`. It does not list `--locked`.
