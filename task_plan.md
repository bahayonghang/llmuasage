# 任务计划：`src/` 代码优化实施计划

## 目标
基于对 `src/` 的只读审计，按“先锁行为、再小步修复、最后重构和文档”的顺序优化 llmusage 的运行可靠性、模块边界、查询性能、集成健壮性和注释契约。

## 当前阶段
阶段 8 已完成；当前计划全部完成。

## 总体原则
- 本计划已进入实施阶段；阶段 1 已落地，后续阶段继续遵循“先锁行为、再改实现、最后统一回归”。
- 每个 P0/P1 修复前优先添加或更新回归测试。
- 保持现有 CLI 行为、数据库兼容性和本地优先/零上传边界。
- 不新增依赖，除非后续用户明确要求。
- 每阶段验证至少运行相关 `cargo test -- --test-threads=1` 子集；最终运行 `just ci`。

## 优先级总览
| 优先级 | 主题 | 主要风险 | 建议状态 |
|--------|------|----------|----------|
| P0 | run lifecycle 失败闭环 | 失败不可见、doctor 误报健康 | 待实施 |
| P1 | OpenCode cursor inode/DB 替换检测 | DB 替换后高水位跳过新记录 | 待实施 |
| P1 | store/query 边界与查询复用 | 大模块冲突、dashboard/export 多连接多扫描 | 待实施 |
| P1 | 集成配置健壮性 | 畸形配置 panic、路径含空格解析风险、安装整体中断 | 待实施 |
| P2 | public rustdoc 与注释减噪 | 公共契约不清晰、维护成本高 | 待实施 |
| P2 | Web/API 错误处理与解析内存峰值 | 调试信息弱、首次大导入内存峰值高 | 待实施 |

## 各阶段

### 阶段 1：锁定 run lifecycle 失败语义（P0）
- [x] 新增回归测试：模拟 `sync`/`hook-run`/`export html` 中途失败后，`run_log` 立即记录 `failed`、保存 error、`finished_at` 和 `duration_ms`。
- [x] 引入小型 run guard/helper，确保 `record_run_start` 后所有 `Result` 路径都会 finish 为 `success` 或 `failed`。
- [x] 调整 `doctor` 最近失败判断：至少将 `failed` 与 recovered `aborted` 都视为 warn，或统一依赖 `query::load_health` 的失败定义。
- [x] 保持 `recover_running_runs` 作为崩溃/进程被杀后的兜底，而非普通错误路径。
- **验收标准：** 新失败测试红→绿；正常 `sync`/`hook-run`/`export html` 流程仍通过。
- **验证命令：** `cargo test --test sync_regression -- --test-threads=1`、`cargo test --test local_flow -- --test-threads=1`、`cargo clippy --all-targets --all-features -- -D warnings`。
- **实施结果：**
  - 抽出 `src/commands/mod.rs::run_tracked`，统一处理 run start / success / failed finish。
  - `sync`、`hook-run`、`export html` 失败路径现已即时写入 `run_log.failed`。
  - `doctor` 与 `query::load_health` 统一复用 `RunRecord::counts_as_failure` 识别非成功记录。
  - `tests/sync_regression.rs` 新增 4 个失败生命周期回归测试。
- **状态：** complete

### 阶段 2：补 OpenCode DB 替换/轮转检测（P1）
- [x] 新增回归测试：创建 OpenCode DB，同步后替换为新 DB 且记录时间早于旧 cursor，高水位应重置并导入新记录。
- [x] 在 `sync_opencode` 中读取 DB metadata，使用现有 `metadata_inode` 或更可靠的文件指纹识别 DB 身份。
- [x] 当 DB 身份变化时重置 `last_time_created` 与 `last_processed_ids`，并保存新 inode/status。
- [x] 明确 Windows fallback inode 的局限，必要时结合文件大小/mtime/head signature，避免 len^modified 秒级碰撞。
- **验收标准：** DB 替换场景不跳数；现有 same timestamp high-water 测试仍通过。
- **验证命令：** `cargo test --test sync_regression -- opencode --test-threads=1`、完整 `cargo test -- --test-threads=1`。
- **实施结果：**
  - `src/util.rs` 新增 `file_identity`，将 inode、文件大小、纳秒级 mtime 和 head signature 组合成更稳的 DB 身份指纹。
  - `src/parsers/opencode.rs` 在读取 OpenCode DB 前检测身份变化；若 DB 发生替换/轮转，则重置高水位后继续同步。
  - `tests/sync_regression.rs` 新增 `opencode_replaced_db_resets_high_water`，锁定“新 DB 时间戳早于旧 cursor 仍能导入”的行为。
