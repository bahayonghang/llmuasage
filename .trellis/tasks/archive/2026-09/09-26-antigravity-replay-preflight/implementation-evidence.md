# Antigravity 预检实施证据

日期：2026-09-28。实现者：`/root/antigravity_preflight_impl`。

## 边界与实现

- 仅修改本任务产品边界：`src/parsers/antigravity.rs`、`src/parsers/antigravity/decode.rs`、`src/parsers/antigravity/tests.rs`、`src/domain/source_diagnostics.rs`、`tests/sync/sources/antigravity.rs`。这些文件原有 diagnostics-contracts 改动均保留。
- `sync_antigravity_family` 保持驱动入口；私有 `FamilyInputs` / `sync_family_with_inputs` 隔离 discovery 列表与 metadata 查询，供不修改用户环境的单元测试使用。生产入口调用原有两产品 discovery 和 `std::fs::metadata`。
- 两个 root 均发现并执行 DB/WAL 有界 fingerprint 后，若非空 selected 集合的每个产品都已失败，在 `decode::read_file` 之前返回。`files_processed`、`changed_files`、`skipped_files`、`bytes_scanned`、events seen/replayed/inserted、write_ms 保持 0。parse_ms 包含本轮 discovery 起始至返回的经过时间。
- 保留 SourceStarted；驱动仍发出 SourceFinished，engine 仍查询并填充已有 stored_events。正常 partial-blocking 路径继续解码两 root，保留 native ownership、复制去重和较强 identity 选择。
- root metadata 的 PermissionDenied / 其他访问错误、非目录 root、walk 失败均记 discovery_incomplete。未知 root 内可能包含另一产品的副本，故保守阻断 FAMILY 两产品，最终只向 selected 来源返回诊断。无跟踪历史的 NotFound root 仍表示空 root。
- 只加载 `host_id='local'` 的 cursor 和 source_file membership。未进入 discovered 的本地跟踪路径经 metadata 分为物理缺失、存在但当前 discovery 未覆盖、metadata 不可读。远端 membership 不进入本地文件探测。
- 显式 rebuild + allow-lossy 仅允许清除缺失或现存但不在发现范围的历史；不豁免 root discovery、metadata 权限或 fingerprint 错误。普通 sync 不接受 loss opt-in。
- 不修改 schema、token-accounting 版本、remote wire、public DTO 或 commit_antigravity_snapshot 原子写入协议。没有真实用户 sync/rebuild/reset、安装、提交。

## 闭集诊断

| code | 条件 / count | action |
| --- | --- | --- |
| `tracked_member_missing` | 对未发现的跟踪路径执行 metadata，返回 NotFound；count 为规范化成员数 | 恢复输入，或显式 `sync --rebuild --source <source> --allow-lossy-rebuild` |
| `tracked_member_out_of_scope` | metadata 成功，但本次 discovery 未包含该路径；count 为规范化成员数 | 恢复受支持输入覆盖，或明确接受对应不可重建历史丢失 |
| `tracked_member_unreadable` | metadata 返回非 NotFound 错误；count 为规范化成员数 | 恢复数据库访问后重试；不提供有损豁免 |
| `discovery_incomplete` | root 访问失败、非目录或不完整枚举；每个失败 root 计 1 | 恢复访问后重试；同时保守阻断可能受未知副本影响的所选产品 |

其余 source issue code 保留。新增 2 个 code 后，one-entry-per-code 上限由 6 改为 8。每个 issue 保留 UTC observed_at 和 product_group scope；不存原始路径、正文或异常自由文本。

## Red / Green

- `preflight-red.log` / `.exit`：先加入真实 read_file 调用计数和断言，尚未加入早退。匹配 1 test，失败 1，exit 101。CLI 单选已阻断却实际调用 decoder **1 次**，期望 0。该失败在第一种选择即终止，未将其他选择宣称为已运行的 red 证据。
- `preflight-green.log` / `.exit`：加入早退后同一回归匹配 1 test，pass，exit 0；函数遍历 CLI、IDE、both 三种选择。
- `preflight-matrix.log` / `.exit`：第一轮扩展匹配 5 tests，5 pass，exit 0。
- `decode::test_reads` 仅在 cfg(test) 编译，按 fixture 的唯一规范路径 opt-in 计数，在 read_file 函数入口实际记录调用。生产二进制没有此计数器。

## 验收对应证据

