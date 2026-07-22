# Repository Guidelines

## Project Structure & Module Organization

`src/main.rs` is the CLI entrypoint and `src/lib.rs` wires shared modules. Keep subcommands in `src/commands/`, parsers in `src/parsers/`, tool adapters in `src/integrations/`, SQLite/storage code in `src/store/`, and reporting/query logic in `src/query/`. Terminal and browser UIs live in `src/tui/` and `src/web/`; static dashboard assets are under `src/web/assets/`. Integration tests are in `tests/*.rs`. VitePress docs live in `docs/` with Chinese pages in `docs/zh/`. Treat `ref/` as upstream/reference code.

## Build, Test, and Development Commands

- `cargo run -- <command>`: run the CLI locally, e.g. `cargo run -- sync` or `cargo run -- serve`.
- `just serve`: start the local web dashboard.
- `just docs`: run the VitePress docs dev server.
- `just build`: build the release binary and production docs.
- `just ci`: full gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features -- --test-threads=1`, `cargo doc --no-deps`, the dashboard JS checks (`node --check` / `node --test`), and `npm --prefix docs run docs:build`.
- `just install`: install the CLI from this checkout.

## Coding Style & Naming Conventions

Use Rust 2024 and keep code `cargo fmt` clean. Follow standard Rust naming: `snake_case` for files, modules, and functions; `PascalCase` for types; `SCREAMING_SNAKE_CASE` for constants. Keep command modules aligned with command names, such as `src/commands/sync.rs`. Use short kebab-case names for docs pages, e.g. `getting-started.md`.

## Testing Guidelines

Add focused integration tests in `tests/` for command, parser, store, and report behavior. Prefer names that describe the surface, such as `sync_regression.rs` or `local_flow.rs`. Use `tempfile` for isolated homes, fixtures, and SQLite state. Run `cargo test -- --test-threads=1` to match CI ordering; run targeted tests before `just ci`.

## Commit & Pull Request Guidelines

Recent history uses Conventional Commits with scopes, emoji, and often Chinese text, e.g. `feat(看板): [AI] ✨ ...` or `docs(文档): [AI] 📝 ...`. Keep commits narrow and grouped by feature or surface. PRs should include a summary, affected commands/docs paths, linked issues when available, validation output, and screenshots for dashboard or docs UI changes. When CLI behavior changes, update `README.md`, `README.zh-CN.md`, and matching docs pages.

## Security & Generated Files

Do not commit local usage data, SQLite databases, or copied user config from `~/.llmusage/`. Be careful with rebuild/reset paths and document destructive behavior. Do not hand-edit generated or dependency directories: `target/`, `docs/node_modules/`, `docs/.vitepress/cache/`, or `docs/.vitepress/dist/`.

## Agent-Specific Notes

Before non-trivial domain changes, read `docs/agents/domain.md` and relevant ADRs in `docs/adr/`. For passive parser/source work, follow `docs/agents/passive-parser-onboarding.md` and update `docs/agents/passive-source-candidates.md` when appropriate.
<!-- TRELLIS:START -->

# Trellis Instructions

These instructions are for AI assistants working in this project.

This project is managed by Trellis. The working knowledge you need lives under `.trellis/`:

- `.trellis/workflow.md` — development phases, when to create tasks, skill routing
- `.trellis/spec/` — package- and layer-scoped coding guidelines (read before writing code in a given layer)
- `.trellis/workspace/` — per-developer journals and session traces
- `.trellis/tasks/` — active and archived tasks (PRDs, research, jsonl context)

If a Trellis command is available on your platform (e.g. `/trellis:finish-work`, `/trellis:continue`), prefer it over manual steps. Not every platform exposes every command.

If you're using Codex or another agent-capable tool, additional project-scoped helpers may live in:

- `.agents/skills/` — reusable Trellis skills
- `.codex/agents/` — optional custom subagents

Managed by Trellis. Edits outside this block are preserved; edits inside may be overwritten by a future `trellis update`.

<!-- TRELLIS:END -->
