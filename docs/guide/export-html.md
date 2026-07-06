# Export HTML

Use `export html` when you need a portable, offline dashboard snapshot.

## Export to a directory

```powershell
llmusage export html --out .\llmusage-report
```

If `--out` is omitted, llmusage writes under the runtime export area.

## Output files

The export directory contains:

- `index.html`
- `snapshot.json`
- `assets/*`

The bundle reuses the same dashboard shell as `llmusage serve`, but it loads from `snapshot.json` instead of live HTTP endpoints. `snapshot.json` includes the fixed dashboard sections and the default Cost Explorer payload.

## Snapshot behavior

Static exports keep the captured filters and data. Live-only controls such as sync jobs, auto-refresh, and custom Explorer reruns are disabled with an explanation.

## Suggested flow

```powershell
llmusage sync
llmusage export html --out .\llmusage-report
```

Run a fresh sync first when you want the exported snapshot to include the latest local artifacts.
