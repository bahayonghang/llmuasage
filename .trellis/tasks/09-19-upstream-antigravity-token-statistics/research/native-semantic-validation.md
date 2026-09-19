# Native Antigravity semantic validation — 2026-09-19

## Decision

E1 has independent native schema evidence for both products. `ModelUsageStats.#1` is a model enum, not a token count; `#9` is reasoning and `#10` is visible response output. Native SQLite and step metadata provide request identity and real timestamps. Prefer individual `retry_infos` usage whenever present, then deduplicate generation/step mirrors by their stable identities. Do not sum a generation's direct usage together with its retry entries.

The fixture is `research/sanitized-native-usage.json`. Its `primary_descriptors`, `samples`, `native_conversation_identity`, `aggregate_retry_discrepancies`, and `full_native_oracle` sections distinguish schema, accepted native examples, and complete aggregate evidence. Product code was not modified by this research.

## Primary-source proof

Installed CLI `C:\Users\lyh\AppData\Local\agy\bin\agy.exe --version` returns **1.2.5**. SHA-256: `fbb352d7a1c76d4681b2e0aac02618152b873ceaccf1dd5c6ef1afb5ca6a24d6`. Its embedded protobuf `DescriptorProto` records were decoded with the existing Python `google.protobuf` library; no dependency installation was needed.

| Native descriptor | Binary byte offset | Encoded length |
| --- | ---: | ---: |
| ModelUsageStats | 62951964 | 1265 |
| ChatModelMetadata | 63069088 | 1515 |
| ChatStartMetadata | 63071105 | 767 |
| CortexStepMetadata | 63072666 | 2626 |
| RetryInfo, Cortex form | 63068682 | 403 |

Independent IDE corroboration: installed Antigravity IDE **2.5.5**, editor version **1.107.0**, product commit `ecfbad74d93962fc8ca485d93ab9b4f3d4cb6cf8`, product date `2026-08-13T08:37:22Z`. `C:\Users\lyh\AppData\Local\Programs\Antigravity IDE\resources\app\out\jetskiAgent\main.js:3658` embeds base64 `FileDescriptorProto` values for `exa/codeium_common_pb/codeium_common.proto` and `exa/cortex_pb/cortex.proto`; decoded declarations agree. This file's SHA-256 is `81b8db7396f8ea29604597906d81c820907f99952e7852d8dd55809bbc29d8cc`.

Reproduction: locate the CLI's length-delimited descriptor whose name field is `ModelUsageStats`, decode its preceding protobuf length, and parse precisely that message with `DescriptorProto.FromString`. For the IDE, decode the base64 argument of `wt=go(...)` or `$e=go(...)` and parse `FileDescriptorProto`. These are installed product schemas, not names inferred from ccusage or tokscale.

| Field | Native schema name/type | Accounting meaning |
| --- | --- | --- |
| #1 | model / Model enum | No token contribution |
| #2 | input_tokens / uint64 | Fresh input |
| #3 | output_tokens / uint64 | Total output, including reasoning |
| #4 | cache_write_tokens / uint64 | Cache write |
| #5 | cache_read_tokens / uint64 | Cache read |
| #9 | thinking_output_tokens / uint64 | Reasoning |
| #10 | response_output_tokens / uint64 | Visible response output |
| #7 / #11 / #12 | message_id / response_id / provider_assigned_message_id | Stable request identities |

Enum values 1026, 1132, and 1318 are `MODEL_PLACEHOLDER_M26`, `MODEL_PLACEHOLDER_M132`, and `MODEL_PLACEHOLDER_M318`. Their names do not establish a stable commercial-model alias; use actual response-model metadata rather than treating this research as a new alias table.

`ChatStartMetadata.#4` is `google.protobuf.Timestamp`; **#10 is ContextWindowMetadata, not an opaque timestamp**. A parser must not reinterpret its bytes as epoch time. `CortexStepMetadata` carries real created/started/completed timestamps and `model_usage` at #9; its retry entries are #28. `ChatModelMetadata` usage is #4 and retry entries #17; `RetryInfo.usage` is #2.

## Native reading and privacy boundary

