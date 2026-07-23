# Self-Update Contracts

## Scenario: Update From Official Source Branches

### 1. Scope / Trigger

Apply this contract when changing the `llmusage update` command, its supported
channels, confirmation flow, Cargo invocation, or self-update tests. This is an
application command, not part of the stable library facade, and it must not
depend on runtime database or integration state.

### 2. Signatures

Public CLI signature:

```text
llmusage update [-c|--check] [main|dev]
```

Internal command boundary:

```rust
pub enum UpdateChannel { Main, Dev }
pub fn run(channel: UpdateChannel, check_only: bool) -> anyhow::Result<()>;
```

`main` is the default positional value. Clap must expose only `main` and `dev`
as possible values.

### 3. Contracts

- Repository: `https://github.com/bahayonghang/llmuasage` (fixed).
- Cargo package: `llmusage` (fixed).
- Environment/config input: none. Do not accept a repository URL, branch name,
  tag, revision, or update source from environment variables or config files.
- Generated subprocess:

  ```text
  cargo install --git https://github.com/bahayonghang/llmuasage llmusage --branch <main|dev> --locked --force
  ```

- Pass each argument directly to `std::process::Command`; never use a shell
  command string. Inherit stdin/stdout/stderr so Cargo progress remains visible.
- `--check` output contains current version, repository, channel, and the exact
  command, then returns without reading stdin or starting Cargo.
- A real update shows the same plan, then accepts empty input, `y`, or `yes` as
  confirmation. `n` or `no` cancels successfully. Matching is case-insensitive.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Channel omitted | Select `main` |
| Channel is `dev` | Select `dev` |
| Any other channel/tag/revision | Clap parse error before side effects |
| `--check` | Success; executor call count remains zero |
| Confirmation is `n` / `no` | Success with cancellation message; executor call count remains zero |
| Confirmation is invalid | Print retry guidance and read again |
| Confirmation reaches EOF or I/O fails | Error; executor call count remains zero |
| Cargo cannot start | Error includes channel and copyable manual command |
| Cargo exits non-zero | Error includes exit code when available, channel, and manual command |
| Cargo succeeds | Success with `llmusage --version` verification guidance |

### 5. Good / Base / Bad Cases

- Good: `llmusage update dev --check` previews the fixed official `dev` command
  and performs no network or install work.
- Base: `llmusage update` previews `main`, accepts an empty confirmation line,
  streams Cargo output, and succeeds only when Cargo succeeds.
- Bad: `llmusage update feature-x` is rejected by Clap; it must not silently
  treat an arbitrary branch as an official update channel.
- Bad: non-interactive EOF must not use the prompt's visual default as consent.

### 6. Tests Required

- Clap parsing: assert omitted channel is `Main`, `dev` is `Dev`, and an
  arbitrary value reports `possible values: main, dev`.
- Help: assert default `main`, possible values, and `--check` are visible.
- Command plan: assert repository, package, channel, `--locked`, and `--force`
  argument positions without starting Cargo.
- Confirmation: assert empty/yes/no, retry, EOF, and injected read failure.
- Executor seam: assert `--check`, cancellation, EOF, and input failure make
  zero calls; assert success, startup failure, and non-zero exit propagation.
- Smoke only `update --help`, `update --check`, and `update dev --check`; never
  run a real self-install in automated project validation.

### 7. Wrong vs Correct

#### Wrong

```rust
Command::new("sh")
    .arg("-c")
    .arg(format!("cargo install --git {user_repo} --branch {user_branch} --force"));
```

This permits unsupported sources and introduces shell parsing/injection risk.

#### Correct

```rust
Command::new("cargo").args([
    "install", "--git", OFFICIAL_REPOSITORY, "llmusage",
    "--branch", channel.as_str(), "--locked", "--force",
]);
```

Keep the repository constant and channel typed. Tests inject the executor,
reader, and writer instead of modifying global `PATH` or installing a binary.
