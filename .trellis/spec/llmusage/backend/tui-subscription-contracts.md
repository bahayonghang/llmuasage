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
  (`9b7a6f3dec12764222891c2d8f5aeb42db7bd490`). The user approved the 2.0.0
  development version boundary on 2026-09-28. Downstream callers use `.report`
  and `cache_hit` as documented in `docs/reference/migration-v2.md`. Do not add
  a compatibility wrapper. Version preparation does not authorize publishing.
- Applicable tools for cache provenance (`fetched_at`, `cache_hit`, no mtime
  signal): Claude Code, Codex, Grok Build, Kimi Code, OMP.
- Public `UsageFetchReport`, output/metric/diagnostic structs and
  `UsageFetchOutcome` retain their existing fields, values and signatures.
  Private `fetch_all_with_presentation` shares the same core and returns the
  original outcome plus neutral `QuotaPresentationHints` for TUI rendering.
- Private cache documents may add optional `presentation` metadata beside
  `fetched_at` and the unchanged `report`. This is an explicit private-format
  compatibility extension: old readers ignore it and old caches still load.
  Bind hints to the exact report fingerprint; malformed or mismatched hints
  are discarded independently of an otherwise valid fresh cache report.
  TTL, cache-hit provenance and endpoint call counts remain unchanged.
- Hints are emitted at original provider/HTTP decisions and contain stable
  diagnostic/window/unit identities and original remaining/limit quantities.
  Never recover facts by parsing English text or infer absolute quantities
  from percentages. Do not store locale, translated templates, credentials,
  tokens or response bodies in hints/cache metadata.
- TUI holds hints in its private session, accepts them with the same quota
  report/request tag and uses a fingerprint-checked frame scope. No required
  public AppState field or global last-hint buffer is added. The private quota
  request tag is a new stale-result association guard; request scheduling,
  cancellation and r/R policy retain their prior meaning.
- Tests inject `UsageEndpoints` to a local listener. CI must not contact public
  quota hosts.
- Emails default to the localized hidden-email label. `m` toggles visibility;
  original email/account data is unchanged.
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
- Pair live/fresh-cache rendering and preserve report numbers, exact absolute
  remaining/limit, TTL/cache_hit and request counts. Cover old/new readers,
  missing/malformed/mismatched hints, unchanged credential bytes, superseded
  requests and r/R/m. Headless tests do not establish native terminal PASS.
