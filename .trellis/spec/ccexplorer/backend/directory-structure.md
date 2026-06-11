# ccexplorer Backend Directory Structure

## Package Shape

Keep runtime code inside `ref/ccexplorer/ccexplorer/`:

- `data.py` reads `~/.claude/projects/*/*.jsonl`, derives project/session/day
  values, and folds records into `AggregatedRow` objects.
- `pricing.py` owns the static Claude model family price table and cost math.
- `cli.py` owns `argparse` command setup, CLI defaults, exit codes, and optional browser opening.
- `web.py` owns report rendering and writes the output HTML.
- `templates/all.html` is the only browser asset; `web.py` embeds rows by replacing placeholders.

Tests live in `ref/ccexplorer/tests/` and mirror the runtime modules:

- `test_data.py` for slug parsing, session processing, aggregation, and unreadable files.
- `test_pricing.py` for model families and token bucket cost math.
- `test_cli.py` for argparse defaults and end-to-end build behavior against synthetic JSONL.
- `test_web.py` for placeholder replacement, totals, empty rows, and file writing.

## Local Rules

- Put new ingestion or aggregation behavior in `data.py`; do not make `cli.py`
  parse JSONL internals.
- Put new pricing buckets or fallback behavior in `pricing.py`; tests should
  prove both known and unknown model handling.
- Put report-template changes in `templates/all.html` and keep `web.py` as the
  narrow substitution layer.
- Keep runtime dependencies empty unless the project intentionally stops being
  standard-library-only. `pyproject.toml` currently has `dependencies = []`.

## Avoid

- Do not add a web framework, ORM, or background service for this package.
- Do not scatter output formatting across parser/pricing modules.
- Do not add tests outside `tests/` unless the package layout changes first.
