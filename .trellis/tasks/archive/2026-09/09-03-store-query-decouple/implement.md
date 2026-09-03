# Implement: store/query decouple

1. Add architecture fixture + test that currently fails on `src/store/mod.rs` importing query (or write the test after the move; include a fixture that would fail).
2. Move pricing types; update store/query/sync imports; keep query re-exports.
3. Move `register_functions`; `Store::open_connection` calls the new path; query timezone expressions still emit the same SQL function names.
4. Grep `crate::query` under `src/store` = 0.
5. Run `cargo test --test architecture_dependencies` and store/query/sync tests.

Risky files: `store/mod.rs`, `store/sync_writer.rs`, `store/pricing_catalog.rs`, `query/mod.rs`, `query/timezone.rs`.