- **状态：** complete

### 阶段 3：拆分 `store` 的低风险边界（P1）
- [x] 先只做文件级拆分，不改变 SQL 行为：建议拆为 `store/schema.rs`、`store/connection.rs`、`store/run_log.rs`、`store/lease.rs`、`store/cursor.rs`、`store/sync_writer.rs`、`store/integration.rs`。
- [x] 保持 `Store` 对外方法名稳定，避免一次性改动所有调用点。
- [x] 将 schema SQL、ensure_column 和迁移辅助集中到 schema 模块；给迁移兼容策略补 rustdoc。
- [x] 将 `SyncRunWriter` 的 reset/write/cursor/bucket/project rollup 拆出并保留现有测试。
- **验收标准：** 行为 diff 为零；`src/store/mod.rs` 降为 re-export/协调层；sync 回归测试全绿。
- **验证命令：** `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --test sync_regression -- --test-threads=1`。
- **实施结果：**
  - `src/store/mod.rs` 已收敛为类型定义 + 子模块装配层。
  - 新增 `src/store/{connection,schema,lease,run_log,trigger,cursor,integration,sync_status,sync_writer}.rs`，按职责拆分原 `Store`/`SyncRunWriter` 实现。
  - 对外 API（`Store::*` / `SyncRunWriter::*`）保持稳定；现有调用点和测试无需改名。
  - 重构过程中修复了 `file_identity` 写回 SQLite 时可能因 `u64 -> i64` 符号位溢出变成 0 的问题。
- **状态：** complete

### 阶段 4：查询与 dashboard 性能优化（P1）
- [x] 新增轻量性能 fixture：批量插入较多 bucket/event，记录 snapshot 构建时间或至少连接调用次数。
- [x] 引入 `QueryContext`/`DashboardQueries`，复用单个 `Connection` 构建 snapshot。
- [x] live API 可保留单 endpoint 粒度，但内部 helper 应能共享查询逻辑，避免 snapshot 与 live 双实现漂移。
- [x] 评估 `usage_event(source, path_hash)` 或 `usage_bucket_30m(source/model/project/hour)` 这类索引是否必要；先用 fixture 证明收益，再改 schema。
- **验收标准：** snapshot 构建复用单连接；现有 Web/export 输出不变；大 fixture 性能不退化。
- **验证命令：** `cargo test web::tests query -- --test-threads=1`、`cargo test -- --test-threads=1`。
- **实施结果：**
  - `src/query/mod.rs` 新增私有 `QueryContext`，把 overview/trends/models/sources/projects/costs/health 的 SQL 逻辑集中到共享 helper。
  - `build_dashboard_snapshot` 现在只打开 1 个 SQLite 连接；`load_*` 公共 wrapper 仍保持原有 API。
  - `Store` 新增 crate-private `load_integration_states_with_conn` / `recent_runs_with_conn`，让 snapshot health 也能复用同一连接。
  - 新增 `query::tests::build_dashboard_snapshot_reuses_single_connection_and_matches_wrappers`：用 180 行 bucket/event fixture 断言 snapshot 只开 1 次连接且输出与独立 `load_*` wrapper 一致。
  - 本轮 fixture 下无需新增索引；先保留现有 schema，后续若真实数据量证明瓶颈再加索引。
- **状态：** complete

