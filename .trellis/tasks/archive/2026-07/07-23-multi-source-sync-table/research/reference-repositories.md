# Reference Repositories And Network Evidence

Evidence date: 2026-07-23. Local checkouts are pinned; URLs below are immutable commit links where possible.

## `ccusage`

- Checkout: `ref/repo/ccusage`, commit `31e084afbca3981af97ab6b55abe4f38f451bad4`.
- Remote: `https://github.com/ccusage/ccusage.git`.
- Pi adapter: `rust/crates/ccusage/src/adapter/pi/paths.rs` defaults to `~/.pi/agent/sessions`, accepts `PI_AGENT_DIR`, and supports named additional stores. `adapter/pi/parser.rs` filters assistant messages and maps `input`, `output`, `cacheRead`, `cacheWrite`, and `totalTokens`; `reasoningTokens` is present in newer local samples and must remain separate.
- Kimi adapter: `rust/crates/ccusage/src/adapter/kimi/paths.rs` recognizes both `~/.kimi` and `~/.kimi-code` roots. `adapter/kimi/parser.rs` accepts the new `usage.record` shape and counts only explicit `usageScope=turn` records.
- Immutable source links: [Pi paths](https://github.com/ccusage/ccusage/blob/31e084afbca3981af97ab6b55abe4f38f451bad4/rust/crates/ccusage/src/adapter/pi/paths.rs), [Pi parser](https://github.com/ccusage/ccusage/blob/31e084afbca3981af97ab6b55abe4f38f451bad4/rust/crates/ccusage/src/adapter/pi/parser.rs), [Kimi parser](https://github.com/ccusage/ccusage/blob/31e084afbca3981af97ab6b55abe4f38f451bad4/rust/crates/ccusage/src/adapter/kimi/parser.rs).

## `tokscale`

- Checkout: `ref/repo/tokscale`, commit `0d620fbe331c3e54c45e277320e655d6978755eb`.
- Remote: `https://github.com/junhoyeo/tokscale`.
- `crates/tokscale-core/src/clients.rs:260-264` registers Kimi's `wire.jsonl`.
- `crates/tokscale-core/src/sessions/kimi.rs:153-214` documents Kimi Code's `usage.record`, `usageScope=turn`, camelCase token fields, timestamp, and model normalization. Its test at `:601-617` proves the shape without needing a user transcript.
- `crates/tokscale-core/src/sessions/pi.rs` parses assistant usage from Pi-compatible JSONL and keeps the source store name in the display model.
- `crates/tokscale-core/src/sessions/grok.rs` parses `updates.jsonl`, derives positive deltas from `_meta.totalTokens`, and heuristically reconciles compaction via `signals.json`. Official Grok Build source shows that this field is context state/estimation rather than a reliable accumulated usage total, so the adapter is evidence of a possible heuristic, not a valid llmusage token contract.
- Immutable source links: [clients](https://github.com/junhoyeo/tokscale/blob/0d620fbe331c3e54c45e277320e655d6978755eb/crates/tokscale-core/src/clients.rs), [Kimi session parser](https://github.com/junhoyeo/tokscale/blob/0d620fbe331c3e54c45e277320e655d6978755eb/crates/tokscale-core/src/sessions/kimi.rs), [Grok session parser](https://github.com/junhoyeo/tokscale/blob/0d620fbe331c3e54c45e277320e655d6978755eb/crates/tokscale-core/src/sessions/grok.rs).

## Official Upstream Checks

- Kimi CLI pinned docs: [sessions guide](https://github.com/MoonshotAI/kimi-cli/blob/4a550effdfcb29a25a5d325bf935296cc50cd417/docs/en/guides/sessions.md) and [wire file implementation](https://github.com/MoonshotAI/kimi-cli/blob/4a550effdfcb29a25a5d325bf935296cc50cd417/src/kimi_cli/wire/file.py). The wire file is appendable JSONL with a metadata line and persisted records; the usage fields used by the parser are observed in the Kimi Code local artifacts and `tokscale` adapter.
- Oh My Pi pinned README: [can1357/oh-my-pi](https://github.com/can1357/oh-my-pi/blob/7b141199d524b859c357fc89654f10b62b9f3df1/README.md). It documents on-disk session resume and the Pi-compatible session engine; the exact usage field contract is corroborated by the `ccusage` Pi adapter and the local OMP files.
- Grok Build current official source at commit `a5727c5960452e7527a154b25cb5bf00cda0545e` confirms both sides of the boundary. The [sessions guide](https://github.com/xai-org/grok-build/blob/a5727c5960452e7527a154b25cb5bf00cda0545e/crates/codegen/xai-grok-pager/docs/user-guide/17-sessions.md#L18-L39) documents local `updates.jsonl` and `signals.json`; [update stamping](https://github.com/xai-org/grok-build/blob/a5727c5960452e7527a154b25cb5bf00cda0545e/crates/codegen/xai-grok-shell/src/session/acp_session_impl/updates.rs#L114-L127) writes `_meta.totalTokens` from an [estimated context counter](https://github.com/xai-org/grok-build/blob/a5727c5960452e7527a154b25cb5bf00cda0545e/crates/codegen/xai-chat-state/src/handle.rs#L426-L433). The exact [usage ledger is not serialized](https://github.com/xai-org/grok-build/blob/a5727c5960452e7527a154b25cb5bf00cda0545e/crates/codegen/xai-chat-state/src/usage.rs#L1-L4), exact turn input/output/cache fields are [`serde(skip)`](https://github.com/xai-org/grok-build/blob/a5727c5960452e7527a154b25cb5bf00cda0545e/crates/codegen/xai-grok-shell/src/session/signals.rs#L121-L130), and the [monitoring guide](https://github.com/xai-org/grok-build/blob/a5727c5960452e7527a154b25cb5bf00cda0545e/crates/codegen/xai-grok-pager/docs/user-guide/24-monitoring-usage.md#L133-L166) exposes exact token channels through external OTEL. This excludes Grok Build from the passive local parser scope.
- Reasonix package metadata: [npm 1.17.17](https://www.npmjs.com/package/reasonix/v/1.17.17). The package/runtime version is identifiable, but the current local session JSONL does not expose usage fields; old `.telemetry.json` sidecars are aggregate summaries rather than per-turn records.

## Reference Limits

Neither reference repository is treated as a compatibility oracle. Their adapters establish useful field mappings and incremental-state/dedup strategies, while this repository's passive-parser onboarding gate remains authoritative for admission, privacy, and tests. In particular, the tokscale Grok positive-delta heuristic is rejected because it treats a mutable context counter as usage.
