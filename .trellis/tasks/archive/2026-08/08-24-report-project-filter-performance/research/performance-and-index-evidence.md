# Period Report Performance and Index Evidence

## Method

- Command: `cargo test --release --locked --all-features measure_project_filtered_period_bundle_performance -- --ignored --test-threads=1 --nocapture`
- Fixture sizes: 100,000 and 500,000 `usage_event` rows, with matching bucket truth.
- Seed and schema bootstrap completed before timing.
- Each measurement used five warm-ups followed by five sequential samples.
- p95 is nearest-rank; five samples therefore use the maximum sample.
- The before oracle is the former project-filtered unified shape: one complete
  event-object scan for overall plus a second complete event-object scan for
  per-source rows.
- The after unified path performs one project-resolution statement and one
  bucket aggregate. It runs no conversation query because `UnifiedReport` does
  not expose conversation counts.

## Results

| Rows | Before p50 / p95 | After p50 / p95 | p95 improvement | No-project p95 ratio |
| ---: | ---: | ---: | ---: | ---: |
| 100,000 | 502.704 / 515.416 ms | 2.563 / 3.121 ms | 99.4% | 0.638 |
| 500,000 | 2,514.656 / 2,578.005 ms | 1.701 / 2.356 ms | 99.9% | 0.656 |

All daily/weekly/monthly project-filtered unified samples returned one row and
stayed below 4 ms. The output was exactly equal to the event oracle before
timing. The amplified no-project path improved rather than regressed.

The exact daily conversation-count paths deliberately retain one narrow event
aggregate. Their synthetic 500k p95 values were:

- overall: 550.572 ms
- source: 573.779 ms
- host: 654.529 ms

These are not unified-path failures and do not materialize `EventRow`, token,
cost, model, prompt, or raw-record fields. They are retained as a documented
worst-case boundary, not relabelled as representative-data PASS.

## Statement and plan evidence

The non-ignored SQL trace test proves:

- daily/weekly/monthly unified bundle: one project resolver, one bucket totals
  aggregate, zero conversation aggregates;
- project-filtered daily overall: one project resolver, one bucket totals
  aggregate, one narrow conversation aggregate;
- conversation SQL contains no token, cost, or model projection.

The synthetic EQP used the bucket primary-key index for bucket aggregation.
The exact conversation count remained a `usage_event` scan plus temporary
group/distinct B-trees.

## Candidate index decision

A task-owned 500k fixture tested this covering candidate without adding a
migration:

```sql
CREATE INDEX idx_usage_event_report_project_candidate
ON usage_event(
    project_hash, event_at, source, host_id,
    session_id, source_path_hash, event_key
);
```

| Evidence | No index | Candidate |
| --- | ---: | ---: |
| Exact daily overall p95 | 526.204 ms | 408.167 ms |
| Database bytes | 361,906,176 | 417,251,328 |
| Size ratio | 1.000 | 1.153 |
| Index creation | n/a | 3,040.696 ms |

The candidate changed EQP to a covering project-hash search, but still missed
the 400 ms read budget and increased the database by 15.3%. It was therefore
rejected and removed before a sync-write benchmark; failing an earlier read/
size gate is already sufficient to block schema adoption. No migration, index,
or schema-version change remains in the implementation.

## Evidence boundary

- Representative temporary backup: **UNVERIFIED**. No explicit user-data copy
  or approval was available for this child task.
- Cold-cache/first-touch behavior: **UNVERIFIED**.
- Synthetic results do not substitute for the representative-data archive
  gate.
