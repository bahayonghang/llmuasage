# Research: A bilingual documentation and acceptance handoff

- Query: Identify the smallest coherent bilingual documentation updates for implemented F/S language behavior and the eventual C/T integration, including stale help/locale/sync claims and final validation limits.
- Scope: internal read-only research. No docs/product edits, builds, network calls or real-data operations.
- Date: 2026-10-21
- Active task: C (`10-20-cli-language-command-output`). A (`10-20-terminal-language-acceptance`) remains planning and must wait for F/S/C/T independent acceptance.

## Findings

### Eight owned pages and exact edit anchors

| Page / current anchor | Current issue or missing contract | Smallest coherent change |
| --- | --- | --- |
| `README.md:45` help paragraph, `:47` runtime root, `:87` Common commands | Chinese help is presented as a special help-only feature; no persistent language command/default/profile scope. | Replace that paragraph with default English and saved `llmusage lang zh` / `lang en`, while retaining one-shot `help --zh`. Add the explicit commands to Common commands; explain root scope once beside runtime root. |
| `README.zh-CN.md:43` help paragraph, `:45` runtime root, `:85` 常用命令 | Calls child help “旧版 clap help” and offers only Chinese top-level help; no saved language explanation. | Paired version of English addition. Do not imply a Chinese README changes the installed CLI language. |
| Both READMEs `## Fast path` / `## 最短路径` (EN :50, ZH :48) | Setup starts at init; readers cannot discover language can be set beforehand. | A short optional `llmusage lang zh` example or reference to language paragraph, without making language selection required for init. Keep normal quick-start actions unchanged. |
| Both READMEs `## Reports` / `## 报表`, EN :111 / ZH :109 | Human `All`/`Agent` wording is English-label-specific after C; machine `agents` fields remain unchanged. | Qualify these as aggregate/per-source rows with labels following selected language; retain exact machine JSON field names. No numeric-accounting rewrite. |
| Both READMEs dash paragraph EN :117 / ZH :115 | No shared startup language, no Usage `m` or `y` controls, and `r` wording omits quota fetch behavior. | Add startup-snapshot semantics, link detailed CLI reference, and accurately name `m` email toggle / `y` sync overlay / `r` local data and Usage quota refresh / `R` auto-refresh without quota polling. Only claim T behavior once T evidence passes. |
| `docs/reference/cli.md:3`, `:5` Top-level help, `:14` help semantics | Says help is always English; says command-specific help is legacy Clap. Intro identifies version 2.0.0 while package/README are 2.0.1. | Describe root and nested help/argument diagnostics following saved language; retain `help --zh` as one invocation override. Refresh the existing version label from actual Cargo metadata without changing package versions. |
| `docs/zh/reference/cli.md:3`, `:5` 顶层 help, `:14` semantics | Same stale English-only/legacy claims. | Paired correction; do not preserve an English-only branch in Chinese docs. |
| Both CLI references `## Global options` / `## 全局参数`, line 24 | Home description lacks language-profile scope and clean exception. | Keep precedence `--home` > `LLMUSAGE_HOME` > `~/.llmusage`; explain settings follow resolved root and `clean` ignores env home. Reads/help/version/argument errors do not create the root. |
| Both CLI references line 31, `--locale <LOCALE>` | Claims titles and numeric formatting follow locale. This is not current behavior. | State it remains a compatibility report value accepting `en-US`, `zh-CN`, `ja-JP`; it neither selects CLI/TUI language nor introduces locale-specific number formatting. `lang` is the interface selector, with only en/zh (zh-CN alias). |
| Both CLI references before `## Setup and sync commands` / `## 设置与同步命令`, current line 131 | No `lang` command reference. | Add one `### llmusage lang` subsection with inspect/menu, explicit en/zh/zh-CN, profile example and cancellation/non-TTY behavior; reference it from the help section. Keep settings details here rather than duplicate whole guides. |
| Both CLI references line 129, `### llmusage statusline` | Only “one summary”; no single-line language/cache contract. | Explain interface labels follow lang and caches are language-separated; hook JSON/model/cost inputs and units stay unchanged. No claim that old `latest.txt` is migrated/deleted. |
| Both CLI references `## Runtime logging` / `## 运行时日志`, lines 42–51, plus `### llmusage logs`, :286–294 | File/console filter details are good, but new language projection and historical-original boundary are absent. | One concise paragraph: current CLI console/replay follows saved language; structured records stay canonical; old/unknown/third-party detail stays marked original; stable ID projection does not rewrite files. Keep rotation/filter defaults exactly as written. |
| Both CLI references `### llmusage sync`, :160–178 | Says one aligned table and final `TOTAL`, but omits closed frame/header/total separators and paired headers. Existing data meanings and timing caution remain valid. | Describe the actual ten-column closed grid, TOTAL/合计 according to language, source-grouped fault/informational diagnostics after bottom border. Show an actual isolated fixture example, not hand-spaced Markdown pipes. Keep BYTES/PARSE/WRITE caveats. |
| Both CLI references `### llmusage dash`, :298–306 | Missing shared language and Usage quota key details; alias description already accurate. | Add same-root language snapshot on launch, no live language switch/menu, scope boundary versus browser/desktop, and `m`/`y`/quota r/R semantics after verified T. Keep the alias deprecated and hotkeys unchanged. |
| `docs/commands/index.md:1–5`, `docs/zh/commands/index.md:1–5` | These are intentional relocation stubs, not a second command reference. | Preserve existing links/legacy URL. If changed, add only a brief paired pointer that language/settings are documented in current CLI reference. Do not duplicate the command list or invent a new TUI page. Unchanged stubs are acceptable if no useful content change is needed. |
| `docs/guide/first-sync.md:3/5/11/15`, `docs/zh/guide/first-sync.md:3/5/11/15` | No optional language setup; summary describes six counters but not full ten-column frame/paired labels. | Small language paragraph before import; update output explanation and add fixture grid near first import. Reference detailed CLI definitions rather than repeat all ten metric paragraphs. |
| Both first-sync guides line 36, `## Import one source` / `## 只导入一个来源` Pi/OMP paragraph | Claims one unbounded ordinary sync rebuilds legacy pi rows, contradicting already updated README source row and current S skip-and-warn policy. | Surgical correction: ordinary sync retains/skips legacy pi; explicit `sync --rebuild --source pi` repairs it, then imports omp under its own source as applicable. Do not edit parser behavior or unrelated source promotion narrative. |
| Both first-sync guides `## Rebuild safely` / `## 安全重建`, line 62 onward | Safety policy is largely current; missing default keep, choice keys and complete coverage explanation. | Add concise explanation of preserved events/current/new inputs, physical missing versus still-existing out-of-scope inputs, and unreadable/discovery protection. Keep exact three-stream TTY/flag gate and default keep; do not say these counters are an exact loss forecast. |
| Both first-sync guides `## Emit NDJSON progress` / `## 输出 NDJSON 进度`, :44–52 | Protocol description is good, but language invariance absent. | Clarify that language affects human stderr only; stdout NDJSON keys/codes/values remain machine contracts and no language menu appears. |

