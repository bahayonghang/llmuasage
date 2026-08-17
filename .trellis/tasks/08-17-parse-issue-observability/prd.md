# Parse issue 分类纠偏与可观测性

## Goal

让 parse issue 可解释、可信任。CLI、doctor、source-status、diagnostics、看板、TUI 共用四类计数。跳过行和记账异常不再伪装成解析失败。Codex 超大行按前缀分类；能回收 token_count 时必须入库。

## Background

2026-08-17 sync：Codex oversized=17；Zcode malformed=17 且 SEEN=COMMITTED=STORED=1610。用户要求彻底修复、相关面都改。
JSONL 单行上限 4 MiB。Kind 只有 Malformed 与 Oversized。Zcode 把未完成行记成 malformed。看板与 TUI 不展示 parse issue。

## Requirements

### R1 四类互斥

- malformed：坏 JSON，或无法形成事件。
- oversized：超过 4 MiB 且用量可能丢失。
- skipped：故意不导入（Zcode 未完成；Codex 超大行非 token_count）。
- accounting_anomaly：数字不一致但事件仍入库。

### R2 Codex

不提高 4 MiB 上限。非 token_count 记 skipped。前缀完整 token_count 入库且不记 issue。无法回收则 oversized。

### R3 CLI

sync 按四类打印非零项。警告色只用于 malformed/oversized。样本：kind、offset、basename。

### R4 doctor / source-status / diagnostics

同一套字段。doctor 仅在 malformed+oversized>0 时 warn。source-status 对有计数的来源加一行。

### R5 看板与 TUI

SyncSourcePayload 带四类计数不含样本。故障时 tone=warn，不覆盖 source error。看板来源卡与 TUI 宽表展示非零计数。

### R6 兼容

不改 token 公式。旧 JSON 缺新字段当 0。cursor、取消、4 MiB 不回归。

## Acceptance Criteria

- [ ] AC1 Zcode completed 全导入；未完成行 skipped；记账异常 anomaly 且事件仍在。
- [ ] AC2 Codex 10 MiB 非 token_count 为 skipped=1，后续行仍解析，缓冲不超过 4 MiB。
- [ ] AC3 前缀完整 token_count 加空白填满：事件入库，四类为 0。
- [ ] AC4 无法回收的 token_count 前缀：oversized=1，后续行仍解析。
- [ ] AC5 sync 人读按四类打印；Zcode 跳过不再出现 malformed=。
- [ ] AC6 doctor 只在故障时 warn；diagnostics 含四类计数。
- [ ] AC7 source-status 打印摘要。
- [ ] AC8 dashboard 与 TUI 可见四类计数。
- [ ] AC9 样本无源记录正文；CLI 最多 basename。
- [ ] AC10 既有 bounded JSONL / Zcode / Antigravity / sync center 测试不回归。
- [ ] AC11 更新 source-sync-contracts.md。

## Out of Scope

- 不调大 4 MiB 默认上限。
- 不做通用残缺 JSON 修复。
- 不给 OpenCode 新造 parse issue。
- 不把样本打进交互 dashboard payload。

## Key Decisions

- 全量表面都改。
- 非用量超大行记 skipped，不记 oversized。
- 成功回收的 token_count 不记 issue。
- doctor 只把 malformed/oversized 当故障。
