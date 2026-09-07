# Implement notes

Ordinary sync and serve no longer reset or parse legacy token-accounting sources. They keep existing rows, skip that source's writes, warn for `llmusage sync --rebuild --source <source>`, and do not advance the marker. Explicit `--rebuild` keeps the existing lossy gate.

Extra test files (needed for `python scripts/ci-rust.py`): `tests/sync/sources/grok.rs`, `tests/sync/sources/pi_omp.rs`, `tests/sync/sources/zcode.rs`.