### Source-verified F/S behavior to document

- `src/commands/lang.rs::run/choose`: default inspection prints current language; all three streams must be terminals for bilingual `1. English / 2. 中文（简体）` menu. Explicit en/zh/zh-CN saves; zh-CN normalizes to zh. Empty/q/Q/EOF cancels without writing. Non-TTY invocation prints current setting plus explicit commands and reads no stdin. No init/database/worker lock is required. Announce the chosen language only after save succeeds.
- `runtime/preferences.rs`: `<resolved-root>/cli-settings.json` stores the preference, independent `cli-settings.lock` protects atomic updates; unknown object members survive. Missing settings means English regardless of OS locale. Invalid/unreadable settings falls back with a diagnostic and does not rewrite on read. Public user docs need the setting file and scope; OS-lock/recovery internals belong in the spec, not the primary flow.
- `commands/entrypoint.rs:167–233` and `help.rs::command_help`: language is resolved before formal help/parse diagnostics; explicit `help --zh` selects Chinese for that help call without saving. Root and child help follow saved language. Help/version/parse error and read-only lang return before file logging/root creation.
- `src/commands/report_args.rs:58` still has an old docstring claiming locale formats titles/numbers, but the paired help catalog and reviewed language contract are authoritative for presentation. Do not repeat this stale wording in docs. If help output still emits it, return the defect to F/C rather than work around it in A.
- `src/commands/sync_summary.rs:21–34`: English full columns are SOURCE, FILES, CHANGED, SKIPPED, SEEN, COMMITTED, STORED, BYTES, PARSE, WRITE. Lines 86–97 choose Chinese 来源/文件/变更/跳过/读取事件/新增事件/库存事件/读取量/解析/写入. Compact English headers can be SRC/F/CHG/SKIP/SEEN/COMMIT/STORE/BYTES/PARSE/WRITE; Chinese currently keeps the Chinese headers. Numeric cells never shrink; source names may shorten. A very narrow terminal can still be exceeded by complete numeric values.
- Summary lines 133–160 form `┌┬┐`, `├┼┤`, vertical cells, second separator before totals and `└┴┘`. Diagnostics follow bottom border, grouped by source. Labels change, counters/order/units/precision/saturating totals do not. Do not say the sum of PARSE and WRITE is whole-command wall-clock time or physical disk I/O.
- `sync_recovery.rs::question/prompt`: reports preserved/current/new counts and missing/out-of-scope/unreadable/discovery state from existing coverage; unreadable/discovery failures permit only restore access and retry. k/K or empty/EOF/read failure keeps history; r/R accepts loss only within original allowed gate. Two invalid input attempts return keep. Do not infer exact discarded event count from inventory deltas.
- Structured logging is canonical; paired console/replay is keyed by stable event IDs/typed facts, not English-text matching. Unknown/future malformed facts retain inspectable original detail. This includes known-code records with unexpected nested fields.

