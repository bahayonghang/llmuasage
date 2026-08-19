# TUI Subscription Quota Contracts

## 1. Scope / Trigger

Apply this contract when changing `src/subscription/`, the dash Usage quota
surface, or the Usage sync overlay.

## 2. Signatures

```text
FetchContext { endpoints, user_home, cache_path, timeout }
fetch_all(&ctx, bypass_cache) -> UsageFetchReport
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
- TestBackend covers quota titles/columns, hidden email, overlay Source Sync,
  and NoColor.
