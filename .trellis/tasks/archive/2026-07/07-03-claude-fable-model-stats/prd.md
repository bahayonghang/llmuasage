# 添加 Claude Fable/Mythos 模型统计

## Goal

让 `llmusage` 把 Claude Fable 5 和 Claude Mythos 5 识别为已知 Claude 模型：新导入或重建后的 Fable/Mythos
usage 不再落到 `unpriced` / unknown-context，模型分布、成本估算、上下文压力等现有统计面板能正常展示。

本任务是模型 catalog/统计覆盖，不是新增数据源或改 parser 采样边界。

## Evidence

### 官方模型事实（2026-07-03 核对）

- Claude Platform models overview lists Claude Fable 5 with API ID / alias
  `claude-fable-5`, Bedrock ID `anthropic.claude-fable-5`, Google Cloud ID
  `claude-fable-5`, and marks it as Fable 5's released model ID. The same page lists
  Claude Mythos 5 with API ID `claude-mythos-5` and says it shares Fable 5's specs
  and pricing, with limited availability through Project Glasswing.
  Source: https://platform.claude.com/docs/en/about-claude/models/overview
- Official pricing lists Claude Fable 5 and Claude Mythos 5 at:
  - base input: `$10 / MTok`
  - 5m cache writes: `$12.50 / MTok`
  - 1h cache writes: `$20 / MTok`
  - cache hits / refreshes: `$1 / MTok`
  - output: `$50 / MTok`
  Source: https://platform.claude.com/docs/en/about-claude/pricing
- Official docs say Fable 5 has 1M-token context at standard pricing and uses the newer
  tokenizer introduced with Opus 4.7, producing roughly 30% more tokens than pre-Opus-4.7
  models for the same text. Release notes state Fable 5 and Mythos 5 both support 1M
  token context, 128k max output tokens, and always-on adaptive thinking. Source:
  https://platform.claude.com/docs/en/release-notes/overview
- Anthropic announced Fable/Mythos access restoration on July 1, 2026 after the June 30 export-control
  update. Source: https://www.anthropic.com/news/redeploying-fable-5

### Repo facts

- `pricing/static-v1.json` currently has Claude rows for Opus, Sonnet, and Haiku, plus an
  OpenCode Claude-Sonnet row. It has no Fable or Mythos matcher.
- Claude parser behavior already stores whatever model string appears in the Claude JSONL
  (`src/parsers/claude.rs` -> `normalize_model(...)`). A `claude-fable-5` or `claude-mythos-5` log should already
  become a model row; the missing part is "known model" pricing/window coverage.
- Cost calculation and context pressure both route through `PricingCatalog::static_v1()`.
  Unknown models are represented as `pricing_status = "unpriced"` and excluded from
  context-pressure ratios.
- `UsageTokens` currently has one aggregate `cache_creation_tokens` field. Claude parser sums
  `cache_creation_input_tokens`, `cache_creation_input_tokens_5m`, and
  `cache_creation_input_tokens_1h`, so static pricing cannot distinguish Fable's 5m and 1h
  cache-write rates without a larger token-schema change.

## Scope Decision

User decision on 2026-07-03: include Claude Mythos 5 (`claude-mythos-5`) in this task despite
its limited availability, because it shares Fable 5 specs/pricing and should not become a
pricing/context blind spot when present in local logs.

## Requirements

1. Add Claude Fable 5 and Claude Mythos 5 to embedded static pricing coverage for at least `source = "claude"`.
2. Include OpenCode/Anthropic-style Fable/Mythos IDs when they can be covered without changing source
   semantics, because the static catalog already supports OpenCode provider-model pricing.
3. Use official Fable/Mythos rates in the static row:
   - `input_per_mtok = 10.0`
   - `cached_per_mtok = 1.0`
   - `cache_creation_per_mtok = 12.5` as the current single-rate approximation for aggregated
     cache creation tokens
   - `output_per_mtok = 50.0`
   - `context_window = 1000000`
4. Preserve existing model names in stored events. Do not rename Fable/Mythos rows into Opus/Sonnet
   buckets and do not add a new `SourceKind`.
5. Keep the change local-first. Do not add remote pricing fetches or API calls.
6. Document the cache-creation limitation in code comments or task notes if implementation uses
   the 5m write rate while the current schema cannot separate 1h writes.
7. Add focused tests proving Fable and Mythos are no longer unpriced and have known context windows.

## Acceptance Criteria

- [ ] `PricingCatalog::static_v1().find("claude", "claude-fable-5")` returns a row with the
      official base input/output/cache-hit rates and `context_window = 1_000_000`.
- [ ] `PricingCatalog::static_v1().find("claude", "claude-mythos-5")` returns a row with the
      same official rates and `context_window = 1_000_000`.
- [ ] Static catalog matching covers expected Fable/Mythos shapes:
      `claude-fable-5`, `claude-mythos-5`, short exact-ish aliases chosen in design, and provider
      prefixed OpenCode/Anthropic IDs chosen in design.
- [ ] `compute_cost("claude", "claude-fable-5", ...)` returns `pricing_status = Static`,
      `pricing_source = Some("static-v1")`, non-zero cost, and a `pricing_rate` containing the
      Fable rates.
- [ ] `compute_cost("claude", "claude-mythos-5", ...)` returns the same static pricing behavior.
- [ ] Dashboard context pressure treats Fable/Mythos as known-window models and uses 1M tokens as the
      denominator.
- [ ] Existing Opus/Sonnet/Haiku/GPT/Gemini pricing tests still pass; no matcher overlap causes
      accidental repricing of existing models.
- [ ] A parser/sync/report fixture proves Claude Fable and Mythos logs can land in model/cost reporting
      without becoming `unpriced`.
- [ ] No schema migration is introduced for this MVP.
- [ ] Validation passes: `cargo fmt --check`, focused pricing/query/parser tests, and
      `cargo test -- --test-threads=1`. Run `cargo clippy --all-targets --all-features -- -D warnings`
      before finishing implementation.

## Out Of Scope

- Splitting cache creation into 5m and 1h token columns. That would require parser, schema,
  storage, query, report, and dashboard changes and should be a separate task if exact Fable/Mythos
  prompt-cache billing is required.
- Adding remote pricing refresh. `doctor --refresh-pricing <file>` intentionally accepts only a
  local snapshot.
- Adding a new Claude source parser. The current Claude parser already reads local Claude JSONL.
- Changing historical rows automatically during binary startup. Existing rows imported before
  this task may need `sync --rebuild` to be repriced from local logs.

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