### Real example source; no existing full rendered grid found

No full ten-column rendered text fixture was found in the eight docs or S's Markdown evidence. Do not fabricate a “captured output” block by retyping numeric data or drawing approximate separators.

There are useful deterministic inputs in `src/commands/sync_summary.rs`:

- `tests::stats` (line 766): codex 1812 files, 18 changed, 1794 skipped, 104070 seen, 2461 inserted, 104050 stored, 7,800,000 bytes, 4100ms parse, 350ms write.
- `tests::summary` (line 782): adds claude (698 files, changed/inserted zero) and absent opencode, exercising placeholders and totals. The remaining inherited fields are intentional test inputs, not realistic reconciling user statistics.
- `bilingual_table_has_closed_equal_width_borders_and_exact_numeric_cells` (line 1155) verifies saturation, equal display width and 11 vertical separators for ten cells in both languages, including an extremely narrow terminal. This stress case is unsuitable as a user-facing normal example.
- `bilingual_diagnostics_are_source_grouped_capped_and_separate_faults_from_information` (line 1199) provides 14 accounting samples, eight retained and six omitted; privacy/offset rules have tests nearby.
- `tests/sync/progress_io.rs::sync_summary_table_is_stdout_only_without_ansi_or_completion_sentence` (line 109) already uses a temporary codex fixture (123 tokens) through the real command. Prefer a sanitized command capture based on the existing fixture harness when A is active, with pricing refresh disabled and no real homes/remotes. Label output as an isolated fixture example. Exact fixture input, command/environment and actual stdout should be recorded in A evidence.

Use one paired table example in the first-sync guide; CLI reference can describe/link it instead of copying identical long blocks into four pages. If paired examples use the same fixture, differences must be only translated labels and display widths, not counters.

### Small coherent language documentation shape

Use one authoritative language subsection in each CLI reference. The README carries discovery/examples and links; first-sync carries optional setup plus output/risk interpretation. The legacy command stubs remain pointers. A compact accurate example set is:

