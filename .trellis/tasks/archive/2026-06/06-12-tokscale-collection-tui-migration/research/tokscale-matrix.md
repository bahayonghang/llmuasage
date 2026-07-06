# tokscale Reference Matrix

This matrix records the implementation decision for each tokscale-inspired platform. It is intentionally conservative: a platform can be monitored without becoming a persisted `SourceKind`, and parser promotion requires sanitized fixtures, documented token semantics, sync-twice tests, cursor/fingerprint regression coverage, and privacy review.

| Platform | tokscale-style root / pattern evidence | Local parse evidence | Token quality | llmusage parser status | llmusage action |
| --- | --- | --- | --- | --- | --- |
| Codex | `CODEX_HOME/sessions` or `~/.codex/sessions`, `*.jsonl` | Existing parser and tests | precise | registered | Keep as parsed `SourceKind::Codex`; expose fingerprint/skipped sync visibility. |
| Claude | `~/.claude/projects`, `*.jsonl` | Existing parser and tests | precise | registered | Keep as parsed `SourceKind::Claude`; expose fingerprint/skipped sync visibility. |
| OpenCode | `OPENCODE_DB`, `OPENCODE_HOME`, XDG/local app data, `opencode*.db` | Existing parser and new channel DB discovery tests | precise | registered | Keep as parsed `SourceKind::Opencode`; support explicit/channel DB discovery. |
| Antigravity | `~/.gemini/config/hooks.json` | Integration descriptor only; no accepted token-bearing artifact | total_only label only for current integration metadata | blocked_no_samples | Monitor and explain as integration-only; do not write usage rows beyond existing source semantics. |
| Gemini CLI | `GEMINI_CLI_HOME/tmp` or `~/.gemini/tmp`, `*.json`, `*.jsonl` | No sanitized schema fixture accepted | unavailable | blocked_no_samples | Monitor-only; do not restore `gemini` as a `SourceKind`. |
| Cursor | tokscale cursor cache/export candidates, `usage*.csv` | No stable local usage export fixture accepted | unavailable | blocked_no_samples | Monitor-only until stable export schema and token fields are proven. |
| GitHub Copilot | config/app data DB or JSON candidates | No token semantics or privacy review | unavailable | blocked_no_samples | Monitor-only; require privacy review before parser work. |
| Zed | XDG or app support data, `*.db`, `*.jsonl` | No fixture coverage | unavailable | blocked_no_samples | Monitor-only. |
| Kiro | `~/.kiro`, JSON/JSONL candidates | No fixture coverage | unavailable | blocked_no_samples | Monitor-only. |
| Goose | config dir, JSONL/DB candidates | No fixture coverage | unavailable | blocked_no_samples | Monitor-only. |
| Grok | `~/.grok`, JSON/JSONL candidates | No fixture coverage | unavailable | blocked_no_samples | Monitor-only. |
| Kimi / Qwen | `~/.kimi`, `KIMI_CODE_HOME`, `~/.qwen`, wire/session JSONL candidates | No token semantics | unavailable | blocked_no_samples | Monitor-only. |
| Roo / Kilo / Cline | VS Code globalStorage JSON task artifacts | Extension-specific schemas not reviewed | unavailable | blocked_no_samples | Monitor-only; split into dedicated parser candidates only after per-extension samples exist. |
| Codebuff | `~/.codebuff`, JSON/JSONL candidates | No fixture coverage | unavailable | blocked_no_samples | Monitor-only. |
| Crush | `~/.crush`, DB/JSON candidates | DB schema not reviewed | unavailable | blocked_no_samples | Monitor-only. |
| Warp / Oz | warp config or `~/.oz`, JSON/DB candidates | No fixture coverage | unavailable | blocked_no_samples | Monitor-only. |
| Amp | XDG amp thread JSON candidates | No thread samples accepted | unavailable | blocked_no_samples | Monitor-only. |
| Hermes | data dir DB candidates | DB schema not reviewed | unavailable | blocked_no_samples | Monitor-only. |
| Trae | config DB/JSON candidates | Auth/privacy review missing | unavailable | blocked_no_samples | Monitor-only. |
| OpenClaw / Pi / Droid | agent/session JSONL candidates | No per-client samples | unavailable | blocked_no_samples | Monitor-only. |
| Gajae-Code | `GJC_CONFIG_DIR` or XDG session JSONL candidates | No fixture coverage | unavailable | blocked_no_samples | Monitor-only. |
| Synthetic | tokscale synthetic DB | Not a real local usage source | unavailable | unsupported | Keep out of imports; monitor descriptor exists only to explain exclusion. |

## Parser Promotion Gate

A monitored platform can move into `SourceKind` and `registered_parsers()` only after all of the following are true:

- Sanitized real fixture or generated fixture preserving the real schema shape.
- Token mapping for input, output, cache read, cache creation/write, reasoning, total, model, timestamp, project/session, and source file id.
- Sync-twice test proving unchanged artifacts do not duplicate usage rows.
- Cursor/fingerprint regression test proving append-only and rewritten-file behavior.
- Source-status/probe test showing parser support and token quality labels.
- Documentation update explaining privacy boundary and quality label.

No parserless platform currently satisfies this gate, so this task completes parser promotion by documenting the block and keeping those platforms monitor-only.

