# Layout verification (2026-08-12)

## Root causes

| Area | Cause | Fix |
|------|-------|-----|
| Hero right blank | `.hero { grid-template-columns: minmax(0, 640px) 360px }` does not fill wide main | `minmax(0, 1fr) minmax(280px, 360px)` |
| Top sessions right blank | `#top-sessions` not `wide` between two full-width widgets | add `wide` class in `shell.rs` |
| Status card empty slot | `.status-grid` 3 columns / 2 cells | `repeat(2, ...)` |

## Evidence

### Unit

```text
cargo test --lib web::tests::overview_wide_layout_avoids_orphan_blank_columns
ok
```

### Live server contract probe

Served assets from `http://127.0.0.1:37421` matched the three layout contracts (hero columns, top-sessions wide, status-grid 2 cols).

### Playwright geometry @ 1600×1100

```json
{
  "status.right": 1552,
  "hero.right": 1552,
  "topSessions.width": 1256,
  "readyGrid.width": 1256,
  "statusGridCols": "154px 154px",
  "cellCount": 2,
  "heroCols": "868px 360px"
}
```

No right-side orphan gap (`main.right - status.right` within padding). Breakpoints 1080 / 720 / 390 did not collapse layout.

Screenshots were captured under `/tmp/llmusage-layout-verify/` during verification and are intentionally not committed (geometry numbers above are the durable evidence).
