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

## Current outcome

No new passive parser is approved in this implementation slice. The shipped work is the descriptor/status/hook foundation plus the onboarding gate.
