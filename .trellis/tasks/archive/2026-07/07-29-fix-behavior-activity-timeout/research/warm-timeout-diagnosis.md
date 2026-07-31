# Activity paired-warm timeout diagnosis

Captured: `2026-07-30`

## Classification

The paired second request in the reboot-gated run was not a fully warm request.
The 3-second hard deadline interrupted Activity before its two full-table
projections had both completed once. The first request warmed only part of the
`usage_event` projection; the second progressed farther and then encountered
the still-cold `usage_turn` projection. A third request completed below the
deadline after both projections had been read far enough into the Windows file
cache.

This is a **hard-timeout-induced incomplete warming** effect. It explains why
all five Step 1D pairs had a timed-out request followed by another timed-out
request without requiring overlapping orphan work. It does not change the
mechanical `NO-GO D1/D2` result and does not authorize an index, migration,
cache, or query rewrite.

## Evidence

### Existing Step 1D logs

All ten first-touch and paired requests had `semaphore_wait_ms=0`. After the
hard deadline, their supervised blocking tasks settled within 13-37 ms. No
SQLite busy/locked signal was present. The second request therefore started
only after the first task released its connection and permit; detached overlap
is excluded for these samples.

### Read-only phase profile

The existing representative Step 1A snapshot was opened with SQLite
`mode=ro` and `query_only=ON`. Only plan operators, row counts, and elapsed
times were printed.

| Projection | State | Rows | Elapsed ms | Plan |
| --- | --- | ---: | ---: | --- |
| event cost | first complete read | 178,795 | 4,982.40 | `SCAN usage_event` |
| event cost | repeated | 178,795 | 653.43 / 348.00 | `SCAN usage_event` |
| all-range turn | first complete read | 174,569 | 2,134.97 | `SCAN t` |
| all-range turn | repeated | 174,569 | 293.69 / 305.32 | `SCAN t` |

The first complete event projection alone exceeded the HTTP deadline. Once it
completed, the same statement was sub-second. The turn projection showed the
same first-complete-read penalty independently.

### Product-path staircase

Two fresh representative databases were created from the Step 1A snapshot
with `robocopy /J`, so the copy operation used unbuffered I/O. Each database
was served by the current debug binary. Requests were sequential, and every
timed-out request waited for its matching query-ID `orphan settled` event
before the next request.

| Copy | Attempt 1 | Attempt 2 | Attempt 3 | Attempt 4 | Attempt 5 |
| --- | ---: | ---: | ---: | ---: | ---: |
| A | timeout 3009 ms | timeout 3012 ms | normalized 1568 ms | normalized 1201 ms | normalized 1199 ms |
| B | timeout 3013 ms | timeout 3006 ms | normalized 1260 ms | normalized 897 ms | normalized 890 ms |

Both independent runs reproduced the exact two-timeout staircase. Permit wait
was zero and no SQLite busy/locked signal appeared in any request.

## Causal chain

1. Activity checks support, then streams the entire `usage_event` cost
   projection into a Rust `HashMap`.
2. It then streams the all-range `usage_turn` projection and performs the
   ordered Rust reducer.
3. On an uncached representative file, the first projection takes longer than
   the 3-second hard deadline. SQLite is interrupted before a complete pass.
4. The paired request is therefore only partially warm. It repeats the event
   scan and can reach cold turn pages before the same total deadline expires.
5. After two interrupted passes, enough pages are resident for the third
   request to finish in 1.26-1.57 seconds; subsequent requests remain below
   1.21 seconds.

The evidence classifies the timeout as cold file-cache amplification combined
with a multi-phase full-table read and hard interruption. It does not show a
query-plan regression: the plans and row counts match the Step 1A production
shape.

## Commands

The read-only phase profile used the existing
`profile_activity_baseline.py::profile_projections` helper against:

```text
target/tmp/activity-baseline-step1-run2/snapshot/llmusage.db
```

The product-path copies were created with:

```powershell
robocopy 'target\tmp\activity-baseline-step1-run2\snapshot' '<diagnostic-runtime>' 'llmusage.db' /J /R:0 /W:0 /NFL /NDL /NJH /NJS /NP
```

Each server was started through
`profile_activity_first_touch.py::start_server`; five
`GET /api/activity?range=all` requests were issued through
`activity_request`, and `wait_for_orphan_settled(query_id)` ran after every
cancelled response. Sanitized server logs are retained under
`target/tmp/activity-timeout-diagnosis/server-logs/`.

## Remaining uncertainty and next gate

- The phase profile reproduces the two individual cold-read penalties, but it
  does not identify which Windows storage component amplified the first read
  (filesystem cache miss, storage latency, or a filesystem filter such as
  antivirus). That distinction is not required to explain the HTTP sequence.
- The exact boundary varies with host state; the causal result is the repeated
  two-timeout-to-success transition, not a claim that every machine requires
  exactly three attempts.
- The original Step 1D pair gate remains mechanically incomplete because a
  request following an interrupted scan cannot be classified as fully warm.

The next gate is a user-approved revision of the acceptance protocol and
solution design. No production change is authorized by this diagnosis. D1 and
D2 remain blocked until that review explicitly selects a direction and defines
new evidence thresholds.
