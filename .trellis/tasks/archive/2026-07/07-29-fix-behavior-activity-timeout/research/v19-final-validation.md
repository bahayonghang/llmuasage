# Activity v19 final validation

The final binary was `llmusage 1.1.1` with SHA-256
`c985be4d8e92d8f2369aecdcfb070537de8ca3ed980aae2fe94095d9eb272fcd`.
The reboot-gated samples were captured at `2026-07-31T01:18:56Z`; the
representative follow-up matrix was captured at `2026-07-31T01:22:31Z`.

## Reboot-cleared `all` first touch

All five manifest-listed v19 snapshots completed their first Activity request
with HTTP 200, `normalized` support, no timeout, no degradation, zero permit
wait, and no SQLite busy/locked signal. Every server exited and every allocated
port was released.

| Metric | Result | Contract |
| --- | ---: | ---: |
| First-touch median | `2572.02 ms` | `<3000 ms` |
| First-touch range | `2510.26-2827.10 ms` | every sample `<3000 ms` |
| Paired warm range | `778.18-947.89 ms` | every sample `<3000 ms` |

The generated first-touch report retains the historical R2 causal-gate label
`NO-GO D1/D2` because its mechanical GO condition requires the cold median to
exceed three seconds. That label means the optimized v19 samples no longer
justify entering D1 or D2; it is not the production v19 acceptance decision.

## Representative `1d/all` matrix

The existing sanitized matrix harness used a consumed v19 snapshot, fresh
database copies, and fresh servers. These samples are not presented as
reboot-cleared I/O evidence.

| Temperature | Range/load | Samples | Median | Range | HTTP/timeouts | Support |
| --- | --- | ---: | ---: | ---: | --- | --- |
| Copy-backed fresh server | `1d` solo | 5 | `29.53 ms` | `24.70-59.33 ms` | `200 / 0` | `no_data` |
| Warm | `1d` solo | 5 | `15.45 ms` | `3.52-27.34 ms` | `200 / 0` | `no_data` |
| Copy-backed fresh server | `all` solo | 5 | `640.45 ms` | `622.58-654.77 ms` | `200 / 0` | `normalized` |
| Warm | `all` solo | 5 | `653.77 ms` | `634.98-829.47 ms` | `200 / 0` | `normalized` |
| Copy-backed fresh server | `1d` concurrency-2 | 10 | `14.07 ms` | `13.45-31.07 ms` | `200 / 0` | `no_data` |
| Warm | `1d` concurrency-2 | 10 | `14.76 ms` | `2.57-27.92 ms` | `200 / 0` | `no_data` |
| Copy-backed fresh server | `all` concurrency-2 | 10 | `664.90 ms` | `651.32-684.69 ms` | `200 / 0` | `normalized` |
| Warm | `all` concurrency-2 | 10 | `795.97 ms` | `675.79-878.61 ms` | `200 / 0` | `normalized` |

The database has no normalized behavior facts in the current `1d` window, so
the explicit `no_data` support state is expected. The baseline harness's
legacy `degraded` helper treats any `supported=false` state as degraded; the
acceptance decision therefore uses the persisted HTTP, support, and timeout
fields. No `1d` request timed out.

## Browser and cleanup

A real Chromium session loaded the dashboard against a fresh v19 server. The
Activity card settled to the explicit `no_data` state. Rendered DOM contained
no `dashboard query exceeded`, timeout, loading, or equivalent Chinese text.
The browser closed, port `43191` was released, and the final process check found
zero `llmusage` processes.

Byte-for-byte Activity equivalence, schema v18-to-v19 and fresh bootstrap,
covering-index plans, and the explicit seven-round sync benchmark are recorded
in `production-d1-validation.md`. The indexed/baseline sync median ratio was
`1.030314`, below the `1.10` blocking threshold. `python scripts/ci-rust.py`,
`just ci`, the Python harness tests, Ruff, and Pyright passed before the sealed
binary was prepared.

## Decision

`PASS`: the final v19 binary meets the Activity `1d` median, every-sample `all`
deadline, exactness, write-regression, browser-settlement, and cleanup gates.
D2 remains out of scope.
