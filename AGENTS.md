# Repository Guidelines

## Project Structure & Module Organization
`src/main.rs` is the CLI entrypoint and `src/lib.rs` wires shared modules. Keep subcommand logic in `src/commands/`, storage and query code in `src/store/` and `src/query/`, parser and tool-specific adapters in `src/parsers/` and `src/integrations/`, and terminal or browser UIs in `src/tui/` and `src/web/`. Static dashboard assets live under `src/web/assets/`. Integration tests sit in `tests/*.rs`. Bilingual docs live in `docs/` and `docs/zh/`. Treat `ref/vibeusage/` as upstream reference code, not the primary edit target.

## Build, Test, and Development Commands
- `cargo run -- <command>`: run the CLI locally, for example `cargo run -- serve` or `cargo run -- sync`.
- `just serve`: start the local web dashboard.
- `just docs`: run the VitePress docs dev server.
- `just build`: build the release binary and production docs.
- `just ci`: repo gate; runs `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test -- --test-threads=1`, and `npm --prefix docs run docs:build`.
- `just install`: install the CLI from the current checkout.

## Coding Style & Naming Conventions
Use Rust 2024 defaults and keep code `cargo fmt` clean. Follow standard Rust naming: `snake_case` for modules, files, and functions; `PascalCase` for types; `SCREAMING_SNAKE_CASE` for constants. Keep each command in a matching module such as `src/commands/sync.rs`. For docs, prefer short kebab-case file names like `getting-started.md`.

## Testing Guidelines
Add or update integration tests in `tests/` for command, parser, and storage changes. Prefer focused names such as `sync_regression.rs` or `local_flow.rs`. Use `tempfile` for isolated filesystem and SQLite fixtures. Run `cargo test -- --test-threads=1` locally to match CI behavior.

## Commit & Pull Request Guidelines
Recent history uses Conventional Commits with scope and emoji, often in Chinese, for example `refactor(web): ♻️ ...` or `docs(readme): 📝 ...`. Keep commits narrow and grouped by feature or surface. PRs should include: a short summary, affected commands or docs paths, validation output from `just ci` or targeted commands, and screenshots when `src/web/` or docs UI changes. When command behavior changes, update both `README.md`/`README.zh-CN.md` and the matching docs pages.

## Security & Generated Files
Do not commit local usage data, SQLite files, or user config copied from `~/.llmusage/`. Do not hand-edit generated directories: `target/`, `docs/node_modules/`, `docs/.vitepress/cache/`, or `docs/.vitepress/dist/`.