```text
llmusage lang
llmusage lang zh
llmusage lang en
llmusage --home <PROFILE> lang zh
llmusage --home <PROFILE> daily --help
llmusage help --zh
```

State once that this covers CLI help, arguments, human command/log/report text and new dash/tui sessions. The browser/desktop dashboard and exported HTML language remain independent. Command names/flags/enum values/model/user labels, JSON/NDJSON/SSH/HTTP data and numerical semantics remain unchanged. Historical/third-party original detail may remain original with a local context/marker; do not promise every stored String will be translated.

### Spec consistency (main coordinator/A ownership)

- F `cli-language-contracts.md` is already current; S `runtime-log-contracts.md` and `source-sync-contracts.md` already document canonical events, scope and strict metadata. Audit and extend them, do not replace their sections with a second policy.
- `runtime-log-contracts.md`, Commands That Must Not Create The Runtime Home, validation table still literally expects Chinese `没有可清理内容` independent of language. Make the expectation paired after C acceptance while preserving no-create behavior.
- C has already updated `report-cli-contracts.md` with display width, numeric semantics and statusline language-cache contract. Reconcile with actual C evidence; no need to reimplement the same spec update.
- T owns removing English-only requirements from `tui-presentation-contracts.md:46/104/135` and updating private optional quota cache presentation metadata in `tui-subscription-contracts.md:38` (currently “report only”). A checks consistency against actual T code/evidence, including startup snapshot/shared hit width and unchanged public DTOs. Do not document still-planned T internals as shipped before T gate passes.
- Keep `tui-runtime-contracts.md` dirty redraw/window/generation/theme invariants. Any new private hint guard must be described as implemented, not as an assertion that it existed before this work.

## Final gate and evidence limits

- `justfile:101` ci recipe runs `check-ci-gate.py --self-test`, `check-ci-gate.py`, `ci-rust.py`, `ci-js.mjs`, `just desktop-check`, docs build. Keep this recipe and lockfiles unchanged. It is not MSRV/audit/semver verification.
- A's focused commands are CLI/TUI targets, docs build, then complete `just ci`. Root Rust + docs is cross-surface, so a docs-only build cannot certify the full task.
- S's `research/evidence.md` final gate records 1253 passed/19 ignored plus fmt/clippy/rustdoc, on an earlier S snapshot. That is stage evidence, not current C/T/A PASS. There is no complete final cross-surface gate result yet.
- Existing S evidence explicitly preserves an unresolved Web Local-versus-requested-timezone relative-window issue. One response-shape test fixture was corrected to an interior prior-day bucket; Web production behavior was not fixed. Do not claim this language project solved it, or weaken broader date-policy tests to make final CI green.
- Native ordinary-window CLI/TUI English/Chinese smoke remains separate from headless TestBackend/subprocess tests. Record PASS/FAIL/NOT_RUN with command/fixture and terminal/screenshot evidence. If native interaction is unavailable, retain NOT_RUN explicitly; compiling, pseudo-terminal execution or a docs example is not native acceptance.
- Do not run real sync/rebuild, quota calls, credential discovery, SSH updates, install, release or commits to obtain examples. All test fixtures use isolated homes/endpoints. Product failures return to F/S/C/T owner; A does not change protocols or business behavior for docs acceptance.

## Related specs and internal references

A PRD/design/implement; parent PRD AC1–AC16 and `research/{surface-coverage,validation-plan}.md`; F CLI language contract; S runtime-log/source-sync contract and final evidence; C report contract/evidence; T handoff in the same C research directory. No external references or public-network verification were required.

## Caveats / Not Found

- T behavior is still a planned dependency at this research boundary. Re-read actual T code and independent results before documenting it as available.
- Existing source lists and some unrelated reference content may have older omissions; this handoff confines edits to language/sync/TUI consistency rather than rewriting the whole CLI guide.
- All line anchors are current reads and may move under ongoing C work. Exact headings and command names are the durable edit anchors.
