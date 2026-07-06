# Passive source candidate evaluation

This is the first-batch candidate table for future passive readers. It is intentionally conservative: no parser is approved until the onboarding gate has real samples and token semantics.

| Candidate | Artifact family | Token semantics | Sample status | Privacy risk | Cursor idea | Decision |
| --- | --- | --- | --- | --- | --- | --- |
| Cursor | Local SQLite / editor usage artifacts | Unverified | No accepted llmusage fixture | May include project/session metadata | DB row id or timestamp if schema proves stable | Blocked: needs real sanitized samples |
| Kiro | Local session/log artifacts | Unverified | No accepted llmusage fixture | Unknown text/log contents | File fingerprint + offset if JSONL/log | Blocked: needs real sanitized samples |
| Copilot | Local editor/telemetry artifacts | Unverified and possibly aggregate-only | No accepted llmusage fixture | Auth/account-adjacent local data possible | DB/log cursor only after schema review | Blocked: needs privacy review and samples |
| Zed | Local JSONL/DB session artifacts | Unverified | No accepted llmusage fixture | May include prompts or file paths | File or DB cursor after schema review | Blocked: needs real sanitized samples |
| Goose | Local session logs | Unverified | No accepted llmusage fixture | May include prompt/response text | File fingerprint + offset if append-only | Blocked: needs real sanitized samples |
| Antigravity as passive reader | Local artifacts beyond hook trigger metadata | Unverified for Antigravity schema | No accepted token-bearing fixture | Unknown transcript/token fields | File fingerprint + tail signature if JSON | Blocked: no parser until a real schema fixture exists |
| Gemini CLI | Local JSON/JSONL temp/session artifacts | Unverified and distinct from Antigravity source id | No accepted llmusage fixture | May include prompts, responses, and local paths | File fingerprint + offset only after schema review | Monitor-only: `gemini` is not a `SourceKind` |
| Grok | Local artifact directory if present | Unverified | No accepted llmusage fixture | Unknown text/log contents | File fingerprint after schema review | Monitor-only: needs real local samples |
| Kimi / Qwen | Local wire/session JSONL artifacts | Unverified | No accepted llmusage fixture | May include prompts/responses and project paths | File fingerprint + offset if append-only | Monitor-only: needs token semantics |
| Roo / Kilo / Cline | VS Code extension task/storage artifacts | Unverified and extension-specific | No accepted llmusage fixture | May include prompts, files, and workspace metadata | Extension-specific file/DB cursor after review | Monitor-only: needs per-extension samples |
| Codebuff | Local session artifacts | Unverified | No accepted llmusage fixture | Unknown text/log contents | File fingerprint after schema review | Monitor-only: needs real local samples |
| Crush | Local database/artifacts | Unverified | No accepted llmusage fixture | Database may include workspace/session metadata | DB row id/timestamp after schema review | Monitor-only: needs schema review |
| Warp / Oz | Local config/data artifacts | Unverified | No accepted llmusage fixture | Unknown text/log contents | File or DB cursor after schema review | Monitor-only: needs real local samples |
| Amp | Local thread JSON artifacts | Unverified | No accepted llmusage fixture | May include prompts/responses and thread metadata | File fingerprint after schema review | Monitor-only: needs thread samples |
| Hermes | Local database artifacts | Unverified | No accepted llmusage fixture | Database may include session metadata | DB row id/timestamp after schema review | Monitor-only: needs schema review |
| Trae | Local config/database artifacts | Unverified and possibly auth-adjacent | No accepted llmusage fixture | Auth/account-adjacent local data possible | DB/log cursor only after privacy review | Monitor-only: needs privacy review and samples |
| OpenClaw / Pi / Droid / Gajae-Code | Local JSONL/session artifacts | Unverified | No accepted llmusage fixture | May include prompts/responses and local paths | File fingerprint + offset if append-only | Monitor-only: needs per-client samples |
| Synthetic | tokscale synthetic database | Not a real user tool source | No llmusage fixture needed | Local database generated for tests/demo | None | Unsupported: keep out of llmusage imports |

## Current outcome

No new passive parser is approved in this implementation slice. The shipped work is the descriptor/status/hook foundation, broad platform monitoring, and the onboarding gate. Monitor-only means `llmusage status` can report candidate roots and blocked parser state, but sync must not write usage rows for that platform.
