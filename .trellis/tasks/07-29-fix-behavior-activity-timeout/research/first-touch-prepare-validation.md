# Activity first-touch prepare validation

Prepared at `2026-07-29T05:05:22Z` on the pre-reboot Windows boot.

## Manifest

- Path: `target/tmp/activity-first-touch-v3/manifest.json`
- Format: `llmusage.activity-first-touch.v3`, version 1
- Canonical manifest SHA-256 seal: valid
- Git HEAD: `6902f4460d9ef24c934cc7e6aa3a338396249869`
- Debug binary: llmusage 1.1.1, 30,724,096 bytes
- Debug binary SHA-256:
  `66f6eb899149f8dbc8669addcf13c6e97739310e82e99c5cfcfcc9d5ca32e1de`

## Snapshots

- Exactly five ordered samples: `sample-01` through `sample-05`.
- Each snapshot is 1,160,073,216 bytes; total temporary database storage is
  5,800,366,080 bytes.
- All five manifest-recorded snapshot hashes are identical:
  `bfbd7dc91b407a8571687772524b701326c525b2ff980986db408e9c3172bde4`.
- All five manifest-recorded database checks are identical: schema v18,
  `quick_check=ok`, 178,795 usage events, and 174,569 usage turns.
- Post-prepare stat-only verification matched every recorded size and mtime.
- No sample has a `first-touch-consumed.json` marker.

The snapshot content was not reopened, queried, hashed, copied, or scanned after
prepare. The checks above use the sealed manifest plus file stat metadata only.

## Source invariants

- Source size before/after/current: 1,160,073,216 bytes.
- Source mtime before/after/current: 1,785,290,678,109,236,300 ns since Unix epoch.
- The prepare process used SQLite online backup in read-only/query-only mode.

## Manual checkpoint

Restart Windows manually. Before opening or inspecting any snapshot, run:

```powershell
Set-Location 'D:\Documents\Code\CLI\llmusage'
python -B '.trellis\tasks\07-29-fix-behavior-activity-timeout\research\profile_activity_first_touch.py' run --manifest 'target\tmp\activity-first-touch-v3\manifest.json'
```

The runner must reject the prepare boot. It will validate the pinned binary and
snapshot stat only, then consume each sample once. Do not run it before restart.
