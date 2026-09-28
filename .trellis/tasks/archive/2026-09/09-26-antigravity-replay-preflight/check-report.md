# Antigravity 预检独立检查

日期：2026-09-28。检查者：`/root/diagnostics_check_resume`。

状态：独立检查完成，A1–A5 通过。发现阶段错误修复、定向回归与完整 `just ci` 均通过。产品编辑和 Cargo 操作均在主会话正式交接后执行。Cargo 和产品文件所有权已经释放。

## Findings (fixed)

- 文件：`src/parsers/source_files.rs:329`。
  问题：共享 discovery 的 `Path::exists` 把根访问错误折叠为空 listing。Antigravity 构造两份 listing 后才再次检查根 metadata；如果访问已经恢复，后置检查无法得知发现阶段失败。未知根中的副本或更强请求身份可能因此遗漏。
  修复：共享发现入口使用一次根 metadata 结果。只有 NotFound 返回无错误的空 listing；PermissionDenied 等访问错误保存在 `listing.errors`，非目录根也保留稳定错误。后续 metadata 成功不会清除发现时的失败。Antigravity 继续通过已有 `discovery_incomplete` 闭集 code 阻断可能受影响的选中产品。
- 私有 `list_matching_files_with_root_metadata` 同时被生产入口与确定性测试使用。未新增公开 API、配置、持久化字段或发现范围。错误文本不拼接完整 root 路径；只含稳定非目录类别或 OS error。既有 WalkDir 错误处理未重构。

### Red / Green

`review-discovery-red.log` / `.exit`：先保留原先的 metadata-error-as-empty 语义并加入测试，3 项中 1 通过、2 失败，exit 101。

- `list_matching_files_retains_root_error_after_access_recovers`：真实临时 root 和候选 DB 存在；发现阶段注入 PermissionDenied，随后真实 metadata 成功。原逻辑的 errors 长度为 0，期望 1。
- `list_matching_files_rejects_non_directory_root`：真实临时 root 是普通文件。原逻辑将 root 本身当成匹配输入，未报告根错误。
- `list_matching_files_preserves_missing_root_as_empty`：真实不存在 root 保持无错误空结果；修复前后均通过。

`review-discovery-green.log` / `.exit`：修复后运行全部 `parsers::source_files::tests` 和七项 `parsers::antigravity::tests::preflight`，17 passed、0 failed、910 filtered，exit 0，测试耗时 4.37 秒。

根权限恢复用例证明错误在真实生产使用的共享 helper 中保留；Antigravity 的 `preflight_unselected_root_failure_blocks_unknown_product_copies` 中 walk-error 分支证明：即使后续 metadata 成功，带错误的 listing 仍阻断选中产品，decoder 调用为 0，完整存储行保持不变。

### 共享 helper 影响面

已核对七处生产调用：Codex sessions、Claude projects、Kimi sessions、Pi session roots、OMP sessions、Antigravity conversations 和 DeepSeek Harness sessions。Antigravity 调用同时服务 CLI/IDE。调用方均以目录为输入；Pi 的 `PI_AGENT_DIR` 按现有 root 目录契约处理。没有新增单文件入口，也未修改内部解析单个文件的函数。

正常目录的递归遍历、过滤、排序和 canonical-path 去重保持不变。NotFound 保持空目录语义。新增行为只将访问失败与非目录输入送入已有 inventory-error 通道。全部 source_files 测试覆盖 Grok 直接 sidecar、DeepSeek 精确文件名、Pi/OMP 多 root 与路径归属。

## A1–A5 检查

