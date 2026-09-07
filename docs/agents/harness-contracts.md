# Harness contracts

Versioned fallback for Claude Code, Codex, Grok Build, Kimi Code, and Oh My Pi (OMP). Shared project facts live in the repository-root `AGENTS.md`. `CLAUDE.md` is a single `@AGENTS.md` import. Do not copy global AGENTS files, skill libraries, or team memory into this page.

Evidence date for every item below: **2026-09-07**. A live new-session hook, skill, or agent handshake is **UNVERIFIED** unless a row says a later probe ran. File presence is not proof that a fresh session loaded the file.

## Shared project facts

| Fact | Value |
| --- | --- |
| Sync engine | `src/sync/` |
| Remote protocol / importer | `src/remote/` |
| Desktop crate | `desktop/` (`desktop/src-tauri/Cargo.toml`; not a root workspace member) |
| Integration tests | `tests/<domain>/` with `[package] autotests = false` |
| Cargo test targets | `api`, `architecture_dependencies`, `cli`, `query`, `remote`, `store`, `sync`, `tui` |
| Ordinary sync | Skips legacy token-accounting sources, keeps that source's history, warns. Does not rebuild. |
| Explicit repair | `llmusage sync --rebuild --source <source>` |
| Semver | `cargo semver-checks --baseline-rev v1.2.0`. Do not pass `--locked`. |
| JS gate | `node scripts/ci-js.mjs` (enumerates `scripts/tests/*.test.mjs`) |
| Desktop gate | `just desktop-check` |
| Local full gate | `just ci` (no `cargo update`; not MSRV, `cargo audit`, or semver) |

`just ci` runs, in order:

```
python scripts/check-ci-gate.py --self-test
python scripts/check-ci-gate.py
python scripts/ci-rust.py
node scripts/ci-js.mjs
just desktop-check
npm --prefix docs run docs:build
```

Pick commands by change surface from `AGENTS.md`.

Applicable tools: Claude Code, Codex, Grok Build, Kimi Code, OMP.  
Verification: read `AGENTS.md`, `Cargo.toml` `[[test]]` entries, `justfile` `ci` / `desktop-check`, `scripts/ci-js.mjs`, `scripts/ci-rust.py`, `README.md` ordinary-sync wording, `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md`, `.trellis/spec/llmusage/backend/token-accounting-contracts.md`.

## Permission boundary

Read-only review (planning, research, independent final review):

- Read tracked project files, task artifacts, and specs.
- Research may write only under the active task's `research/` directory.
- Do not edit product files, `.trellis/spec/` owned by another task, `.trellis/scripts/`, ignored harness copies, user-level model config, or the team knowledge base.
- Do not `git commit`, `git push`, `git merge`, `task.py archive`, or start extra paid harness sessions.

Approved implementation:

- Task status is `in_progress`.
- Frozen file set and acceptance criteria come from the active task.
- Write only owned files. Unpinned sub-agents inherit the parent session model. Planning and independent final review must not auto-downgrade.
- Model names describe quality and permission. A harness brand is not a price. Do not treat an inherited parent model as a cheap lane.

Upgrade to the planning-quality model when requirements are unclear, a red regression has no explanation, or permission, transaction, or protocol design changes.

Applicable tools: Claude Code, Codex, Grok Build, Kimi Code, OMP.  
Verification: parent `research/harness-matrix.md` plus this page. Live sub-agent permission enforcement in a new session is UNVERIFIED.

## Manual context path (hooks off or pull-based)

Use this path when hook injection is off, the `<!-- trellis-hook-injected -->` marker is absent, or the platform is pull-based (Grok Build, Kimi Code):

1. Read repository-root `AGENTS.md` and this file.
2. Resolve the active task: first line `Active task: <path>`, else `python ./.trellis/scripts/task.py current --source`.
3. Read `<task-path>/implement.jsonl` or `check.jsonl` (skip rows without `"file"`).
4. Read `<task-path>/prd.md`, then `design.md` if present, then `implement.md` if present.
5. Read `.trellis/workflow.md` and the specs listed in the jsonl.

Start hook commands from the repository root. Relative `.codex/hooks/` and `.claude/hooks/` commands fail when cwd is `src/`. Resolving the repo root first is a manual fallback. Native relative-command repair is an upstream Trellis template change and is **not done**.

