# Document codex-tracer usage and inspect schema asset

## Goal

Document the shipped `llmusage codex-tracer` command in the repo docs and both READMEs, and verify whether `src/commands/codex_tracer/schema.sql` is an acceptable repository-local source asset.

## Requirements

- Inspect `src/commands/codex_tracer/schema.sql` and its call sites.
- Decide whether the `.sql` file fits current repo conventions based on actual usage, not guesswork.
- Add `codex-tracer` usage documentation to:
  - `README.md`
  - `README.zh-CN.md`
  - repo docs under `docs/` and `docs/zh/`
- Keep the documentation aligned with the real Clap help and current implementation.
- Keep the change scoped to task artifacts and docs/readme files unless a docs-only support change is required for buildability.

## Acceptance Criteria

- [ ] The task records whether `schema.sql` is a runtime source asset and whether that is acceptable in this repo.
- [ ] `README.md` includes a `codex-tracer` usage entry aligned with the shipped command surface.
- [ ] `README.zh-CN.md` includes the same usage guidance in Chinese.
- [ ] The docs site includes a `codex-tracer` guide page in English and Chinese, with links from existing entry pages.
- [ ] `docs/reference/cli.md` and `docs/zh/reference/cli.md` mention `llmusage codex-tracer`.