| AC | 独立结论与证据 |
| --- | --- |
| A1 | 全阻断判断位于 `decode::read_file` 之前。私有 cfg(test) 计数器在真实 decoder 入口计数；CLI、IDE、both 三种选择均校验 0 次调用。工作计数保持 0，parse_ms 保留 discovery 开始后的实际经过时间。engine 单独查询已有 stored_events；没有用文件总大小虚构读取节省。 |
| A2 | 当前 metadata 将未发现的本地路径分为 missing、out-of-scope 和 unreadable；count 按规范化成员累加。发现错误含未知产品时保守阻断两组，最后仅返回 selected 来源。修复后的 discovery 保留初次根错误，不依赖后来访问仍然失败。真实旧 JSON 与改根集成用例检查重启后的 code/count/observed_at/scope。 |
| A3 | 只有所有选中产品失败时早退；partial 路径仍解码两 root。单独未选中历史成员缺失不会触发全部选中产品阻断。既有 native ownership、较强身份、去重、DB/WAL snapshot 与跨产品转移 guard 未改。remote membership 通过 local-only 查询排除，并有 metadata probe 断言。 |
| A4 | early return 不生成 shard；`commit_antigravity_snapshot` 仍为写入入口。恢复成员后完整组更新，source issue 清除。无 lossy rebuild 保留受保护数据；显式 allow-lossy 仅豁免 missing/out-of-scope，不豁免 access/discovery/fingerprint。hook-era 保留逻辑未改。 |
| A5 | 失败覆盖检查不执行成功 inventory 标记，不推进 usage cursor。单元测试比较 events、buckets、raw、turns、tools、cursors、source_file、accounting meta 的完整行；集成测试比较 events/buckets/cursors/source_file/marker 完整行。source issue 的 UTC 观察时点与旧 last_seen_at 分离。driver 保留 SourceFinished，与 SourceStarted 各一次；故障阻止 sweep、recent-completed 与 marker 认证。 |

生命周期与持久化跟踪：`src/parsers/driver.rs:173` 的失败/取消 sweep guard、`:195` 的 SourceFinished、`src/sync/engine.rs:290` 的成功清理 entry、`:427` 的 marker guard、`:453` 的附加诊断持久化、`:458` 的 recent-completed guard。

## Verification

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| 实现者 Antigravity lib 回归 | 21 passed，exit 0 | `antigravity-lib.log` |
| 实现者 Antigravity sync 集成 | 22 passed，exit 0 | `antigravity-sync.log` |
| 实现者 store / remote | 2 / 8 passed，exit 0 | `store.log` / `remote.log` |
| 独立发现阶段 red | 1 passed、2 failed，exit 101，符合预期 | `review-discovery-red.log` |
| 独立 source_files + preflight green | 17 passed，exit 0 | `review-discovery-green.log` |
| Lint：Rust fmt / Clippy `-D warnings` | PASS | `ci-preflight.log` |
| TypeCheck：Rust 编译 / 桌面 TypeScript 构建 | PASS | `ci-preflight.log` |
| 根 Rust lib | 915 passed，12 ignored，0 failed | `ci-preflight.log` |
| 八个集成测试 target | 247 passed，0 failed；含 sync 144、CLI 34、architecture 12 | `ci-preflight.log` |
| Dashboard / scripts JS | 两个 suite 分别 66、4 passed | `ci-preflight.log` |
| 桌面前端 | 65 tests passed；构建通过 | `ci-preflight.log` |
| 桌面 Rust | lib 18、AC 9、quota 6 passed | `ci-preflight.log` |
| CI contract checks / rustdoc / docs build | PASS | `ci-preflight.log` |
| 独立完整 `just ci` | exit 0，251.635 秒 | `ci-preflight.log` / `ci-preflight-result.json` |

完整门禁使用命令范围 PATH 中的原生 `C:/Users/lyh/.cargo/bin/cargo.exe`。命令环境保存至 `ci-preflight-env.json`；退出码、耗时和前后锁比较保存至 `ci-preflight-result.json`。不修改全局 Cargo 配置或门禁参数。

`Cargo.lock`、`desktop/src-tauri/Cargo.lock`、`desktop/package-lock.json`、`docs/package-lock.json` 在完整门禁前后字节一致，SHA-256 分别保存在 `ci-preflight-locks-before.json` 与 `ci-preflight-locks-after.json`。六个最终检查源码文件的 SHA-256 保存在 `reviewed-source-hashes.json`。

## Findings (not fixed)

未发现未修复的产品问题。完整 CI 通过。权限错误采用确定性注入，未更改 Windows ACL；不能据此声称各操作系统权限现场均经过实测。没有使用真实来源目录或用量数据库运行 sync/rebuild/reset，没有测量生产 500 个 DB 的耗时收益或声称零 I/O。
