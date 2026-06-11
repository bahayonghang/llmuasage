# ccexplorer Logging Guidelines

The package uses simple CLI-oriented stderr output. There is no logging
framework.

## Progress And Warnings

Follow the existing pattern:

- `aggregate(..., verbose=True)` prints scan progress to `sys.stderr`.
- `cmd_build()` prints missing-root errors and no-data warnings to `sys.stderr`.
- `--quiet` passes `verbose=False` into aggregation so progress output is suppressed.
- HTML content is written to the output file, not streamed through logs.

Reference files:

- `ref/ccexplorer/ccexplorer/cli.py`
- `ref/ccexplorer/ccexplorer/data.py`
- `ref/ccexplorer/tests/test_cli.py`

## When Adding Output

- Put user-facing command messages at the CLI boundary.
- Keep parser internals quiet except for existing verbose progress.
- Keep stdout clean for future machine-readable command output; use stderr for
  progress and warnings.

## Avoid

- Do not add `logging` configuration or structured log dependencies without a
  concrete CLI need.
- Do not print from `pricing.py` or `web.py`; those modules should stay pure
  enough to test by return values.
- Do not bypass `--quiet` for scan progress.
