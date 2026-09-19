# Upstream checkpoint — 2026-09-19

## Reproducible baseline

| Repository | Local HEAD = live origin HEAD | Commit date | Local state |
| --- | --- | --- | --- |
| ccusage/ccusage | `0663eb9c7aca2c168364eff3949322e4a0f1205c` | 2026-09-19 06:28:28 UTC | clean |
| junhoyeo/tokscale | `d8fd670a46857e5290e71b10245dc522a344fc17` | 2026-09-18 09:20:17 +09:00 | clean |
| llmusage | `d1108b0` | current checkout on `dev` | clean at inspection start |

Read-only `git ls-remote origin HEAD` succeeded for both upstreams after sandbox network access failed at the configured localhost proxy. No fetch, pull, checkout, package install or reference-file edit occurred. The remote equality applies at inspection time, not indefinitely.

Previous repo-owned reference note: `.trellis/tasks/archive/2026-08/08-16-passive-sources-zcode-antigravity-deepseek/research/ccusage-tokscale-reference.md:3`. Its ccusage short hash `25df658c` does not resolve in today's object database; its tokscale entries are several individual feature commits, not one unambiguous HEAD. Therefore this review uses dated history from August 16 through September 19 and inspects final code; it does not claim an exact diff from that older checkpoint. Next review should compare against the two full hashes above, check ancestry, and flag a rewrite if either baseline disappears.

Primary upstream links: [ccusage pinned tree](https://github.com/ccusage/ccusage/tree/0663eb9c7aca2c168364eff3949322e4a0f1205c), [tokscale pinned tree](https://github.com/junhoyeo/tokscale/tree/d8fd670a46857e5290e71b10245dc522a344fc17).

## ccusage findings

| Priority / disposition | Update | Fit against llmusage |
| --- | --- | --- |
| P0 selected investigation | `c951e20d` Aug 31 Antigravity native SQLite adapter; `416af6e7` Sep 16 effort variants | `rust/adapters/antigravity/src/parser.rs:648` interprets #1 as model id and #9/#10 as reasoning/visible output, contradicting current llmusage/tokscale. Supports steps/retries and multi-identity dedupe. Native local IDE DBs exist; prioritize semantics correction and SQLite-first IDE. See native-sqlite-findings.md. |
| P1 follow-up candidate | `809eeb6d` Aug 29 Pi fork-prefix suppression; `a26f5173` Sep 15 excludes generated subagent artifacts | `rust/adapters/pi/src/loader.rs:103,147,198,247` in ccusage. Current `src/parsers/pi.rs:445,495` has path/offset identity without parent-prefix reconciliation. Valuable independent accounting task; OMP requires its own evidence. |
| P1 follow-up candidate | `02db2834` Sep 18 Claude cross-session request dedupe, after `a4b8420c` Aug 29 | ccusage `rust/adapters/claude/src/lib.rs:251,261` distinguishes request-bearing identity from requestless session/time fallback. Local `src/parsers/claude.rs:582,584,598` already handles request+message and sidechains, but requestless cross-session collision needs a focused fixture audit. Do not replace project replay with global replay blindly. |
| P2 separate pricing investigation | `9762e09a` Sep 16 Astra fast pricing; `4be84d63` Reserve alias; `da31b197` Sep 17 date-sensitive auto-review alias | ccusage `rust/adapters/codex/src/parser.rs:620` and tests `lib.rs:293,544`. Local `src/domain/pricing.rs:89` lacks time/service-tier input. A static alias copy could misprice history; raw identities and temporal rates need separate contracts. |
| Already covered in principle | `15b3bef8` Aug 29 Codex explicit cache-write handling; `36ba9f09` Sep 17 missing-pricing warning | Local `src/parsers/codex.rs:911,1120` handles supplied creation aliases; `src/domain/pricing.rs:17,95` has Unpriced. Do not add duplicate feature work. Existing token spec's broad no-cache-write wording should be reconciled in a future Codex-specific task. |
| Defer source expansion | `033d25ee` Aug 30 OpenCode v2 sessions; `8841f921` Aug 31 / `db803c64` Sep 16 Copilot; `8dd19bb2` Sep 16 OpenClaw SQLite | Requires separate real fixtures/onboarding; not part of Antigravity acceptance. |

Daily pricing snapshots and dependency churn were reviewed as history, not treated as independent product features. The Antigravity adapter is now present; its new interpretation is a primary comparison, but accepted native samples and an independent semantic oracle remain required.

## tokscale findings

| Priority / disposition | Update | Fit / limitation |
| --- | --- | --- |
| P1 selected: CLI dates | `3cba89a0` Aug 26, `151211f5` Aug 26, `678ef199` Sep 7, then `997c08dc` Sep 14 | HEAD adds typed steps lookup, but `generation_timestamp_ms` at :526-540 still runs #9.#10 inference before steps, despite comments at :323 saying modern payloads return None. This comment/runtime drift must not be copied; prefer proven typed associations. |
| Reviewed, not selected: IDE RPC | `b6111126` Aug 24 Windows transport, `3bff91c3` / `3048ae1a` Aug 26 proxy/config isolation and bounded response | Native IDE databases were found locally, so this RPC architecture is outside the selected work. Current README:667 still says macOS/Linux only, despite Windows code. |
| Reviewed, out of current scope: RPC reliability | `97886c3d` Aug 26 optional enrichment budget; `e4441a4b` Sep 5 runtime isolation; `aa041805`, `3e375715`, `75fecc0b`, `7b357d43` Sep 7 latency, response cap, cached timestamps, refused-port behavior | Preserve existing successful cache on failed acquisition; timeout enrichment without discarding validated usage. Test slow responses and cancellation instead of copying arbitrary timeout constants. |
| P2 conditional CLI attribution | `8d2630b1` Aug 18 routing-label recovery | Current explicit `#19` can be a routing label. Prefer directly evidenced concrete model ids / unambiguous in-session association; do not freeze evolving tier-to-model guesses. |
| P2 candidate outside scope | `e7f4906a` Sep 12 Pi fork/continued dedupe; `2cee9040` Sep 14 Codex thread-kind groups; `481be279` Sep 18 Copilot session-store | Pi has corroborating evidence from both upstreams; rank it next after Antigravity. Thread-kind dimensions and new sources need separate tasks. |
| Do not copy automatically | `e0e5be55` Sep 18 Kiro content/credit estimates; `91cea59e` Aug 28 Antigravity quota; `01689508` Sep 10 provider disable | Estimated token split and subscription allowance are different products from exact request token accounting. |

Other observed latest changes: `d8fd670a` additive webpki/native trust; `2fdcf716` / `57a040f9` Sep 15 OpenRouter pricing resolution; `a859ad25` Sep 15 reads CLI service port from log head. They are context, not permission to add networking/pricing/log readers broadly.

## Official-source cross-check

[Google Antigravity changelog](https://www.antigravity.google/changelog), checked September 19, lists separate CLI/IDE/SDK releases and SDK turn/session-level usage reporting with live usage updates. It does **not** certify the reverse-engineered SQLite field numbers, IDE RPC channel overlap, or tokscale's Windows support. Neither SDK counters nor quota percentages establish the IDE historical usage contract.

## Selection

Create two Antigravity children: shared modern SQLite/accounting correction, then native IDE source integration. RPC is outside scope. Keep Pi/Claude dedupe and temporal pricing as ranked follow-ups, without creating an unrelated implementation programme. Do not add either reference project as a runtime dependency. Tokscale is MIT (`ref/repo/tokscale/LICENSE:1`); any substantial copied code/tests must retain the required notice. Prefer independently written adaptations and source-linked fixtures.
