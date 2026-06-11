# ccexplorer Error Handling

## Parsing And Aggregation

The parser is tolerant of bad input inside a larger scan:

- `process_session()` catches `json.JSONDecodeError` for malformed JSONL lines and keeps reading.
- `aggregate()` catches per-file exceptions, skips the bad path, and continues
  with remaining files.
- `_parse_day()` returns `None` for invalid timestamps instead of throwing into
  the command layer.

Reference files:

- `ref/ccexplorer/ccexplorer/data.py`
- `ref/ccexplorer/tests/test_data.py`

Keep this skip-and-continue behavior for user data. A single broken session file
should not prevent a report from being generated from valid files.

## CLI Boundary

`cli.py` is stricter about command setup and user-supplied paths:

- Missing project roots return a nonzero exit code and print an error to stderr.
- Empty but valid roots print a warning and still write a report.
- Build defaults are covered in `ref/ccexplorer/tests/test_cli.py`.

Reference file: `ref/ccexplorer/ccexplorer/cli.py`.

## Template Boundary

Template placeholder behavior is tested in `ref/ccexplorer/tests/test_web.py`.
If a placeholder is renamed in `templates/all.html`, update `web.py` and the
template tests in the same change.

## Avoid

- Do not let low-quality user JSONL crash the whole scan.
- Do not silently ignore a missing project root; that is a CLI input error.
- Do not catch broad exceptions around pricing or template logic without a test
  proving the intended fallback.
