# ccusage Component Guidelines

Here, "components" means terminal presentation units, not React components.

## Terminal Tables

Use `@ccusage/terminal/table` for report layout:

- `ResponsiveTable` handles compact mode based on `process.env.COLUMNS` or
  `process.stdout.columns`.
- `createUsageReportTable()`, `formatUsageDataRow()`, `pushBreakdownRows()`,
  and `addEmptySeparatorRow()` centralize repeated table behavior.
- Existing command modules such as `apps/ccusage/src/commands/monthly.ts` and
  `session.ts` build table config, push rows, then call `writeStdoutLine(table.toString())`.

Reference files:

- `ref/ccusage/packages/terminal/src/table.ts`
- `ref/ccusage/apps/ccusage/src/commands/monthly.ts`
- `ref/ccusage/apps/ccusage/src/commands/session.ts`

## JSON And Table Modes

- `--json` is the stable integration surface. In JSON mode, command modules set
  `logger.level = 0` and write formatted JSON to stdout.
- Table mode may use `logger.box()`, warnings, and compact-mode hints.
- Keep stdout for the requested report payload; do not mix progress logs into
  JSON output.

## Documentation Presentation

Docs live in `docs/` and are VitePress-based. `docs/CLAUDE.md` calls out
screenshots immediately after H1 and consistency for new pages.

## Avoid

- Do not use `console.log` from command or terminal code; use `logger` and
  `writeStdoutLine`.
- Do not duplicate table formatting logic in each command when
  `packages/terminal/src/table.ts` already owns it.
- Do not make JSON output depend on terminal width or colors.
