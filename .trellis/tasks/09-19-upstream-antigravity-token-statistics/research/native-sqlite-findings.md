# Native SQLite findings — 2026-09-19

## Bounded read-only metadata probe
Only directory extension counts, SQLite schemas/counts and numeric protobuf field aggregates were output. No filenames, conversation IDs, prompts/responses, credentials or raw blobs were saved. Opened using SQLite URI mode=ro and PRAGMA query_only=ON; no import/update/rebuild/checkpoint or RPC. This is structure evidence, not complete fixture acceptance.

| User-home root | Counts |
| --- | --- |
| .gemini/antigravity-cli/conversations | 110 .db, 106 .db-wal, 106 .db-shm, 1 .pb |
| .gemini/antigravity-ide/conversations | 519 .db, 75 .db-wal, 75 .db-shm |
| .gemini/antigravity/conversations | 11 .db, 98 .pb |
| .gemini/antigravity-backup/conversations | 100 .pb |
| .config/antigravity/conversations | absent |

Selected first two and last six DBs by mtime per DB family; queried at most 12 latest gen_metadata rows per DB. Some errors occurred after partial progress.

| Family | Selected | Decoded usage blobs | Numeric field #1 | Errors |
| --- | --- | --- | --- | --- |
| CLI | 8 | 22 | 1026 in13; 1132 in8; absent in1 | 4 OperationalError |
| IDE | 8 | 11 | 1318 in11 | 7 OperationalError |
| old antigravity | 8 | 0 | none | 8 OperationalError |

Exception classes do not establish corruption, loss or precise cause. Diagnose access/schema errors during sample acceptance; not all selected DBs succeeded.

Observed gen_metadata(idx,data,size) and steps(idx,step_type,status,has_subtrajectory,metadata,error_details,permissions,task_details,render_info,step_payload,step_format). Only gen_metadata blobs and counts were queried; step payload/error text was not selected. #3=#9+#10 held in21/22 CLI blobs (one lacked ordinary usage channels), and11/11 IDE blobs. This equality cannot identify which summand is reasoning.

## Critical field disagreement
| Field | llmusage / tokscale | Current ccusage |
| --- | --- | --- |
| #1 | system-prompt tokens added to input | model ID, no token contribution |
| #2 | fresh input | fresh input |
| #3 | output+reasoning checksum | total output |
| #4 | no imported cache write | cache creation |
| #5 | cache read | cache read |
| #9 | visible output | reasoning |
| #10 | reasoning | visible output |
| #7/#11/#12 | mainly response ID #11 | message/response/provider message identities |

Evidence: src/parsers/antigravity.rs:452,466,532; tokscale crates/tokscale-core/src/sessions/antigravity_cli.rs:297; ccusage rust/adapters/antigravity/src/parser.rs:648 (both under ref/repo). ccusage parser.rs:808 maps1000+ to model placeholders and specifically1318 to a model enum. Local values are consistent with this interpretation, not standalone semantic proof.

P0 investigation: if the modern definition applies, current parser adds a model enum per request and swaps reasoning/output labels; missed cache-write or step/retry usage can also change totals. The old checksum and two decoders implementing the same guessed map cannot independently prove semantics. No production error magnitude is claimed.

## New upstream adapter
[SQLite introduction](https://github.com/ccusage/ccusage/commit/c951e20dbe60155f1e5df63399f2b1217f797346), Aug31; [effort variant fix](https://github.com/ccusage/ccusage/commit/416af6e75a26136d9ad9061b31412767fd3b8eaf), Sep16. July adapter was reverted Aug3, explaining Aug16 historical absence.

ccusage parser.rs:567 reads generation/retries; :598 steps/retries/time/model; :81 multiple identities; loader.rs:34 connected identity merge across DBs. paths.rs:9 covers five DB roots but one source, no CLI/IDE subtype or RPC. Synthetic loader tests :213,316,386 cover model-id exclusion, copies and retries; not native acceptance. Mtime fallback at parser.rs:446 and component maxima must not be copied without semantic review.

## Direction
Native SQLite first; RPC and .pb-only versions outside scope. Accept normal/empty/error/retry fixtures and trustworthy schema evidence, then correct shared decoder and add IDE with proven product ownership. P1 separate-source choice remains open. Accounting version change and protected hook history need a concrete safe repair path (E2), not naive bump-and-rebuild.
