# Design: dashboard/report façade

## Approach

`QueryFilter` remains the SQL filter. `ReportFilter` becomes a thin wrapper:

```text
ReportOptions { filter: QueryFilter, order, locale, breakdown, project label, blocks options }
```

SQL generation goes only through `QueryFilter::{event,bucket,turn,tool}_filter`.

`load_*_report(conn, options)` / `load_*_report(dashboard, options)`. CLI opens one `Dashboard` (or one connection) per command.

`Dashboard::blocks_report` calls `reports::load_blocks_report_with_conn(&self.conn, ...)`.

## Ordering

Land after `store-query-decouple` if both dirty `src/query/`. Land after `query-sql-performance` if both dirty `reports.rs`.

## Compatibility

CLI JSON DTOs unchanged (`report-cli-contracts.md`).
