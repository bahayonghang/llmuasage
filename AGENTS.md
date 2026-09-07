# Repository Guidelines

## Project Structure & Module Organization

`src/main.rs` is the CLI entrypoint and `src/lib.rs` wires shared modules. Keep subcommands in `src/commands/`, parsers in `src/parsers/`, tool adapters in `src/integrations/`, SQLite/storage code in `src/store/`, and reporting/query logic in `src/query/`. The sync engine and job registry live in `src/sync/`. SSH remote protocol and importer live in `src/remote/`. Terminal and browser UIs live in `src/tui/` and `src/web/`; static dashboard assets are under `src/web/assets/`. `desktop/` is an independent Tauri crate (`desktop/src-tauri/Cargo.toml`), not a root workspace member. Integration tests live under `tests/<domain>/` and are discovered only through eight explicit Cargo targets. VitePress docs live in `docs/` with Chinese pages in `docs/zh/`. Treat `ref/` as upstream/reference code.

## Build, Test, and Development Commands

- `cargo run -- <command>`: run the CLI locally, e.g. `cargo run -- sync` or `cargo run -- serve`.
- Ordinary `llmusage sync` (bounded or unbounded) skips legacy token-accounting sources, keeps that source's history, and warns. Repair is `llmusage sync --rebuild --source <source>`.
- `just serve`: start the local web dashboard.
- `just tdev`: start the Tauri desktop development shell (`desktop-dev`).
- `just tinstall`: build the unsigned Windows NSIS installer and run it.
- `just docs`: run the VitePress docs dev server.
- `just build`: build the release binary and production docs.
- `just ci`: full local gate. The recipe does not run `cargo update` and does not rewrite lockfiles:
  `python scripts/check-ci-gate.py --self-test`,
  `python scripts/check-ci-gate.py`,
  `python scripts/ci-rust.py`,
  `node scripts/ci-js.mjs`,
  `just desktop-check`,
  `npm --prefix docs run docs:build`.
  `just ci` is not MSRV, `cargo audit`, or `cargo semver-checks`.
- Semver is a dedicated CI job: `cargo semver-checks --baseline-rev v1.2.0`. Do not pass `--locked` to that tool.
- `just install`: install the CLI from this checkout.

## Commands by change surface

Pick the smallest gate that covers the files you changed:

| Surface | Typical paths | Command |
| --- | --- | --- |
| Root Rust | `src/`, `tests/`, `Cargo.toml`, `Cargo.lock` | `python scripts/ci-rust.py`. For one domain: `cargo test --locked --all-features --test <target> -- --test-threads=1` |
| Dashboard JS | `src/web/assets/`, `scripts/tests/`, `scripts/*.mjs` | `node scripts/ci-js.mjs` |
| Desktop | `desktop/` | `just desktop-check` |
| Docs | `docs/` | `npm --prefix docs run docs:build` |
| Cross-surface | more than one row above | `just ci` |

The eight Cargo test targets are `api`, `architecture_dependencies`, `cli`, `query`, `remote`, `store`, `sync`, and `tui`.

## Coding Style & Naming Conventions

Use Rust 2024 and keep code `cargo fmt` clean. Follow standard Rust naming: `snake_case` for files, modules, and functions; `PascalCase` for types; `SCREAMING_SNAKE_CASE` for constants. Keep command modules aligned with command names, such as `src/commands/sync.rs`. Use short kebab-case names for docs pages, e.g. `getting-started.md`.

## Testing Guidelines

`Cargo.toml` sets `autotests = false`. Add focused integration tests under `tests/<domain>/` for command, parser, store, remote, query, and report behavior. The eight explicit targets are `api`, `architecture_dependencies`, `cli`, `query`, `remote`, `store`, `sync`, and `tui`. Prefer names that describe the surface, such as `accounting.rs` or `local_flow.rs`. Use `tempfile` for isolated homes, fixtures, and SQLite state. Run `cargo test --locked --all-features -- --test-threads=1` to match CI ordering; run targeted tests before `just ci`.

## Commit & Pull Request Guidelines

Recent history uses Conventional Commits with scopes, emoji, and often Chinese text, e.g. `feat(看板): [AI] ✨ ...` or `docs(文档): [AI] 📝 ...`. Keep commits narrow and grouped by feature or surface. PRs should include a summary, affected commands/docs paths, linked issues when available, validation output, and screenshots for dashboard or docs UI changes. When CLI behavior changes, update `README.md`, `README.zh-CN.md`, and matching docs pages.

## Security & Generated Files

Do not commit local usage data, SQLite databases, or copied user config from `~/.llmusage/`. Be careful with rebuild/reset paths and document destructive behavior. Do not hand-edit generated or dependency directories: `target/`, `docs/node_modules/`, `docs/.vitepress/cache/`, or `docs/.vitepress/dist/`. Local Trellis copies under `.agents/`, `.claude/`, `.codex/`, `.grok/`, `.kimi-code/`, and `.omp/` are gitignored; the versioned harness contract is `AGENTS.md` plus [`docs/agents/harness-contracts.md`](docs/agents/harness-contracts.md).

## Agent-Specific Notes

Before non-trivial domain changes, read `docs/agents/domain.md` and relevant ADRs in `docs/adr/`. For passive parser/source work, follow `docs/agents/passive-parser-onboarding.md` and update `docs/agents/passive-source-candidates.md` when appropriate.

Harness file discovery, hook fallback, read-only vs implementation boundary, and delegation examples: [`docs/agents/harness-contracts.md`](docs/agents/harness-contracts.md). `CLAUDE.md` is a single `@AGENTS.md` import. Do not copy a second rule set.

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