Queries selected only `gen_metadata(idx,data)`, `steps(idx,step_type,status,metadata)`, and `trajectory_meta` identity/source fields. No `step_payload`, `error_details`, permissions, render information, prompt/response columns, credentials, full logs, or RPC were read. Nested prompt/tool/response-header/error strings inside metadata were skipped without decoding or persistence.

Connections used SQLite URI `mode=ro`, `PRAGMA query_only=ON`, and `BEGIN` snapshots, with existing WAL visible. The initial sandbox returned `SQLITE_CANTOPEN: unable to open database file` for two selected WAL databases. The exact read-only queries succeeded after task-authorized escalation. This was an access boundary, not evidence of corrupt data. Subsequent complete passes reported no SQLite errors. No `immutable=1`, database copy that discards WAL, checkpoint, sync, rebuild, or data mutation was used.

Fixture protobufs were rebuilt from a closed allowlist of usage, model, timestamp, and numeric linkage fields. All request and conversation identities are SHA-256 hashes. Time fields were shifted by whole days to 2026-01-01 while preserving time of day and relative intervals. No original metadata blob or freeform error text is retained.

All **13** accepted sample fixtures passed a recursive field-allowlist check, hashed-identity format validation, protobuf decoding, and exact-channel mirror verification. Each has `expected_unique_channels`, including expected request count, computed with the independent native-descriptor protobuf runtime after retry precedence. Individual samples are separate test cases; some intentionally reuse the same native request to exercise generation-plus-step and step-only entry paths.

## Retry semantics and accepted examples

The native normal fixtures show the same request three ways: generation direct usage, a `retry_infos` entry, and a step usage entry. These represent one event. The field name `retry_infos` does not mean a retry necessarily occurred: a successful first attempt normally has a one-entry list.

Actual multi-attempt examples are saved for both products. Numeric tuples below are `(input, total output, cache read)`:

| Product | Direct generation usage | Attempt 1, error present | Attempt 2 | Expected distinct attempts |
| --- | --- | --- | --- | ---: |
| CLI | (42558, 9404, 48897) | (40273, 8032, 0) | (2285, 1372, 48897) | 2 |
| IDE | (6343, 1284, 122506) | (2482, 1169, 61256) | (3861, 115, 61250) | 2 |

In these examples direct usage equals the sum of both attempts while retaining the first attempt's identity. Both individual attempts independently appear in step metadata. A component-maxima merge of direct usage with attempt 1, followed by adding attempt 2, therefore overcounts.

The full CLI pass found all 2,304 generation aggregates equal the sum of retry entries. IDE found 9,500 of 9,502 equal; **two direct usages contain only the last attempt**. One omitted failed attempt has 59,993 input and zero output; the other omitted attempt has 576 input, 3 visible output, and 74,032 cache-read tokens. Both omitted attempts are present in their retry lists and step records. Exact numeric discrepancies and hashed identities are saved. This demonstrates why retry-entry precedence is correct and why equality of direct usage to the retry sum cannot be imposed as a validity requirement.

Native fixture coverage includes normal generation/step mirrors, steps-only shape, real error-status steps without usage, empty databases, true multi-attempt retry usage, and a CLI enum-only zero-usage record. The enum-only record must not generate model-number tokens. Positive input-only failed attempts remain countable. Proto3 omitted output fields mean zero, not malformed data.

## Stable conversation and product identity

`trajectory_meta` has `trajectory_id TEXT PRIMARY KEY`, `cascade_id TEXT`, `trajectory_type INTEGER`, and `source INTEGER`. Every inspected database has one identity row. `cascade_id` equals the filename stem; `trajectory_id` differs and must not be assumed to be the filename identity.

The installed `CortexTrajectorySource` enum defines 17 as `CORTEX_TRAJECTORY_SOURCE_CLI` and 1 as `CORTEX_TRAJECTORY_SOURCE_CASCADE_CLIENT`. Native CLI files all use source 17 and IDE files source 1; both use trajectory type 4 (`CORTEX_TRAJECTORY_TYPE_CASCADE`). These source values provide product provenance when a file is copied, while filesystem roots remain discovery paths.

