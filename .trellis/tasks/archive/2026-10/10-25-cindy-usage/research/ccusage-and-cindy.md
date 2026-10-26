# Research: Cindy usage source

Date: 2026-10-25. Planning evidence only. Do not copy raw session transcripts into fixtures.

## ccusage

Sources: [ccusage README](https://github.com/ccusage/ccusage/blob/main/README.md), [pi-agent guide](https://github.com/ccusage/ccusage/blob/main/docs/guide/pi/index.md), [config files](https://github.com/ccusage/ccusage/blob/main/docs/guide/config-files.md). Fetched 2026-10-25 from `ccusage/ccusage` `main`.

- ccusage reads local artifacts only and reports each product as its own agent. Unified `daily` includes every detected source. There is no Cindy row. `pi-agent` is a separate source from Codex and Claude.
- Default Pi root is `~/.pi/agent/sessions/`. `PI_AGENT_DIR` and `--pi-path` replace or list paths for that default Pi agent only. They are not the way to add a fork.
- Forks that write the same JSONL outside that root use additive named stores (`pi.stores`). Each store has its own agent name in unified reports. Names cannot collide with a built-in agent. Missing paths are empty. Resolved paths that overlap the default Pi store or another named store, including nesting, are rejected so usage is not counted twice.
- Named stores do not add a focused `ccusage omp` command. That is a ccusage CLI limit. llmusage already gives Oh My Pi its own `--source omp`, so a Cindy source should follow the local source-id model, not that CLI limit.
- Pi cost: auto mode can price from tokens when recorded cost is zero; `--mode display` shows Pi's recorded cost. llmusage already maps `usage.cost` for Pi / Oh My Pi (ADR 0016). Reuse that contract instead of inventing a Cindy cost mode.

## Cindy artifact

Sources: [makecindy/cindy](https://github.com/makecindy/cindy) `apps/desktop/src/main/localDb/ipc/sessions.ts`, `apps/desktop/src/main/regionUserData.ts`, `docs/local-agent-api.md`, `docs/linux.md`, `packages/maker-shared/src/brandIdentity.ts`. Code search on 2026-10-25.

- Desktop and headless hosts set the Pi agent home to `{userData}/pi-agent-home`. Session JSONL lives under `{userData}/pi-agent-home/sessions/`.
- Official userData directory names are `Cindy` (Mainland China) and `CindyGlobal` (global). `CindyDev` and `CindyGlobal-dev2-*` are dev or sandbox profiles.
- Published locations: Windows `%APPDATA%/Cindy` or `%APPDATA%/CindyGlobal`; macOS `~/Library/Application Support/`; Linux `$XDG_CONFIG_HOME` or `~/.config`. Installers do not move an existing region profile.
- Cindy's own skill analyzer explicitly skips Pi `type:message` records. A comment says discovering `pi-agent-home/sessions` and parsing Pi format is separate follow-up work. The SQLite rollup is therefore not the precise ledger for this agent.

## Local sample (this machine, 2026-10-26)

- `~/.pi/agent/sessions` does not exist. Two Cindy session files exist at `%APPDATA%\CindyGlobal\pi-agent-home\sessions\*.jsonl`.
- They are Pi session version 3. Assistant messages carry `usage.input/output/cacheRead/cacheWrite/totalTokens/cost`, `provider`, and `model`. Non-zero rows exist. `toolResult.usage` was absent in this sample.
- `sessions.total_token_usage` for `agent_kind=pi` was 0, and `daily_model_usage` had no Pi row. Those tables had separate Claude Code and Codex aggregates. Importing them would miss the Pi sessions and could double-count other agents later.
- One isolated Codex rollout exists under `%APPDATA%\CindyGlobal\codex-home\sessions`. llmusage Codex discovery reads `CODEX_HOME` or `~/.codex/sessions` only.
- `~/.llmusage/llmusage.db` has no `pi` events and no Cindy paths.

## Constraint from this repo

ADR 0015 already split Oh My Pi from Pi despite the shared JSONL shape. Hiding Cindy inside `pi` would repeat that mistake. `PI_AGENT_DIR` replaces the Pi root; pointing it at Cindy would drop a real Pi install and still label the rows `pi`.
