# TUI Subscription Quota Contracts

## 1. Scope / Trigger

Apply this contract when changing `src/subscription/`, the dash Usage quota
surface, or the Usage sync overlay.

## 2. Signatures

```text
FetchContext { endpoints, user_home, cache_path, timeout }
fetch_all(&ctx, bypass_cache) -> UsageFetchOutcome
UsageFetchOutcome { report, cache_hit }
UsageFetchReport { outputs, diagnostics }
```

## 3. Contracts

- Providers in this release: Claude, Codex, Grok Build, Kimi. A provider is
  requested only when local credentials exist under `user_home` (or documented
  env overrides).
- Credential files are read-only. Fetchers must not refresh tokens or rewrite
  Claude, Codex, Grok, or Kimi credential documents.
- Entering Usage or pressing `r` fetches quota. A 5-minute cache under
  `{LLMUSAGE_HOME}/cache/subscription-usage.json` may satisfy an entry fetch.
  `r` bypasses the cache. `R` does not poll quota APIs.
- Cache freshness is the JSON `fetched_at` field versus a 300-second TTL.
  File mtime is not a cache-hit signal.
- `UsageFetchOutcome.cache_hit` is true only when `cache::load` succeeds on the
  `bypass_cache=false` path. Live fetch and `bypass_cache=true` set
  `cache_hit=false`. Desktop `QuotaResponse.cache_hit` copies that field.
- `fetch_all` in v1.2.0 returned `UsageFetchReport`. The `UsageFetchOutcome`
  return type is a public breaking change versus baseline tag `v1.2.0`
  (`9b7a6f3dec12764222891c2d8f5aeb42db7bd490`). Recorded; crate version stays
  1.3.0 until a separate release approval. Do not add a compatibility wrapper.
- Applicable tools for cache provenance (`fetched_at`, `cache_hit`, no mtime
  signal): Claude Code, Codex, Grok Build, Kimi Code, OMP.
- Cache documents store `UsageFetchReport` only. They must not store access or
  refresh tokens.
- Tests inject `UsageEndpoints` to a local listener. CI must not contact public
  quota hosts.
- Emails default to `[hidden email]`. `m` toggles visibility.
- Source Sync remains in the `y` overlay. Web `sync_command_center` JSON is
  unchanged.

## 4. Validation

- Fetcher tests cover success, HTTP 4xx diagnostics, missing credentials, and
  unchanged credential bytes.
- Cache provenance tests cover expired `fetched_at` (live, `cache_hit=false`),
  corrupt JSON (not `cache_hit=true`), fresh `fetched_at` with old mtime
  (`cache_hit=true`, no endpoint call), and `bypass_cache=true` (live, false).
- TestBackend covers quota titles/columns, hidden email, overlay Source Sync,
  and NoColor.
