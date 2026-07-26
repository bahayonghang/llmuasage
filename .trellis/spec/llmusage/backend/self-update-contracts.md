# Self-Update Contracts

## Scenario: Update From Resolved Official Targets

### 1. Scope / Trigger

Apply this contract when changing the `llmusage update` command, its supported
channels, remote-ref resolution, confirmation flow, Cargo invocation, or
self-update tests. This is an application command, not part of the stable
library facade, and it must not depend on runtime database or integration state.

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

`main` is the default positional value and means the latest stable release,
not the current `main` branch head. Clap must expose only `main` and `dev` as
possible values.

### 3. Contracts

- Repository: `https://github.com/bahayonghang/llmuasage` (fixed).
- Cargo package: `llmusage` (fixed).
- Environment/config input: none. Do not accept a repository URL, branch name,
  tag, revision, or update source from environment variables or config files.
- Resolution uses direct `git ls-remote` arguments against the official
  repository. It must not invoke a shell.
- Stable resolution considers only complete three-part semantic-version tags
  whose numeric components are canonical (no leading zero unless the component
  is `0`) and fit the supported integer range. It excludes prerelease/build
  tags, selects the highest version, and resolves an annotated tag to its
  peeled commit or a lightweight tag to its direct commit.
- Every accepted object id is a complete 40- or 64-character hexadecimal SHA.
  Missing, malformed, ambiguous, or inconsistent refs fail closed.
- Stable install is pinned to the resolved immutable commit:

  ```text
  cargo install --git https://github.com/bahayonghang/llmuasage llmusage --rev <resolved-sha> --locked --force
  ```

- Dev resolves and displays the current official `dev` commit, warns that the
  branch remains mutable and is not a verified stable release, then installs:

  ```text
  cargo install --git https://github.com/bahayonghang/llmuasage llmusage --branch dev --locked --force
  ```

- Pass each argument directly to `std::process::Command`. Inherit
  stdin/stdout/stderr for Cargo so build progress remains visible.
- Preview and `--check` show the current version, repository, channel, resolved
  commit, stable tag when present, and exact Cargo command.
- `--check` performs remote ref resolution but never reads confirmation input
  or starts Cargo.
- A real update accepts empty input, `y`, or `yes` as confirmation. `n` or `no`
  cancels successfully. Matching is case-insensitive.
- After confirmation, resolve the target again and require an exact match with
  the previewed target before executing the original plan. Never silently
  follow a moved tag or branch.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Channel omitted | Select stable `main` semantics |
| Channel is `dev` | Select mutable official `dev` semantics |
| Any other channel/tag/revision | Clap parse error before side effects |
| Stable tag is annotated | Use its complete peeled commit SHA |
| Stable tag is lightweight | Use its complete direct commit SHA |
| Stable refs are missing, malformed, ambiguous, or prerelease-only | Error before confirmation or Cargo |
| Dev ref is missing, malformed, or inconsistent | Error before confirmation or Cargo |
| `--check` | Resolve once; executor call count remains zero |
| Confirmation is `n` / `no` | Success with cancellation message; no second resolution or executor call |
| Confirmation is invalid | Print retry guidance and read again |
| Confirmation reaches EOF or I/O fails | Error; executor call count remains zero |
| Target differs after confirmation | Error; executor call count remains zero |
| Ref lookup fails before or after confirmation | Error; executor call count remains zero |
| Cargo cannot start | Error includes channel and copyable manual command |
| Cargo exits non-zero | Error includes exit code when available, channel, and manual command |
| Cargo succeeds | Success with `llmusage --version` verification guidance |

### 5. Good / Base / Bad Cases

- Good: `llmusage update --check` resolves the latest stable release tag and
  commit, previews an immutable `--rev` command, and never starts Cargo.
- Good: an annotated stable tag uses the peeled commit rather than the tag
  object id; a lightweight stable tag uses its direct commit.
- Base: `llmusage update` previews the stable tag and commit, accepts an empty
  confirmation line, revalidates the target, and succeeds only when Cargo does.
- Base: `llmusage update dev --check` shows the current dev SHA, mutable warning,
  and branch install command without starting Cargo.
- Bad: `llmusage update feature-x` is rejected by Clap; it must not silently
  treat an arbitrary branch as an official update channel.
- Bad: a moved tag/branch, failed lookup, short SHA, conflicting ref, or
  non-interactive EOF must never fall back to branch installation or consent.

### 6. Tests Required

- Clap parsing: assert omitted channel is `Main`, `dev` is `Dev`, and an
  arbitrary value reports `possible values: main, dev`.
- Help: assert default `main`, possible values, and `--check` are visible.
- Stable resolver/planner: cover version ordering, annotated and lightweight
  tags, prerelease/build exclusion, noncanonical or overflowing numeric
  components, invalid/short SHA, missing tag refs, and inconsistent targets.
  Assert stable argv contains the displayed `--rev` SHA and never
  `--branch main`.
- Dev resolver/planner: assert the current SHA and mutable warning are displayed
  while argv remains `--branch dev`.
- Confirmation: assert empty/yes/no, retry, EOF, and injected read failure.
- Executor seam: assert `--check`, cancellation, EOF, resolution failure,
  target movement, and input failure make zero calls; assert success, startup
  failure, and non-zero exit propagation.
- Inject reader, writer, ref provider, and process executor in unit tests. Tests
  must not access the network or run a real self-install.

### 7. Wrong vs Correct

#### Wrong

```rust
Command::new("cargo").args([
    "install", "--git", OFFICIAL_REPOSITORY, "llmusage",
    "--branch", "main", "--locked", "--force",
]);
```

This installs a mutable branch head while presenting it as a stable release.

#### Correct

```rust
Command::new("cargo").args([
    "install", "--git", OFFICIAL_REPOSITORY, "llmusage",
    "--rev", resolved_release.commit_sha(), "--locked", "--force",
]);
```

Resolve only official refs, show the target before confirmation, revalidate it
after confirmation, and execute the already-previewed immutable stable plan.