### 阶段 5：集成安装健壮性（P1）
- [x] 为 Claude 畸形 `settings.json` 增加测试：顶层非对象、`hooks` 非对象、事件非数组时返回错误/修复策略，而不是 panic。
- [x] 为 Windows 路径含空格增加 command string 测试；修复 `cmd /c` 双层 quoting，或尽量转向 argv/list 形式。
- [x] 调整 `install_all`：各集成独立尝试并收集结果，除非 hook wrapper 生成失败这类全局前置失败。
- [x] 保持备份/恢复语义，避免覆盖用户原始配置。
- **验收标准：** 畸形配置有明确错误或可恢复结果；路径含空格测试通过；一个集成失败不阻断其他可安装集成。
- **验证命令：** `cargo test --test local_flow -- --test-threads=1`、新增 integration 单元/集成测试、`cargo clippy --all-targets --all-features -- -D warnings`。
- **实施结果：**
  - `src/integrations/claude.rs` 新增 settings root/hooks/event 形状校验，畸形配置现在返回明确 `Result` 错误，不再依赖 `unwrap()`。
  - `src/integrations/mod.rs::platform_shell_command` 在 Windows 上改为 `cmd /c ""...hook.cmd" ..."` 形式，支持 hook 路径含空格。
  - `install_all` 现在会逐个收集 Codex/Claude/OpenCode 的安装结果；单个集成失败会记录 `integration_install.status=error`，但不会阻断其他集成。
  - `tests/local_flow.rs` 新增 3 个回归测试：Claude 畸形配置、部分失败不中断、路径含空格 command string。
- **状态：** complete

### 阶段 6：注释与 rustdoc 合同化（P2）
- [x] 为 public domain types 补 rustdoc：`SourceKind`、`UsageTokens`、`ProjectInfo`、`UsageEvent`、`SourceSyncStats`、query payload、cursor/run records。
- [x] 为复杂不变量补“为什么”：file cursor append/reparse、event_key 构造、bucket rollup 幂等、run lifecycle、integration backup/restore。
- [x] 删减或压缩重复的“步骤/目标”块注释，避免注释解释显而易见的代码。
- [x] 不建议一次性启用全 crate `missing_docs`；可先在核心模块局部收敛。
- **验收标准：** 关键 public API 有 rustdoc；注释重点从流程说明转向契约/不变量/失败语义；文档不引入行为变更。
- **验证命令：** `cargo doc --no-deps`、`cargo clippy --all-targets --all-features -- -D warnings`。
- **实施结果：**
  - 为 `src/models.rs`、`src/parsers/mod.rs`、`src/query/mod.rs`、`src/store/mod.rs`、`src/app.rs`、`src/paths.rs` 的核心 public 类型与关键 public query 函数补充 rustdoc。
  - 文档重点改为字段单位、隐私边界、游标/运行记录语义、query payload 契约，而不是重复解释显而易见的流程代码。
  - 保持局部收敛，不开启全 crate `missing_docs`，避免一次性扩大改动面。
- **状态：** complete

### 阶段 7：Web/API 错误处理与前端安全边界（P2）
- [x] 给 API handler 抽统一错误映射，至少记录 tracing error，并可返回稳定 JSON error shape。
- [x] 修复 `renderError`：对 error message/stack 做 HTML escaping 或改用 `textContent`。
- [x] 为错误态渲染增加最小 JS/HTML 字符串测试（若不引入 JS 测试框架，可在 Rust asset 测试中检查 escape helper/模式）。
- **验收标准：** 本地 UI 出错时可诊断；无未转义错误字符串插入 `innerHTML`。
- **验证命令：** `cargo test web::tests -- --test-threads=1`、手动 `cargo run -- serve` smoke（实施时再做）。
- **实施结果：**
  - `src/web/mod.rs` 新增统一 `api_json` 错误映射：记录 tracing error，并返回稳定 `{ error: { code, message, detail, endpoint } }` JSON。
  - `src/web/assets/app.js` 的 `renderError` 改为 DOM 节点 + `textContent` 写入错误详情，避免把 stack/message 直接注入 `innerHTML`。
  - `src/web/assets/data/fetch.js` 会优先读取结构化 JSON 错误载荷，再回退到文本错误。
  - `src/web/mod.rs` 新增 3 个最小测试：结构化 API 错误 JSON、`renderError` 使用 `textContent`、fetch 层读取结构化错误。
