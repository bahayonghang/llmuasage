# ccexplorer Backend Guidelines

`ref/ccexplorer` is a small Python package that reads Claude Code JSONL files,
aggregates usage in memory, and renders a self-contained HTML report. It has no
database, no runtime third-party dependencies, and a pytest/ruff development
toolchain.

## Pre-Development Checklist

- Read [Directory Structure](./directory-structure.md) before moving modules or adding CLI/report code.
- Read [Database Guidelines](./database-guidelines.md) before adding persistence or cache behavior.
- Read [Error Handling](./error-handling.md) before changing JSONL parsing, file walking, or CLI exits.
- Read [Logging Guidelines](./logging-guidelines.md) before adding progress, warning, or quiet-mode output.
- Read [Quality Guidelines](./quality-guidelines.md) before changing pricing, aggregation, templates, or tests.
- Also read `.trellis/spec/guides/index.md` for shared cross-layer and reuse checks.

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Package modules, tests, template ownership | Documented |
| [Database Guidelines](./database-guidelines.md) | Filesystem JSONL and in-memory aggregation rules | Documented |
| [Error Handling](./error-handling.md) | Skip-vs-fail behavior for malformed input and CLI errors | Documented |
| [Quality Guidelines](./quality-guidelines.md) | pytest, ruff, pricing, and template regression rules | Documented |
| [Logging Guidelines](./logging-guidelines.md) | stderr progress, warnings, and quiet mode | Documented |

## Quality Check

- For spec-only changes, scan `.trellis/spec/ccexplorer/backend/` for template
  markers and trailing whitespace.
- For runtime changes, run the relevant pytest slice under `ref/ccexplorer/tests/`;
  pricing and template changes should include `test_pricing.py` or `test_web.py`.
- Confirm no runtime dependency was added unless `pyproject.toml` and this spec
  intentionally document the new dependency boundary.

## Core References

- `ref/ccexplorer/PROJECT.md` describes the tool as Cost Explorer for Claude Code JSONL.
- `ref/ccexplorer/pyproject.toml` pins Python `>=3.9`, no runtime dependencies, pytest/ruff dev tooling, and line length 100.
- `ref/ccexplorer/ccexplorer/data.py` owns ingestion and aggregation.
- `ref/ccexplorer/ccexplorer/pricing.py` owns static price families and token cost math.
- `ref/ccexplorer/ccexplorer/cli.py` owns argparse commands and process exit codes.
- `ref/ccexplorer/ccexplorer/web.py` owns HTML template substitution.
- `ref/ccexplorer/tests/` contains the trusted examples for parser, pricing, CLI, and template changes.

All ccexplorer spec documentation is written in English.
