# Activity three-copy confirmation

Captured: `2026-07-30T03:17:27Z`

Each copy came from the retained Step 1A snapshot via `robocopy /J`, used a fresh current debug server, and issued at most five sequential `range=all` requests. Every timeout/cancelled request waited for its matching query-ID orphan-settled event. Only sanitized timing and state data was retained.

| Copy | Sequential attempts | Copy pass | SQLite busy/locked | Cleanup |
| --- | --- | --- | --- | --- |
| copy-01 | 1=3044.30ms/degraded/timeout=true; 2=3028.38ms/degraded/timeout=true; 3=1622.74ms/normalized/timeout=false; 4=977.63ms/normalized/timeout=false | True | False | True |
| copy-02 | 1=3033.43ms/degraded/timeout=true; 2=3030.08ms/degraded/timeout=true; 3=1595.34ms/normalized/timeout=false; 4=940.09ms/normalized/timeout=false | True | False | True |
| copy-03 | 1=3024.66ms/degraded/timeout=true; 2=3014.64ms/degraded/timeout=true; 3=1662.35ms/normalized/timeout=false; 4=941.47ms/normalized/timeout=false | True | False | True |

## Mechanical gate

- [x] All three copies pass
- [x] Every permit wait is zero
- [x] No SQLite busy/locked signal
- [x] Process, port, database, sidecar, and raw-log cleanup succeeded

Conclusion: `GO D1`

## Command

```powershell
python -B '.trellis/tasks/07-29-fix-behavior-activity-timeout/research/profile_activity_confirmation.py' confirm --snapshot 'target/tmp/activity-baseline-step1-run2/snapshot/llmusage.db' --binary 'target/debug/llmusage.exe' --work-dir 'target/tmp/activity-timeout-confirmation' --output '.trellis/tasks/07-29-fix-behavior-activity-timeout/research/confirmation-results.json' --validation '.trellis/tasks/07-29-fix-behavior-activity-timeout/research/confirmation-validation.md'
```

The five consumed `target/tmp/activity-first-touch-v3/sample-*` databases were
not used by the command. Their file metadata was checked before and after the
run without opening their contents.