| AC | 测试 / 观察 |
| --- | --- |
| A1 | `preflight_all_selected_blocked_never_calls_usage_decoder`：CLI/IDE/both 3 种选择；从已导入正数 usage 开始；实际 decoder 调用 0；所有导入工作计数 0；逐行比较 events/buckets/raw/turn/tool/cursors/inventory/accounting meta；UTC 观测范围；SourceStarted 数量；计时用已知起始 Instant 偏移验证，不使用 sleep。 |
| A2 | `preflight_uncovered_members_distinguish_missing_scope_and_access`：missing、旧 JSON、旧 root 文件、PermissionDenied × allow-lossy 两状态，共 8 例；访问失败在显式 lossy rebuild 下仍不写。`preflight_unselected_root_failure_blocks_unknown_product_copies`：CLI/IDE 单选 × 权限/非目录/walk error/NotFound，共 8 例。PermissionDenied 为私有 metadata 函数的确定性注入；非目录和 NotFound 使用真实临时文件系统。 |
| A2/A5 | 集成 `antigravity_preflight_existing_legacy_paths_and_changed_root_preserve_history`：现存旧 JSON count=2、GEMINI_CLI_HOME 改根 count=1；普通与无 lossy rebuild 均保留逐行历史；Store 重新打开后读取 code/count/observed_at/scope；不报告 tracked_member_missing；恢复 root 或明确 lossy 后可继续。 |
| A3 | `preflight_partial_blocking_keeps_cross_root_decode_and_selected_scope`：CLI-only 与 both 两种选择；native CLI/IDE 放入相反 discovery root，确认实际两个 reader 各调用 1 次，成功组继续、失败组不写。现有 stronger-identity-unselected-copy、native copy、conflict、WAL-only、Windows cursor、bounded、busy、cancel 测试均通过。 |
| A3 | `preflight_empty_selection_and_cancellation_do_not_decode_or_write`：空选择与预取消不解码、不写；`preflight_fingerprint_failure_does_not_accept_lossy_rebuild`：未选中 root 内新 DB 在发现后消失，阻断所选产品并保持全表；`preflight_remote_membership_never_probes_local_filesystem`：remote-only 路径若进入 metadata probe 则测试直接失败，实际成功导入本地来源。 |
| A4 | 增强 `antigravity_missing_group_member_preserves_bounded_and_forgotten_history`：完整/bounded 首次导入、missing/forgotten 旧成员、后续完整与 bounded 阻断、no-lossy rebuild 拒绝、恢复后 input 总数从旧快照 300 更新为 400，source issue 清除，再次缺失时显式 allow-lossy 保留可重建成员。现有 full rebuild hook-history 测试通过。 |
| A5 | 增强 missing-group 集成逐行比较 events/buckets/cursors/inventory/marker，不修改 last_seen_at；校验 SourceStarted/Finished 各 1 次、stored_events=2、没有 TokenAccountingRepairFinished、blocked 工作计数全 0。UTC observed_at 在测试请求时点范围内，重启后仍存在。 |

## 最终定向门禁

命令运行时仅在当前进程将 `C:/Users/lyh/.cargo/bin` 放到 PATH 首位；运行前 Get-Command cargo 确认为原生 Cargo。未修改全局配置或锁文件。

| 命令 | 实际结果 | 日志 |
| --- | --- | --- |
| `cargo test --locked --all-features --lib parsers::antigravity -- --test-threads=1` | 21 passed，0 failed，903 filtered；测试 5.46s；exit 0 | `antigravity-lib.log` / `.exit` |
| `cargo test --locked --all-features --test sync antigravity -- --test-threads=1` | 22 passed，0 failed，122 filtered；测试 14.73s；exit 0 | `antigravity-sync.log` / `.exit` |
| `cargo test --locked --all-features --test store -- --test-threads=1` | 2 passed，0 failed；测试 0.38s；exit 0 | `store.log` / `.exit` |
| `cargo test --locked --all-features --test remote -- --test-threads=1` | 8 passed，0 failed；测试 2.87s；exit 0 | `remote.log` / `.exit` |
| `cargo fmt --all --check` | exit 0 | `fmt.log` / `.exit` |
| `cargo clippy --locked --all-features --all-targets -- -D warnings` | exit 0 | `clippy.log` / `.exit` |
| scoped `git diff --check` | exit 0；仅 Git CRLF 提示，无空白错误 | 当前调用输出 |

测试构建输出包含 MSVC 创建 import library 的 linker_messages 提示；不影响退出码。clippy 无警告失败。

## 后续验证边界

- 主会话拥有 README/docs/spec 与 Trellis 状态，负责独立 check 和本任务之后的完整 just ci；本实现者没有宣称本任务完整 CI 已运行。Cargo slot 已释放。
- 主会话已交给 `diagnostics_check_resume` 独立检查。检查者正在核查一个时序候选：共享 discovery 的早期 Path::exists 若因访问失败返回 false，而后续 root metadata 已恢复成功，空 listing 可能未保留早期失败。实现者收到交接后停止产品修改与 Cargo；该候选的回归和必要 discovery 修复由检查者负责，本报告不将该候选标为已解决。
- 权限用例采用确定性注入，未修改 Windows ACL。跨平台文件系统现场行为未分别实测。
- 本次证明全阻断的 usage decoder 调用为 0，未对真实 500 个 IDE DB 执行同步，也没有声称物理读取为 0 或提供生产耗时收益百分比。Discovery、metadata、DB/WAL fingerprint 的 I/O 仍存在。
- 文件缺失的外部原因及旧 JSON 是否有无损 native 覆盖仍未确定。此实现不自动修复这两类输入事实。
