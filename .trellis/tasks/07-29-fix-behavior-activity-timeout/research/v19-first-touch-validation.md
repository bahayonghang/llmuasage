# Activity first-touch validation

Captured: `2026-07-31T01:18:56Z`

Each sample used a distinct manifest-listed SQLite snapshot and a fresh debug server. Server bootstrap was part of the product lifecycle before the first Activity request. The five files share one reboot-cleared cache event; user-mode code cannot prove that third-party software did not read them first.

State columns are `HTTP/support/degraded/timeout`; first-touch server timing is `wait/query/cancelled`. Permit wait is non-primary only when `wait < query` for every first-touch timeout sample.

| Sample | First-touch wall ms | First state | First server ms/state | Warm wall ms | Warm state | SQLite busy/locked | Cleanup |
| --- | ---: | --- | --- | ---: | --- | --- | --- |
| sample-01 | 2675.67 | 200/normalized/false/false | 0/2596/false | 857.29 | 200/normalized/false/false | false | confirmed |
| sample-02 | 2566.04 | 200/normalized/false/false | 0/2548/false | 806.49 | 200/normalized/false/false | false | confirmed |
| sample-03 | 2510.26 | 200/normalized/false/false | 0/2492/false | 817.34 | 200/normalized/false/false | false | confirmed |
| sample-04 | 2827.10 | 200/normalized/false/false | 0/2824/false | 947.89 | 200/normalized/false/false | false | confirmed |
| sample-05 | 2572.02 | 200/normalized/false/false | 0/2569/false | 778.18 | 200/normalized/false/false | false | confirmed |

## Mechanical gate

First-touch median: `2572.02 ms`

- [ ] Five first-touch `all` samples have median > 3000 ms
- [x] All five paired warm `all` samples are < 3000 ms
- [x] Permit waiting is not primary and no SQLite busy/lock was observed
- [x] Evidence is complete

Conclusion: `NO-GO D1/D2`