Applicable tools: Claude Code, Codex, Grok Build, Kimi Code, OMP.  
Verification: parent Codex `cwd=src` probe exit 2 on 2026-09-07; Grok/Kimi described as pull-based in `.trellis/scripts/common/cli_adapter.py`. Live SessionStart injection remains UNVERIFIED.

## Five-tool matrix

### Claude Code

| Topic | Contract |
| --- | --- |
| File discovery order | 1. User/managed Claude settings (outside this repo). 2. Root `CLAUDE.md` (`@AGENTS.md`). 3. Nested `CLAUDE.md` walking from cwd toward the git root (product tree: root file only; ignore `ref/` and `target/`). 4. `.claude/settings.json`. 5. `.claude/agents/*.md`. 6. `.claude/skills/**/SKILL.md`. 7. `.claude/hooks/*.py`. |
| Configured | Local gitignored copies exist: `.claude/settings.json`, `.claude/agents/trellis-{research,implement,check}.md`, skills, hooks. `CLAUDE.md` import is tracked. |
| UNVERIFIED | Fresh-session load of `CLAUDE.md`, SessionStart / UserPromptSubmit / Task PreToolUse injection, hook trust. |
| Manual path | Same as [Manual context path](#manual-context-path-hooks-off-or-pull-based). `trellis-implement` and `trellis-check` say: if `<!-- trellis-hook-injected -->` is absent, pull jsonl from `Active task:`. `trellis-research` resolves the task with `python ./.trellis/scripts/task.py current --source`. |
| Review vs implement | `trellis-research` may write only `research/`. `trellis-implement` / `trellis-check` may write owned product files after approval. No `git commit` / `git push` / `git merge` from those agents. |
| Delegation | Main session Task/Agent tool, agent name `trellis-implement` / `trellis-check` / `trellis-research`. Prompt first line: `Active task: <path>`. Existing three agents do not pin a model; they inherit the parent. |

Hook commands in `.claude/settings.json` are `python .claude/hooks/...` (relative). Claude docs recommend `CLAUDE_PROJECT_DIR`. This repo documents the root-cwd fallback; the generated template is not fixed here.

Verification: read `.claude/settings.json` and `.claude/agents/*.md`; parent research row Claude Code 2.1.263. Live handshake UNVERIFIED.

### Codex

| Topic | Contract |
| --- | --- |
| File discovery order | 1. `AGENTS.md` from cwd toward git root (`.codex/config.toml` `project_doc_fallback_filenames = ["AGENTS.md"]`). 2. `.codex/config.toml`. 3. `.codex/hooks.json`. 4. `.codex/agents/*.toml`. 5. `.agents/skills/**/SKILL.md`. 6. `.codex/skills/` if present. |
| Configured | Local gitignored copies exist. `[agents] max_depth = 1` is set. `model` and `model_reasoning_effort` in the three agent TOMLs are commented, so agents inherit the parent. |
| UNVERIFIED | Fresh-session `AGENTS.md` merge, SubagentStart injection, `/hooks` TUI approval, exact-hash trust. `.codex/config.toml` comments say hooks need user-level `[features].hooks = true`. 2026-09-07 `codex features list`: `hooks` stable true, `multi_agent` stable true. Treat the live features/trust bits as the source of truth, not the comment. |
| Manual path | If `Full hook output saved to: <path>` appears, read that file. Else if `<!-- trellis-hook-injected -->` is absent, pull jsonl from `Active task:`. |
| Review vs implement | `trellis-research.toml` writes `research/` only. `trellis-implement.toml` and `trellis-check.toml` use `sandbox_mode = "workspace-write"` on owned files after approval. |
| Delegation | Native Codex subagents from `.codex/agents/trellis-*.toml`. Prompt first line: `Active task: <path>`. |

`.codex/hooks.json` commands are `python -X utf8 .codex/hooks/...`. Parent probe: cwd=`src` → Python exit 2, file not found. cwd=repo root can run the script. Start from the repository root, or resolve the repo root before the hook. Native relative-command repair is **not done**.

Verification: read `.codex/config.toml`, `.codex/hooks.json`, `.codex/agents/*.toml`; 2026-09-07 `codex features list`; parent cwd=`src` probe. Live handshake UNVERIFIED.

### Grok Build

| Topic | Contract |
| --- | --- |
| File discovery order | 1. Root `AGENTS.md` and `CLAUDE.md`. 2. `.grok/agents/*.md`. 3. `.grok/skills/**/SKILL.md`. 4. `.grok/commands/*.md`. 5. Shared `.agents/skills/` if the client also scans that tree. Grok is pull-based: hooks were OFF on the 2026-09-07 inspect. Do not assume Claude/Codex hook injection. |
| Configured | 2026-09-07 `grok inspect` from the repo root (CLI 1.0.22): project instructions include `AGENTS.md` and `CLAUDE.md`; 97 skills; project agents `trellis-check`, `trellis-implement`, `trellis-research`; Claude-compat hooks OFF. Local `.grok/agents/trellis-{research,implement,check}.md` exist. Inspect is configuration discovery, not a new-session handshake. |
| UNVERIFIED | Fresh-session load, `spawn_subagent` delivery of `Active task:`, skill auto-load in a chat turn. |
| Manual path | Always pull. This is the normal Grok path, not a degradation. Follow [Manual context path](#manual-context-path-hooks-off-or-pull-based). |
| Review vs implement | Same shared boundary. Grok agents do not pin a model; they inherit the parent. Do not infer review quality or cheap execution from the Grok brand. |
| Delegation | Main session `spawn_subagent` with `subagent_type` `trellis-implement` / `trellis-check` / `trellis-research`. Prompt first line: `Active task: <path>`. |

Do not enable Grok hooks only to "align" with Claude. Leave hooks OFF unless a later task authorizes a change.

Verification: 2026-09-07 `grok inspect` (CLI 1.0.22) from the repository root; `.trellis/scripts/common/cli_adapter.py` documents Grok as `pull-based skills/agents; no hook context injection`. Parent audit recorded 1.0.21. Live handshake UNVERIFIED.

### Kimi Code

| Topic | Contract |
| --- | --- |
| File discovery order | 1. Root `AGENTS.md`. 2. Official project skill auto-scan: `.kimi/skills`, `.claude/skills`, `.codex/skills`, `.agents/skills`. 3. Built-in sub-agents `plan`, `explore`, `coder`. **Official auto-scan does not include `.kimi-code/skills`.** Do not claim `.kimi-code` is auto-scanned. An auto-scanned `.agents/skills/trellis-check` skill is the shared Trellis skill, not the Kimi three-role adapter. |
| Three-role instructions | Readable from known paths: `.kimi-code/skills/trellis-research/SKILL.md`, `.kimi-code/skills/trellis-implement/SKILL.md`, `.kimi-code/skills/trellis-check/SKILL.md`. The main session must read those files and include the instructions when dispatching a built-in sub-agent. |
| Configured | Those three `SKILL.md` files exist locally (gitignored). Shared `.agents/skills` also exists and is in the official scan list. This repo has no `.kimi/skills` tree. |
| UNVERIFIED | Fresh-session skill auto-discovery, built-in `coder`/`explore`/`plan` receiving pasted role instructions, any secondary-model override. User-level `secondary_model` is out of scope. `highspeed` is not proof of a cheap lane. |
| Manual path | Always pull task jsonl. Include the matching `.kimi-code/skills/trellis-<role>/SKILL.md` body in the dispatch prompt. |
| Review vs implement | Kimi has no project-level custom sub-agent definitions. `explore` is read-only and cannot persist research files. Research, implement, and check all dispatch built-in `coder` with the matching SKILL.md, plus a statement that the child is already that Trellis role. |
| Delegation | Agent tool → built-in `coder` (or `explore` only for read-only search that must not write). Prompt: `Active task: <path>`, then the SKILL.md instructions, then the read/write boundary. Unpinned children inherit the parent. |

`.trellis/workflow.md` still tells the main session to dispatch built-in `coder`/`explore` with `.kimi-code/skills/trellis-<role>/SKILL.md`. That manual path works. Automatic discovery needs an upstream Trellis template or Kimi scan-path change and is **not done**.

Verification: parent research H2; local read of the three SKILL.md files; Kimi customization docs cited in `research/harness-matrix.md`. Live handshake UNVERIFIED.

### OMP (Oh My Pi)

| Topic | Contract |
| --- | --- |
| File discovery order | 1. Walk toward the git root for `AGENTS.md` (OMP may also see `CLAUDE.md`; this repo does not add a second rule set). 2. `.omp/agents/*.md`. 3. `.omp/skills/**/SKILL.md`. 4. `.omp/commands/*.md`. 5. `.omp/extensions/trellis/`. |
| Configured | Local gitignored copies exist, including `.omp/agents/trellis-{research,implement,check}.md`. |
| UNVERIFIED | Fresh-session context-file walk, `task` tool spawn, `pi/task` resolution against a live provider. |
| Manual path | Same pull path as Grok. OMP can read root `AGENTS.md`; do not add a duplicate OMP-only rule book. |
| Review vs implement | Same shared boundary. `trellis-research.md` and `trellis-implement.md` set `model: pi/task`. `trellis-check.md` has no model field and inherits. |
| Delegation | OMP `task` tool with those agent files. Prompt first line: `Active task: <path>`. |

`pi/task` is an inheritance hint: the child follows the parent session model. `pi/task` is not an ordinary searchable model id. A `models find` miss must not be reported as agent failure. `smol` / `prewalk` need a valid provider/model first; they are not a current cheap lane.

Verification: parent research OMP 18.1.12; read `.omp/agents/*.md`; [OMP context files](https://github.com/can1357/oh-my-pi/blob/main/docs/context-files.md) and [task-agent discovery](https://github.com/can1357/oh-my-pi/blob/main/docs/task-agent-discovery.md) as cited in parent research. Live handshake UNVERIFIED.

## Delegation examples

Each example names the active task path, read/write boundary, file ownership, model tier (quality and permission, not price), acceptance, and upgrade condition. Replace the path with the live `task.py current` value. Unpinned agents inherit the parent model.

### Claude Code — approved implement

- Active task: `.trellis/tasks/09-07-evergreen-harness-contracts` (repo-relative) or the absolute path under the checkout.
- Mechanism: Task/Agent tool → `.claude/agents/trellis-implement.md`.
- Read/write: workspace write on owned files. No git commit/push/merge. No edits under `.trellis/spec/` owned by other children.
- File ownership: `AGENTS.md`, `CLAUDE.md`, `docs/agents/harness-contracts.md`, plus this child's task notes.
- Model tier: inherit parent. Do not pin a model id in the agent file.
- Acceptance: PRD AC1–AC5; validation commands in `implement.md`.
- Upgrade: unclear requirements, unexplained red tests, or a permission/protocol change → return to the planning-quality parent. Do not auto-downgrade planning or independent final review.

### Codex — approved implement

- Active task: same path as above.
- Mechanism: native subagent `.codex/agents/trellis-implement.toml` (`sandbox_mode = "workspace-write"`; `model` / `model_reasoning_effort` commented → inherit).
- Read/write and ownership: same as Claude.
- Acceptance and upgrade: same as Claude. If SubagentStart injection is missing, pull jsonl.

### Grok Build — approved implement

- Active task: same path. Dispatch prompt first line must be `Active task: <path>`.
- Mechanism: `spawn_subagent(subagent_type="trellis-implement", prompt=...)`.
- Read/write and ownership: same as Claude. Always pull context; Grok does not inject SessionStart task jsonl.
- Model tier: inherit parent. Do not call the Grok brand a cheap lane.
- Acceptance and upgrade: same as Claude.

### Kimi Code — approved implement

- Active task: same path.
- Mechanism: Agent tool → built-in `coder`. Include the body of `.kimi-code/skills/trellis-implement/SKILL.md`. State that the child is already `trellis-implement` and must not spawn another implement/check agent. Do not rely on `.kimi-code` auto-scan.
- Read/write and ownership: same as Claude.
- Model tier: inherit parent unless the user already configured a working secondary model. Do not invent a per-agent override. `highspeed` is not a cheap-lane proof.
- Acceptance and upgrade: same as Claude.

### OMP — approved implement

- Active task: same path.
- Mechanism: `task` tool → `.omp/agents/trellis-implement.md`. `model: pi/task` means inherit the parent session. Do not search for a model named `pi/task`.
- Read/write and ownership: same as Claude.
- Model tier: inherit parent. `smol`/`prewalk` only after a provider actually resolves those targets.
- Acceptance and upgrade: same as Claude.

### Independent final review (all five tools)

- Active task: same path as the implement examples. Replace with the live `task.py current` value.
- Read/write: workspace write on owned files for self-fix. No git commit/push/merge. No edits under `.trellis/spec/` owned by other children.
- File ownership: `AGENTS.md`, `CLAUDE.md`, `docs/agents/harness-contracts.md`, plus this child's task notes.
- Model tier: planning-quality parent. Unpinned children inherit that parent. Do not chain check from a downgraded implementer. Planning and independent final review must not auto-downgrade.
- Acceptance: PRD AC1–AC5; validation commands in `implement.md`. Do not accept the implementer's self-reported PASS as the only evidence. Live handshakes stay UNVERIFIED.
- Upgrade: unclear requirements, unexplained red tests, or a permission/protocol change → stay on the planning-quality parent.

Mechanisms (first prompt line `Active task: <path>`):

- Claude Code: Task/Agent tool → `.claude/agents/trellis-check.md`.
- Codex: native subagent `.codex/agents/trellis-check.toml` (`sandbox_mode = "workspace-write"`; `model` / `model_reasoning_effort` commented → inherit).
- Grok Build: `spawn_subagent(subagent_type="trellis-check", prompt=...)`. Always pull context.
- Kimi Code: Agent tool → built-in `coder`. Include `.kimi-code/skills/trellis-check/SKILL.md`. State that the child is already `trellis-check`. Do not rely on `.kimi-code` auto-scan. Do not treat `.agents/skills/trellis-check` as that adapter.
- OMP: `task` tool → `.omp/agents/trellis-check.md` (no `model` field → inherit).

Applicable tools: Claude Code, Codex, Grok Build, Kimi Code, OMP.  
Verification: agent definition files listed above; parent `research/harness-matrix.md` inherit-when-unpinned rule. Live dispatch in a new session is UNVERIFIED.

## Generated-template drift

Ignored local copies are not the team contract. Tracked sources of truth are `AGENTS.md`, `CLAUDE.md`, and this file. Do not mark upstream Trellis fixes done.

| Path | Observed drift | Upstream suggestion | Status |
| --- | --- | --- | --- |
| `.codex/hooks.json` (commands at the SessionStart, UserPromptSubmit, SubagentStart, PreToolUse entries) | Relative `python -X utf8 .codex/hooks/<script>.py`. Fails when cwd is `src/` (parent probe exit 2). | Trellis template: resolve the repository root with a platform-stable root (Codex hook cwd is session cwd) and re-verify from repo root and from `src/`. | Not done |
| `.claude/settings.json` hook `command` values | Relative `python .claude/hooks/<script>.py`. Same cwd risk; not upgraded to a full session failure in the parent audit. | Trellis template: prefer `CLAUDE_PROJECT_DIR` (or equivalent) so the script path does not depend on startup cwd. | Not done |
| `.kimi-code/skills/trellis-{research,implement,check}/SKILL.md` | Files exist, but Kimi official auto-scan is `.kimi/skills`, `.claude/skills`, `.codex/skills`, `.agents/skills`. `.trellis/workflow.md` and `.trellis/scripts/common/cli_adapter.py` still point at `.kimi-code`. | Either emit role skills into a scanned directory, or request Kimi to scan `.kimi-code/skills`. Until then, main session reads the known paths. | Not done |
| `.codex/config.toml` comments on hooks | Comments say project config cannot enable hooks and the user must set `[features].hooks = true`. Parent 2026-09-07 features list had `hooks=true`; exact-hash trust is a separate gate. | Align the generated comment with current Codex defaults. Do not treat the comment as live state. | Comment drift only; no product fix in this task |
| `.gitignore` entries `.agents/`, `.claude/`, `.codex/`, `.grok/`, `.kimi-code/`, `.omp/` | Local Trellis copies are untracked. `trellis update` dry-run "unchanged" does not prove the templates are correct. | Keep the copies gitignored. Put durable rules in this tracked file. | By design |
| `.trellis/workflow.md` Kimi dispatch line | Instructs built-in `coder`/`explore` to read `.kimi-code/skills/trellis-<role>/SKILL.md`. | Same as the Kimi scan-path row. Manual read remains valid. | Not done (upstream / Trellis scripts are out of this child's frozen set) |

Applicable tools: Claude Code, Codex, Grok Build, Kimi Code, OMP.  
Verification: parent `research/harness-matrix.md` H1–H4; local file reads on 2026-09-07. Upstream patches in the Trellis installer/configurator are not part of this repo change.

## Contract register

| ID | Contract | Applicable tools | Evidence date | Verification method | Live handshake |
| --- | --- | --- | --- | --- | --- |
| C1 | `AGENTS.md` lists `src/sync`, `src/remote`, `desktop/`, `tests/<domain>/`, eight Cargo targets, change-surface commands, current `just ci` (no lockfile update), and a link to this file | Claude Code, Codex, Grok Build, Kimi Code, OMP | 2026-09-07 | Read `AGENTS.md`, `Cargo.toml`, `justfile` | n/a (tracked files) |
| C2 | `CLAUDE.md` remains `@AGENTS.md` only | Claude Code (also readable by Grok/OMP) | 2026-09-07 | Read `CLAUDE.md` | UNVERIFIED as a fresh Claude import |
| C3 | Ordinary `sync` skips legacy sources and keeps history; repair is `sync --rebuild --source <source>` | Claude Code, Codex, Grok Build, Kimi Code, OMP | 2026-09-07 | Read `README.md` and token-accounting contracts | n/a |
| C4 | Semver command is `cargo semver-checks --baseline-rev v1.2.0` without `--locked` | Claude Code, Codex, Grok Build, Kimi Code, OMP | 2026-09-07 | Read `justfile` / CI toolchain contracts | n/a |
| C5 | Five-tool discovery, configured vs UNVERIFIED, hook-off pull path, review vs implement | Claude Code, Codex, Grok Build, Kimi Code, OMP | 2026-09-07 | This matrix + parent harness-matrix | UNVERIFIED |
| C6 | Kimi three-role files are readable at `.kimi-code/skills/trellis-{research,implement,check}/SKILL.md`; `.kimi-code` is not claimed as auto-scan | Kimi Code | 2026-09-07 | Read those three files; parent H2 | UNVERIFIED auto-scan |
| C7 | Delegation examples include task path, R/W boundary, ownership, model tier, acceptance, upgrade; unpinned inherit; planning and independent final review do not auto-downgrade | Claude Code, Codex, Grok Build, Kimi Code, OMP | 2026-09-07 | This page vs local agent files | UNVERIFIED live dispatch |
| C8 | OMP `pi/task` is an inheritance hint, not a searchable model | OMP | 2026-09-07 | Read `.omp/agents/trellis-implement.md` and `trellis-research.md` | UNVERIFIED resolution |
| C9 | Generated-template drift lists exact paths; upstream fixes are not marked done | Claude Code, Codex, Grok Build, Kimi Code, OMP | 2026-09-07 | Parent H1–H4 + local reads | n/a |
| C10 | Ignored harness copies are not the versioned contract | Claude Code, Codex, Grok Build, Kimi Code, OMP | 2026-09-07 | `.gitignore` lines for those directories | n/a |

## UNVERIFIED live probes

Read-only CLI probes on 2026-09-07 (this child): `grok --version` / `grok inspect`, `codex --version` / `codex features list`, `claude --version`, `kimi --version` / `kimi doctor`, `omp --version`. File reads of the three Kimi SKILL.md paths. No new paid chat session. No new-session hook/skill/agent handshake.

Do not report these as loaded:

- Claude Code new-session SessionStart, UserPromptSubmit, and Task/Agent PreToolUse injection.
- Codex new-session hook trust, `/hooks` TUI approval, and SubagentStart jsonl delivery.
- Grok Build new-session `spawn_subagent` with `Active task:` delivery.
- Kimi Code new-session skill auto-discovery and built-in sub-agent receipt of `.kimi-code` SKILL.md text.
- OMP new-session context walk and `pi/task` provider resolution.
- Any cheap-lane binding (Claude Haiku, Codex mini/Terra/Luna, Kimi highspeed, OMP smol/prewalk) on this account.