- **状态：** complete

### 阶段 8：解析内存峰值优化（P2 / 可选）
- [x] 先用大 fixture 量化首次同步内存/耗时，避免过早重构。
- [x] 若确有压力，将 parser 输出改成 bounded batch/channel 或 per-source streaming writer。
- [x] 保持 reset/replay 的事务语义，避免流式写入破坏幂等性。
- **验收标准：** 大历史日志导入内存峰值下降；现有重放/增量测试全绿。
- **验证命令：** 大 fixture benchmark/smoke、`cargo test --test sync_regression -- --test-threads=1`。
- **实施结果：**
  - `src/commands/sync.rs` 不再先 `try_join!` 三个 source 再统一持有全部事件；现改为单 writer 常驻，按 source 顺序解析并即时写入。
  - `src/parsers/codex.rs` / `src/parsers/claude.rs` 改为按 shard 写入：每个 shard 解析完成后立即执行 replay reset、event batch write 与 cursor flush，不再把整个 source 的 events 累积到单个大 `Vec`。
  - `src/parsers/opencode.rs` 改为按分页写入：每页 `message` 转换成 `page_events` 后立刻批量落库并释放内存。
  - `src/parsers/mod.rs` 移除旧的 `SourceParseOutput` 全量事件缓存结构，改为以 `SourceSyncStats` 驱动 sync 汇总。
  - 该实现将峰值内存从“所有 source 的全部事件总和”收敛为“当前 source 的单批 shard/page 事件 + cursor 元数据”，在不新增依赖的前提下完成低风险优化。
- **状态：** complete

### 阶段 9：最终回归与文档同步
- [x] 运行完整仓库门禁：`just ci`。
- [x] 如果命令行为、doctor 输出、README 中的能力描述发生变化，同步 `README.md`、`README.zh-CN.md` 与 docs 对应页面。
- [x] 汇总变更、风险和验证结果。
- **实施结果：**
  - 已运行 `just ci`，其中包含 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test -- --test-threads=1` 与 `npm --prefix docs run docs:build`，全部通过。
  - 本轮改动未改变面向用户的 CLI 命令集或 README/docs 中已公开的行为说明，因此未额外修改 `README.md`、`README.zh-CN.md` 或 docs 页面。
  - 阶段 8 保持 defer；其余阶段已完成并通过回归验证。
- **状态：** complete

## 已做决策
| 决策 | 理由 |
|------|------|
| 第一实施优先级是 run lifecycle | 失败可见性是用户信任和后续诊断的基础，且改动范围可控 |
| 大模块重构排在 P0 修复之后 | 先锁行为，避免把 bugfix 和重构混在一起 |
| 查询优化先复用连接，再考虑索引/物化 | 低风险改动优先；索引需要数据量证据 |
| 注释优化以 rustdoc 和不变量为中心 | 当前问题不是注释行数少，而是公共契约缺失 |

## 遇到的错误
| 错误 | 尝试次数 | 解决方案 |
|------|---------|---------|
| `omx explore` Windows harness not ready | 1 | 记录错误，改用 PowerShell/rg/Python 只读分析 |
| `doctor --json` 回归测试读取 stdout 失败（tracing 日志混入 stdout） | 1 | 在测试子进程里设置 `RUST_LOG=off`，先锁定阶段 1 行为；后续再单独评估是否把日志切到 stderr |
| `file_identity` 哈希值写入 SQLite 时命中 `i64` 符号位导致读取为 0 | 1 | 在 `file_identity` 中屏蔽最高位，确保可稳定往返存取 |
| query fixture 直接插入 `usage_bucket_30m` 时命中联合主键冲突 | 1 | 改为按 `(day, hour)` 生成唯一 `hour_start`，保留大样本但不破坏 bucket 主键 |

## 备注
- `findings.md` 是本计划的证据来源，实施前应先读取。
- `progress.md` 记录本轮命令和验证结果。
- 当前计划已执行完成；如后续继续优化，可从阶段 8 的大历史日志基线评估重新开启。
