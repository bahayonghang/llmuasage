# Official model facts — GPT-6 Astra and Claude Fable 5.1

Checked on 2026-09-05. Prefer the vendor pages over secondary writeups.

## GPT-6 Astra (OpenAI)

Primary sources:

- https://developers.openai.com/api/docs/models/gpt-6-astra
- https://developers.openai.com/api/docs/pricing
- Codex model switcher docs list `codex -m gpt-6-astra`: https://learn.chatgpt.com/docs/models
- Product announcement: https://openai.com/index/gpt-6-astra/

| Field | Value |
| --- | --- |
| Canonical API / Codex id | `gpt-6-astra` |
| Published snapshots / aliases | only `gpt-6-astra` (self-alias) |
| Context window | 1,050,000 |
| Max output | 128,000 |
| Knowledge cutoff | 2026-04-30 |
| Release | staged GA 2026-09-03 (Trusted Access first) |

Standard text-token rates, USD per million tokens:

| Channel | Short context | Long context (`prompt_tokens > 272_000`) |
| --- | ---: | ---: |
| Input | 10.00 | 20.00 |
| Cached input | 1.00 | 2.00 |
| Cache writes | 12.50 | 25.00 |
| Output | 50.00 | 75.00 |

Long-context rule matches the GPT-5.6 catalog: 2× input and cache channels, 1.5× output, applied to the full request. Cache writes are 1.25× uncached input.

Out of catalog MVP:

- Batch / Flex = 50% of Standard
- Fast mode = 2× applicable rates
- Regional processing 10% uplift
- Tool-call fees (web search, computer use, containers)
- Unofficial Codex leak id `gpt-6-astra-aeon` (no public rate card)

Do not invent a `gpt-6` alias. Official docs do not list it.

## Claude Fable 5.1 (Anthropic)

Primary sources:

- https://platform.claude.com/docs/en/models/fable-5-1/overview
- https://platform.claude.com/docs/en/models/fable-5-1/whats-new-fable-5-1
- https://www.anthropic.com/claude-fable-and-mythos-5-1

| Field | Value |
| --- | --- |
| Claude API / Google Cloud / Foundry id | `claude-fable-5-1` |
| Amazon Bedrock id | `anthropic.claude-fable-5-1` |
| Context window | 1,000,000 at standard per-token pricing (no 272K long-context multiplier) |
| Max output | 128,000 |
| Released | 2026-09-01 |
| Default effort | `high`; adaptive thinking always on |

Rates, USD per million tokens:

| Channel | Rate |
| --- | ---: |
| Base input | 10.00 |
| Output | 50.00 |
| 5m cache write | 12.50 |
| 1h cache write | 20.00 |
| Cache read | 0.25 |

Cache read is the only rate that changed versus Claude Fable 5 (`1.00` → `0.25`, 0.025× base input). Input, output, and cache-write rates are unchanged.

The main Anthropic pricing table still lists Claude Fable 5 / Mythos 5. Fable 5.1 rates live on the model page and the 5.1 announcement, not yet as a distinct row on `about-claude/pricing` when checked.

## Claude Mythos 5.1

Same specs and pricing as Fable 5.1. API id `claude-mythos-5-1`. Project Glasswing only.

User decision 2026-09-05: include Mythos 5.1 in this task.
