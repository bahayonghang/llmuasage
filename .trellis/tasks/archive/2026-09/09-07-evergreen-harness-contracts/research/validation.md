# Validation notes

Date: 2026-09-07. Working directory: repository root. No `just ci`. No `cargo update`. No commit.

## Baseline before edit

- `AGENTS.md` still said `tests/*.rs` and an outdated `just ci` list.
- `CLAUDE.md` was already `@AGENTS.md`.
- `docs/agents/harness-contracts.md` did not exist.

## Commands

| Command | Exit |
| --- | --- |
| `python ./.trellis/scripts/get_context.py --mode phase --platform grok` | 0 |
| `python ./.trellis/scripts/get_context.py --mode phase --platform codex` | 0 |
| `python ./.trellis/scripts/task.py validate .trellis/tasks/09-07-evergreen-harness-contracts` | 0 |
| `git diff --check` | 0 |
| `npm --prefix docs run docs:build` | 0 after replacing VitePress links to repo-root `AGENTS.md` with path text |

## Read-only CLI probes

| Command | Result |
| --- | --- |
| `grok --version` | `grok 1.0.22` |
| `grok inspect` | Project instructions include `AGENTS.md` and `CLAUDE.md`; 97 skills; project agents `trellis-check`, `trellis-implement`, `trellis-research`; Claude-compat hooks OFF |
| `codex --version` | `codex-cli 0.153.4` |
| `codex features list` | `hooks` stable true; `multi_agent` stable true |
| `claude --version` | `2.1.263 (Claude Code)` |
| `kimi --version` | `0.41.0` |
| `kimi doctor` | config.toml and tui.toml OK |
| `omp --version` | `omp/18.1.12` |

Inspect/features/version are not new-session hook/skill/agent handshakes.

## AC

- AC1: `AGENTS.md` lists `src/sync`, `src/remote`, `desktop/`, `tests/<domain>/`, eight targets, change-surface commands, current `just ci`. `CLAUDE.md` unchanged `@AGENTS.md`.
- AC2: five-tool matrix in `docs/agents/harness-contracts.md`. Kimi roles named at `.kimi-code/skills/trellis-{research,implement,check}/SKILL.md`. No auto-scan claim for `.kimi-code`.
- AC3: one implement example per tool plus independent final review. Unpinned inherit. Planning/final review must not auto-downgrade. OMP `pi/task` is an inheritance hint.
- AC4: tracked writeback in `AGENTS.md` and `docs/agents/harness-contracts.md`. Each register row has tools, 2026-09-07, verification method. Upstream template drift is not marked done.
- AC5: probes above. Live handshakes UNVERIFIED.

## Out of scope (confirmed not edited)

`.trellis/spec/**`, `.trellis/scripts/`, sibling task dirs, ignored harness copies, Trellis installer, team knowledge base, model pins.
