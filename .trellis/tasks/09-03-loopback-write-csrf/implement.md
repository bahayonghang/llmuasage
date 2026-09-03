# Implement: loopback write CSRF

1. Inventory POST routes on `loopback_router`.
2. Shared guard next to `reject_non_local_write`.
3. Tests: evil Origin, missing Origin + bad Host, good Origin, public still 404.
4. Confirm dashboard JS still posts same-origin.
5. Run web tests inside `src/web/mod.rs` (`cargo test --all-features --lib web`).

Risky file: `src/web/mod.rs` (already 6k lines). Keep the guard small; do not relocate the module in this task.
