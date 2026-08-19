# Design: sync command center last-run headline

## Behavior gap

Today `sync_command_center_with_diagnostics` reads the last 10 `run_log` rows of any command, counts sync-family `counts_as_failure` (including recovered `aborted`) as `recent_failures`, and uses that count for the failed headline. `reason_key` independently prefers rebuild risk. A later successful sync plus Claude missing files therefore renders failed title + rebuild-risk body + success last-run.

## Where it lives

Owner: `Dashboard::sync_command_center_with_diagnostics` in `src/query/mod.rs`.

Supporting read: `RunLog` needs a usage-import window (`sync`, `sync --rebuild`, `hook-run`) instead of a mixed-command `LIMIT 10`.

Not owners: `RunRecord::counts_as_failure`, doctor, health/diagnostics failure lists, rebuild guard, JS job overlay (`centerWithJobOverlay`).

## Data flow

```
run_log (usage-import rows)
  -> last_run = newest usage-import row
  -> last_run_failed = last_run.status == "failed"
  -> recent_failures = count of status == "failed" in last 10 usage-import rows
source_file Path::exists + events
  -> lossy_rebuild_risk
headline/reason:
  busy lock -> busy / existing reason pairing
  else last_run_failed -> failed / lastRunFailed
  else lossy_rebuild_risk -> rebuildRisk / rebuildRisk
  else empty statuses -> ready / empty
  else ready / ready
live JS overlay still replaces keys for running / failed job / cancelled
```

## Compatibility

- Payload field names unchanged.
- `safety.recent_failures` remains a count. Its meaning for the command center becomes failed usage-import rows, not aborted/serve rows.
- `last_run.error_key` is set only for `status == "failed"`.
- Doctor still warns on recovered aborted runs.

## Rollback

Revert the query/run_log/test/spec edits. No schema migration.
