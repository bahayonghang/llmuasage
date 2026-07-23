# Upstream And Local Artifact Evidence

Evidence date: 2026-07-23. Inspection was read-only and emitted field names/counts only; no prompt, response, credential, or raw project path is suitable for a fixture.

## Kimi Code

Observed root: `~/.kimi-code/sessions/**/wire.jsonl` (custom root must be supported by the source descriptor/implementation if the upstream environment variable is used).

- 22 candidate files.
- 1099 valid `usage.record` lines.
- All observed records are `usageScope=turn`.
- Observed model is `kimi-code/k3`; the parser must retain the source model string rather than whitelist it.
- Usage keys: `inputOther`, `output`, `inputCacheRead`, `inputCacheCreation`.
- No stable record id is present in the Kimi Code record shape; event identity must therefore combine source path identity, byte/record position, timestamp, model, and normalized token tuple, while the file cursor handles append/rewrite detection.
- Quality decision: `precise` for the observed turn-level fields. Do not count session-scoped aggregate records or duplicate `step.end` usage.

## Pi And Oh My Pi

Observed roots:

- Pi default root: `~/.pi/agent/sessions` was absent on this machine.
- Oh My Pi root: `~/.omp/agent/sessions` contained 3 JSONL files.

Observed OMP records:

- 8 assistant messages with usage.
- Usage keys: `input`, `output`, `cacheRead`, `cacheWrite`, `totalTokens`, `reasoningTokens`, `cost`.
- Models observed: `gpt-5.5` and `codex-auto-review`.
- Session files also contain `title`, `session`, `model_change`, and thinking-level metadata; those lines must not become usage events.
- Quality decision: `precise` when `totalTokens` and the four channels are present. Preserve `reasoningTokens` as a separate diagnostic field and never add it to output by default.
- Product decision: represent Pi and Oh My Pi as one stable `pi` source with two roots. The root controls discovery/path hashing, not the persisted source id.

## Grok Build (Excluded After Verification)

Observed local state: `.grok` contains only `hooks/orca-status.json` and its backup; no `sessions/**/updates.jsonl`, `signals.json`, or executable is present on this machine.

Official source verification used `xai-org/grok-build` commit `a5727c5960452e7527a154b25cb5bf00cda0545e` (2026-07-22):

- `crates/codegen/xai-grok-pager/docs/user-guide/17-sessions.md:18-39,269-275` confirms that Grok persists sessions under `$GROK_HOME/sessions` or `~/.grok/sessions`, including `updates.jsonl` and `signals.json`.
- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/updates.rs:114-127` stamps every persisted update with `_meta.totalTokens`, but the value comes from `get_estimated_total_tokens()`.
- `crates/codegen/xai-chat-state/src/handle.rs:426-433` defines that value as the last model-reported context count plus a bytes/4 estimate for new tool results. It is used for context-overflow decisions, not as an accumulated usage ledger, and rewinds/compaction can reduce it.
- Exact per-call input/output/cache/reasoning accounting exists in the in-memory usage ledger, but `crates/codegen/xai-chat-state/src/usage.rs:1-4` explicitly marks that ledger as not serialized.
- `crates/codegen/xai-grok-shell/src/session/signals.rs:121-130` marks the exact per-turn input/output/cache fields `serde(skip)`. `acp_session_impl/turn.rs:1750-1768` persists only `snap.current`, so those exact fields do not enter `signals.json`.
- `crates/codegen/xai-grok-pager/docs/user-guide/24-monitoring-usage.md:133-166` exposes exact input/output/reasoning/cache-read usage through external OpenTelemetry, not through a replayable passive local usage artifact.

The pinned `tokscale` adapter derives positive deltas from `_meta.totalTokens`, but that measures context growth/estimation rather than reliable request usage and can diverge around repeated prompts, tool rounds, rewinds, and compaction. It therefore does not satisfy this repository's `TotalOnly` contract of a reliable total.

Decision: exclude Grok Build from this task. Local session files are directly readable, but they cannot reconstruct a reliable token-usage total; external OTEL is outside the passive local-reader boundary.

## Reasonix

- Current local app data contains 9 session JSONL files. Their parsed top-level records have `role/content/tool_calls` style fields and no usage-bearing lines.
- Six old `*.telemetry.json` sidecars expose aggregate `promptTokens`, `completionTokens`, `totalTokens`, `reasoningTokens`, cache counts, request count, and session cost.
- The sidecars are cumulative session summaries and can be rewritten independently of transcript lines; using them as event input would create double-counting and weak cursor semantics.

Decision: monitor-only / future candidate. Admission requires a current stable per-turn usage artifact, a privacy review, and cursor/dedupe fixtures.

## Privacy Boundary

Fixtures must contain only structural JSON/JSONL fields and synthetic ids/timestamps/models/token counts. Remove `content`, prompt/response bodies, tool arguments, credentials, raw workspace paths, and telemetry memory/compiler payloads. Store no raw archive for these sources unless a later explicit decision changes the repository's privacy boundary.
