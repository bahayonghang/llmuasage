# ccexplorer Quality Guidelines

## Tooling

`pyproject.toml` defines the local quality stack:

- Python `>=3.9`.
- Runtime dependencies are empty.
- Development dependencies include `pytest`, `pytest-cov`, and `ruff`.
- Ruff line length is 100.

Use pytest for behavior checks. Keep tests focused and synthetic; existing tests
build fixture JSONL trees under `tmp_path` instead of depending on a real
`~/.claude` directory.

## Regression Coverage

Match the existing test surfaces:

- Parser and aggregation changes: update `ref/ccexplorer/tests/test_data.py`.
- Pricing changes: update `ref/ccexplorer/tests/test_pricing.py` and use
  `pytest.approx` for floating-point cost assertions.
- CLI default or exit-code changes: update `ref/ccexplorer/tests/test_cli.py`.
- HTML/template changes: update `ref/ccexplorer/tests/test_web.py`.

## Cost And Template Invariants

- Unknown models currently fall back to opus pricing in `pricing.family()`.
  Changing that fallback needs an explicit pricing test.
- `cost_for_usage()` treats missing or `None` token buckets as zero; preserve
  that behavior unless tests are intentionally rewritten.
- `render_html()` must replace all expected placeholders and embed JSON row data.
  `test_web.py` is the source of truth for placeholder coverage.

## Avoid

- Do not introduce broad refactors while changing a narrow parser or pricing rule.
- Do not add a dependency for work the standard library already handles here.
- Do not leave dashboard-specific or unrelated main-project guidance in this
  package spec; ccexplorer is a standalone Python reference package.
