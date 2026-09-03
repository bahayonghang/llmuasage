# Antigravity 打开失败不得清空历史

## Goal

Antigravity conversation DB 打不开或 `gen_metadata` prepare 失败时，保留已导入用量，不把该文件标成“已成功重扫”。

## Background

`parse_conversation_file` 在 `Connection::open_with_flags` 失败时记一条 Malformed 并 `return Ok` 空结果（`src/parsers/antigravity.rs:345-361`）。`prepare("SELECT rowid, data FROM gen_metadata...")` 任意 `Err` 被当成“无表”（`:371-378`）。调用方 `parse_antigravity_file` 在已有 cursor 时仍 `reset_path_hashes.push` 并 `finalize_cursor`（`:309-325`）。下一轮 fingerprint 匹配则跳过（`file_state.rs:75-79`）。SQLITE_BUSY / CORRUPT / 权限错误会删掉该会话已导入行且不再重试。

## Requirements

- R1. 打开 conversation DB 失败：不产生 `reset_path_hashes`，不写成功 cursor，文件计为失败/malformed，已有 `usage_event` 保留。
- R2. `prepare` 失败且错误不是“表不存在”：与 R1 相同。表确实不存在的空会话仍可干净跳过。
- R3. 成功打开且读到行时，现有“整文件 reset + 重解析”语义不变。
- R4. 回归测试：已导入行 + 随后打开失败（或 prepare 失败）→ 行数不变，cursor 不前进到“可跳过成功态”。

## Acceptance Criteria

- [x] AC1. 打开失败路径不调用 `reset_path_hashes` / 不 `finalize_cursor` 为成功 fingerprint。
- [x] AC2. 非“缺表”的 prepare 错误不重置该 path 的事件。
- [x] AC3. 测试覆盖打开失败与缺表跳过两条分支。
- [x] AC4. 不改其他源 parser 的 replay 协议。

## Out of scope

- OpenCode/ZCode 打开模式（`09-03-parser-sync-robustness`）。
- 改 `commit_shard` 协议。
