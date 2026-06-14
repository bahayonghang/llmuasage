# Restore source-status CLI command

## Goal

Restore `llmusage source-status` as a documented, first-class CLI entrypoint so the README/docs command references match actual CLI behavior.

## Requirements

- Add a visible `source-status` subcommand to the Clap `Commands` enum.
- Reuse the existing source/platform status logic in `src/commands/source_status.rs` instead of duplicating capability calculations in a new module.
- Keep `llmusage status` working as the broader health/status command.
- Include `source-status` in user-facing CLI help surfaces that list common commands.
- Add focused tests proving the command parses, appears in help, and can execute against an isolated runtime.

## Acceptance Criteria

- [x] `llmusage source-status --help` succeeds.
- [x] `llmusage source-status` succeeds against a fresh isolated `LLMUSAGE_HOME`.
- [x] Clap help includes `source-status` as a visible subcommand.
- [x] The custom top-level help table includes `source-status`.
- [x] Existing `llmusage status` behavior remains available.

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
- Repository evidence: README/docs already mention `source-status`; `src/commands/mod.rs` currently registers `Status` but no `SourceStatus`; `src/commands/source_status.rs` provides reusable builders used by `status`.
- Scope boundary: this task does not add new parser/platform capabilities or change source promotion rules.