No same cascade-ID filename or request identity was shared across the two native roots during the complete scans. No distinct databases within either root shared a request identity. This verifies the observed corpus only; synthetic copy/root-overlap tests remain necessary. The old/backup roots were outside this evidence scan.

## Complete independent oracle

The final oracle reads **every** generation and step row without row/sample caps, using a read-only transaction per database. It constructs a Python protobuf runtime message directly from the installed native `ModelUsageStats` descriptor. Retry entries replace direct cumulative/final usage when available. All message/response/provider identifiers connect equivalent observations. Components require exact six-channel equality; the oracle does not take component maxima. Zero records are excluded; positive records without identity, without time, and conflicting components are reported explicitly.

| Metric | CLI | IDE |
| --- | ---: | ---: |
| Databases | 110 | 529 |
| Generation rows | 2310 | 9541 |
| Step rows | 4896 | 21791 |
| Positive candidate occurrences after retry precedence | 4700 | 19567 |
| Unique positive request events | 2395 | 10043 |
| Generation/step mirror events | 2305 | 9524 |
| Step-only events | 90 | 519 |
| Positive idless events | 0 | 0 |
| Events without real timestamp | 0 | 0 |
| Identity components with conflicting token channels | 0 | 0 |
| Input | 17617451 | 91616758 |
| Visible output | 331995 | 3100587 |
| Reasoning | 348159 | 5854179 |
| Total output | 680154 | 8954766 |
| Cache read | 80035407 | 476058002 |
| Cache write | 0 | 0 |
| Total tokens | **98333012** | **576629526** |

`total = input + cache_read + cache_write + output_total`; `output_total = visible_output + reasoning`. The earlier complete bounded pass observed 36,565 raw usage occurrences with the output identity true in every occurrence. Cache-read exceeded input in 31,411 occurrences, independently supporting a fresh-input interpretation rather than subtracting cache from #2.

Exact UTC start/end, hashed per-database identities, channel sums, counts, and DB/WAL modification fingerprints are under `full_native_oracle` in the JSON. The IDE is actively creating databases during research (526, 527, and 529 in consecutive passes). These totals are a dated per-database snapshot, not immutable current totals. A later isolated importer must reconcile unchanged databases against their saved fingerprints, or regenerate this independent oracle against the same stable inputs; growth must not be reported as an accounting regression.

## Coverage limits

Positive cache-write values were not present in the native corpus. Its field mapping is independently schema-proven, but positive cache-write arithmetic needs a clearly labeled synthetic fixture. Error-status-without-usage and positive input-only error/retry examples are real; arbitrary transport/corruption/failure modes are not established by them. Pricing correctness and placeholder-to-commercial-model aliases are not proven by this token oracle.

All evidence here is Windows native reading and metadata analysis. Cross-platform behavior, root-copy ownership, WAL-only change detection, cancellation, parser history rebuild, UI consistency, and the integrated llmusage import are implementation acceptance work, not established by this research.

## Native fingerprint failure probe

During implementation, the first isolated native CLI import reported 84 `database changed while reading` issues. Those counts can include conservatively attributed failures from IDE files inspected during the same family scan; they do not prove that 84 idle CLI databases changed.

A bounded read-only reproduction compared each database and WAL's existence, length, modification nanoseconds, SHA-256 of its first 4096 bytes, and SHA-256 of its last 4096 bytes. Three oldest CLI databases were each checked across two transactions at open, BEGIN, full generation/step metadata SELECT, ROLLBACK, and close: **zero fingerprint components changed in all six transactions**. A further four transactions used the Windows extended canonical path form (`\\?\...`), one idle empty-WAL and one positive-WAL database per product, and included trajectory metadata reads: again **zero changed components**. At this probe, CLI WAL classes were 104 empty and 6 positive; IDE classes were 528 empty and 2 positive. No raw paths or content were printed.

These observations do not reproduce reader-induced WAL modification and do not establish corruption, checkpoint side effects, or a reason to ignore empty-WAL timestamps. Python's SQLite is not necessarily the same build as bundled rusqlite. The parser implementation has moved the pre-read fingerprint from family inventory time to immediately before the individual database transaction; the next rebuilt isolated import must verify whether that change resolves the original symptom. The cause of the original failure remains **UNVERIFIED** until that test.
