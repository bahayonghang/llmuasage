# Dashboard Runtime And Logs Evidence

## Repository Findings

- `src/web/shell.rs:174-187` orders sidebar links as `#cost`, `#status`, `#logs`.
- `src/web/shell.rs:590-597` places the event-log section before the cost section.
- `src/web/shell.rs:609-646` nests `#status` inside the cost section's `.cost-status-grid`; the status node is not a top-level section and has no own section heading.
- `src/web/assets/app.js:764` observes sections in `logs`, `cost`, `status` order, which differs from the sidebar order.
- `src/web/assets/components.css:1222-1226` defines three status diagnostic columns although the shell contains only two `.subpanel-section` children.
- `src/web/assets/data/fetch.js:365-373` sends `page_size=50` for every logs page.
- `src/web/assets/render/logs-viewer.js` already owns generation/signature fencing, cursor append, session reset, snapshot empty state, and event-key raw detail fetch. The layout fix must preserve those behaviors.
- The Dashboard performance contract requires logs to retain session filtering, single-event detail precedence, generation/filter-signature fencing, reset-before-replacement behavior, and old-snapshot compatibility.

## Live Browser Findings

Observed at `http://127.0.0.1:37421/` on 2026-08-28:

- The page loads successfully with no browser console warning/error.
- Clicking “运行状态” sets `#status`, but the target is a 1245px-tall child of `.cost-status-grid`; its top-level parent section is `#cost` and the visible heading is “成本估算”.
- At 1280×720, the cost summary row is 196px high, the cost ranking is 318px high, and the adjacent status panel is 1245px high. The shared row therefore leaves about 927px of empty area below the cost ranking.
- The three-column status template gives each of the two real subpanels only about 126px at this viewport and leaves a third grid track unused.
- The first logs request renders 50 event rows plus 50 hidden detail rows. `#logs` is about 3897px high and its table about 3699px high.
- The observed session label is 64 characters. The current table has horizontal overflow but no bounded vertical viewport or sticky header.

## Preserved Contracts

- No database, DTO, cursor encoding, public route, raw-retention, or Rust page-size default change.
- `event_key` detail mode remains zero-or-one record, ignores pagination, and requests raw JSON only for that record.
- Global filter/session changes continue to discard stale responses through generation plus signature checks.
- Current sync health remains owned by `sync_command_center`; historical failures remain diagnostic-only.
