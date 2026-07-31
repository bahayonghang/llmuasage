# Activity first-touch runbook

All commands below run from `D:\Documents\Code\CLI\llmusage` in PowerShell.
The harness never restarts Windows and never evicts the global file cache.

## Before prepare

Confirm that no task-owned `llmusage serve` or `llmusage sync` process is running:

```powershell
Set-Location 'D:\Documents\Code\CLI\llmusage'
Get-CimInstance Win32_Process | Where-Object {
    $_.Name -eq 'llmusage.exe' -and
    ($_.CommandLine -match '\bserve\b' -or $_.CommandLine -match '\bsync\b')
} | Select-Object ProcessId, Name, CommandLine
```

Build the exact debug binary that the manifest will pin:

```powershell
Set-Location 'D:\Documents\Code\CLI\llmusage'
cargo build
```

## Prepare

This command creates exactly five snapshots under
`D:\Documents\Code\CLI\llmusage\target\tmp\activity-first-touch-v3\sample-01..05`.
It reads the live database with SQLite online backup and does not start a server.
Expect about 5.8 GB of temporary disk use.

```powershell
Set-Location 'D:\Documents\Code\CLI\llmusage'
$sourceDb = Join-Path $env:USERPROFILE '.llmusage\llmusage.db'
python -B '.trellis\tasks\07-29-fix-behavior-activity-timeout\research\profile_activity_first_touch.py' prepare --source-db $sourceDb --binary 'D:\Documents\Code\CLI\llmusage\target\debug\llmusage.exe'
```

The command must report this manifest and a post-reboot command:

```text
D:\Documents\Code\CLI\llmusage\target\tmp\activity-first-touch-v3\manifest.json
```

After `prepare` succeeds, do not open, hash, check, copy, scan, or run a server
against any of the five snapshots. At this point, stop and restart Windows
manually. Do not run the post-reboot command before the restart.

## After the manual restart

Run this command before opening or inspecting any snapshot. The runner first
proves that the Windows boot identity changed, validates the manifest and binary,
and checks snapshot size/mtime only. It then consumes each snapshot once.

```powershell
Set-Location 'D:\Documents\Code\CLI\llmusage'
python -B '.trellis\tasks\07-29-fix-behavior-activity-timeout\research\profile_activity_first_touch.py' run --manifest 'D:\Documents\Code\CLI\llmusage\target\tmp\activity-first-touch-v3\manifest.json'
```

The sanitized output is written only after all five samples finish:

```text
D:\Documents\Code\CLI\llmusage\.trellis\tasks\07-29-fix-behavior-activity-timeout\research\first-touch-results.json
D:\Documents\Code\CLI\llmusage\.trellis\tasks\07-29-fix-behavior-activity-timeout\research\first-touch-validation.md
```

Server stdout/stderr remains under this ignored task directory:

```text
D:\Documents\Code\CLI\llmusage\target\tmp\activity-first-touch-v3\server-logs
```

Do not rerun or remove anything after a failed sample. A sample with
`first-touch-consumed.json` has entered server bootstrap and cannot be reused as
a cold sample. Review the failure and unconsumed sample state before deciding on
a new evidence run. Do not start D1 or D2 until the generated conclusion and the
user checkpoint both say `GO D1`.
