# Semver record: `fetch_all` return type

Baseline: `cargo semver-checks --baseline-rev v1.2.0`
(`git rev-parse v1.2.0^{commit}` = `9b7a6f3dec12764222891c2d8f5aeb42db7bd490`).

v1.2.0 signature:

```rust
pub async fn fetch_all(ctx: &FetchContext, bypass_cache: bool) -> UsageFetchReport
```

This task:

```rust
pub async fn fetch_all(ctx: &FetchContext, bypass_cache: bool) -> UsageFetchOutcome
```

`UsageFetchOutcome { report, cache_hit }` is a new public struct. This is a
public breaking change. Pre-existing v1.2.0 → 1.3.0 diffs in
`research/semver-checks.log` are unrelated. Crate version stays 1.3.0. No
compatibility wrapper. Release/version bump needs separate approval.
