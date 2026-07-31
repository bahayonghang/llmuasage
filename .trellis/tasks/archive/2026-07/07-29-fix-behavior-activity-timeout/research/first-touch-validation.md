# Activity first-touch validation

Captured: `2026-07-30T02:28:50Z`

Each sample used a distinct manifest-listed SQLite snapshot and a fresh debug server. Server bootstrap was part of the product lifecycle before the first Activity request. The five files share one reboot-cleared cache event; user-mode code cannot prove that third-party software did not read them first.

State columns are `HTTP/support/degraded/timeout`; first-touch server timing is `wait/query/cancelled`. Permit wait is non-primary only when `wait < query` for every first-touch timeout sample.

| Sample | First-touch wall ms | First state | First server ms/state | Warm wall ms | Warm state | SQLite busy/locked | Cleanup |
| --- | ---: | --- | --- | ---: | --- | --- | --- |
| sample-01 | 3140.01 | 200/degraded/true/true | 0/3005/true | 3037.21 | 200/degraded/true/true | false | confirmed |
| sample-02 | 3009.43 | 200/degraded/true/true | 0/3006/true | 3023.93 | 200/degraded/true/true | false | confirmed |
| sample-03 | 3023.90 | 200/degraded/true/true | 0/3005/true | 3015.98 | 200/degraded/true/true | false | confirmed |
| sample-04 | 3025.45 | 200/degraded/true/true | 0/3008/true | 3043.36 | 200/degraded/true/true | false | confirmed |
| sample-05 | 3009.36 | 200/degraded/true/true | 0/3006/true | 3023.32 | 200/degraded/true/true | false | confirmed |

## Mechanical gate

First-touch median: `3023.90 ms`

- [x] Five first-touch `all` samples have median > 3000 ms
- [ ] All five paired warm `all` samples are < 3000 ms
- [x] Permit waiting is not primary and no SQLite busy/lock was observed
- [ ] Evidence is complete

Conclusion: `NO-GO D1/D2`
