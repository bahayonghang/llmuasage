# Bug Analysis: bootstrap-critical asset blocked by client filter

## 1. Root Cause Category

- **Category**: D/E - Test Coverage Gap and Implicit Assumption
- **Specific Cause**: `app.js` imported `/assets/data/fingerprint.js` at the top level. The helper only generated deterministic render cache keys, but the user Chrome profile's content filter matched the URL and returned `net::ERR_BLOCKED_BY_CLIENT`. ES module dependency resolution stopped before `app.js` could call `claim()`, so the dashboard never started even though the listener and API were healthy. Clean-browser validation implicitly assumed that a server-reachable asset URL was also client-loadable.

## 2. Why Earlier Fixes Did Not Resolve It

1. **Server lifecycle and query performance fixes**: these addressed confirmed listener supervision, finite errors, and progressive loading, but the blocked dependency prevented all application code from executing.
2. **Clean browser and direct HTTP checks**: the server correctly returned the old asset, so environments without the user's extension could not reproduce the client-side rejection.
3. **Bootstrap watchdog**: it converted permanent loading into a visible module-startup error, which exposed the failure class, but it could not make a blocked module graph execute.
4. **Discriminating check**: the user's Chrome network error named the exact blocked URL. Renaming only that URL to `render-key.js` made the same profile load it with HTTP 200 and eliminated `Network.loadingFailed`, while server/data behavior remained unchanged.

## 3. Prevention Mechanisms

| Priority | Mechanism | Specific Action | Status |
| --- | --- | --- | --- |
| P0 | Architecture | Use client-filter-safe, domain-neutral URLs for bootstrap-critical embedded modules | DONE |
| P0 | Test coverage | Assert new route 200, old route 404, and no `fingerprint.js` in the live module graph | DONE |
| P0 | Real-environment check | Re-run in the same user Chrome profile when the error is `ERR_BLOCKED_BY_CLIENT` | DONE |
| P1 | Documentation | Record the asset URL contract in `web-server-contracts.md` | DONE |
| P1 | Diagnosis | Separate browser client failures from listener, API, and SQLite timing evidence | DONE |

## 4. Systematic Expansion

- **Similar Issues**: other top-level asset URLs can fail for keyword-based client policies even when their bodies are benign. Expansion is limited to browser-visible bootstrap dependencies; there is no evidence requiring broad internal symbol renames.
- **Design Improvement**: treat embedded route names as a client compatibility boundary, not only as a server registry detail.
- **Process Improvement**: when a clean browser passes but the user's browser fails, inspect `Network.loadingFailed` and the exact request URL before reopening database/query optimization.
- **Evidence update**: the root-cause confidence moved from multiple plausible service/data/module causes to high confidence after the exact client error and same-profile rename verification discriminated the URL-filter hypothesis.

## 5. Knowledge Capture

- [x] Added PRD requirement and acceptance evidence.
- [x] Added the design decision and implementation/validation record.
- [x] Added executable route, failure-matrix, test, and wrong/correct contracts to the backend Web spec.
- [x] Added focused Rust/Node regressions and same-profile browser proof.
- [x] Confirmed this repository has no `src/templates/markdown/spec/` mirror to sync.
- [ ] Commit remains intentionally deferred because the current user boundary forbids commits.
