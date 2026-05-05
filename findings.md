# 发现与决策：`src/` 架构、质量、逻辑、性能与注释审计

## 需求
- 对 `src/` 中代码做深入分析，覆盖：架构、代码质量、逻辑正确性、性能风险、注释是否完善。
- 交付一个后续可执行的优化 plan。
- 本轮范围起始于只读分析 + 创建/更新 `task_plan.md`、`findings.md`、`progress.md`，随后进入阶段化实施。

## 方法与验证
- 读取范围：`src/main.rs`、`src/lib.rs`、`src/commands/*`、`src/store/mod.rs`、`src/query/mod.rs`、`src/parsers/*`、`src/integrations/*`、`src/web/*`、`src/tui/mod.rs`、`tests/*`。
- `omx explore` 尝试失败：当前 Windows outside-tmux surface 返回 built-in explore harness not ready；后续改用 PowerShell/rg/Python 只读检查。
- 验证命令：
  - `cargo fmt --check`：通过。
  - `cargo clippy --all-targets --all-features -- -D warnings`：通过。
  - `cargo test -- --test-threads=1`：通过；共 10 个测试通过（4 个 web 单测、1 个 local_flow 集成测试、5 个 sync_regression 集成测试）。

## 阶段 1 实施发现（2026-05-05）
- `src/commands/mod.rs` 增加 `run_tracked` 后，`sync` / `hook-run` / `export html` 可以在不复制样板代码的情况下统一写入 `run_log.success` 或 `run_log.failed`。
- `src/commands/hook_run.rs` 需要把 `mark_trigger_worker_finished` 放在 `run_once` 结果解包之前，否则失败时会留下 `last_worker_finished_at = NULL` 的半闭合状态。
- `src/store/mod.rs::RunRecord::counts_as_failure` 让 `doctor` 与 `query::load_health` 共用“非 success 且非 running 即失败”的定义，消除了 `aborted` 仅在 health 可见、doctor 不可见的分叉。
- 新增失败注入 seam：
  - OpenCode schema 损坏：只创建一个无 `message` 表的 `opencode.db`，可稳定触发 `sync` / `hook-run` 失败。
  - export 失败：把 `--out` 指向已存在普通文件，可稳定触发 `create_dir_all` 失败。
  - recovered aborted：直接 `record_run_start("sync")` 后调用 `recover_running_runs`，可稳定断言 doctor/health 对齐。
- 额外发现：`doctor --json` 在默认 `info` 日志级别下会把 tracing 日志写到 stdout，导致机器读取 JSON 时需要抑制日志；当前只在测试侧用 `RUST_LOG=off` 规避，产品级修复应单独评估是否改为 stderr 或 JSON 模式静音。

## 阶段 2 实施发现（2026-05-05）
- 单独复用 `metadata_inode` 仍不足以稳妥识别 Windows 上的 DB 替换：其 fallback 只有 `len ^ modified_secs`，在同秒重建且大小接近时碰撞概率过高。
- 用 `inode + len + modified_ns + head signature` 组合出的 `file_identity` 可以在不改 schema 的前提下复用现有 `OpencodeCursor.inode` 字段，兼容成本低。
- 对旧 cursor（`inode == 0`）保持“只补录新 identity、不主动 reset”更稳妥：这样不会让已有用户在升级后立即重放全部 OpenCode 历史。
- 新增回归测试表明：当第二个 DB 的 `time_created` 早于旧 cursor 时，只要 identity 检测命中，就能正确重置高水位并导入新记录；此前逻辑会直接跳过该记录。

## 阶段 3 实施发现（2026-05-05）
- 对这个仓库来说，最低风险的 store 拆分方式不是先移动类型，而是保留 `FileCursor` / `RunRecord` / `Store` / `SyncRunWriter` 等核心类型在 `src/store/mod.rs`，只把 `impl` 和 helper 拆到子模块。这样外部路径保持 `crate::store::*` 不变。
- `trigger_state` 与 `source_sync_status` 虽未出现在最初建议模块名里，但单独拆成 `trigger.rs` 与 `sync_status.rs` 能避免把运行控制状态和 integration 状态硬塞进同一文件，边界更清晰。
- `SyncRunWriter` 的 helper（bucket rollup、project flush、bucket flush）与 writer 方法应一起移动到 `sync_writer.rs`；如果只移动 public methods，剩下的私有 helper 仍会把 `mod.rs` 留成半个实现文件。
- 新发现：`file_identity` 作为 `u64` 哈希直接写入 SQLite `INTEGER` 时，若命中符号位，读取逻辑中的 `max(0)` 会把它变成 0。将 identity 限制到 `i64::MAX` 范围即可避免这类往返损失。

## 阶段 4 实施发现（2026-05-05）
- `build_dashboard_snapshot` 的重复开连接问题不仅来自 query wrappers，本地 health 载荷也会额外打开 `integration_install` 和 `run_log` 连接；因此需要把 `load_integration_states` / `recent_runs` 下沉出 crate-private `*_with_conn` helper，才能真正把 snapshot 收敛到单连接。
- 对外 public query API 保持 wrapper 形态最稳妥：live API 和其它调用点继续走 `load_*(&Store)`，snapshot 则通过 `QueryContext` 复用 SQL，避免出现“为了性能把所有调用点都改成传连接”的侵入式重构。
- test-only 连接计数器比脆弱的时间断言更适合本阶段目标：它能稳定证明“snapshot 构建只开 1 次连接”，同时不引入 flaky benchmark 阈值。
- 本轮 180 行 fixture 足以证明“共享连接 + 输出不变”；由于未观察到真实 schema 热点，先不新增 `usage_event(source, path_hash)` 或更细粒度 bucket 索引，避免在没有数据证据时扩展迁移面。

## 阶段 5 实施发现（2026-05-05）
- Claude settings 的真正风险点不是 JSON 解析失败，而是“能解析成 `Value` 但结构不符合预期”的情况；把 root/hooks/event 三层都改成显式形状校验后，才能把 panic 降为可诊断错误。
- 对 `install_all` 而言，最小可回归的改动是“局部失败转成 `IntegrationAction { status: error }` 并落库”，而不是让 `init` 整体失败；这样可以同时满足“不中断其他安装”和“让 doctor/diagnostics 看到失败状态”。
- Windows 字符串命令的关键不是简单给路径加一层双引号，而是 `cmd /c ""C:\\path with spaces\\hook.cmd" --args"` 这种双层 quoting；否则 `cmd` 会把整串命令误判成可执行路径。
- 本轮没有改动 Codex argv-list 路径，因为它原本就是结构化 `notify` 数组；真正需要修的是 Claude/OpenCode 这类必须落字符串命令的集成面。

## 阶段 6 实施发现（2026-05-05）
- 这个仓库最有价值的 rustdoc 不是为每个 helper 函数补一行“做了什么”，而是给公共类型和字段补“单位/语义/隐私边界/持久化角色”说明；这能直接降低 query/export/diagnostics 后续维护成本。
- `QueryContext` 已把查询逻辑收拢到一个中心点，因此对外仅需给 public wrappers 补用途说明，就能保持文档层面的低噪音；无需把私有内部 SQL helper 也全部注释化。
- `Store` 类型层的文档要强调“cursor / run_log / integration_install / source_sync_status”的持久化角色，而不是复制各子模块的过程性注释；这更符合阶段 6 的“契约化”目标。

## 阶段 7 实施发现（2026-05-05）
- 对本地 Web UI 来说，最稳妥的错误改造不是让前端猜测纯文本 500，而是让后端统一返回稳定 JSON error shape；这样 `fetch.js` 和 `renderError` 都能围绕一个结构演进。
- `renderError` 的关键点不是简单调用 `escapeHtml`，而是彻底切换到 DOM API + `textContent`；这样错误详情即使包含 HTML/stack 片段，也不会重新变成模板字符串注入点。
- 结构化 API 错误测试放在 `src/web/mod.rs` 足够轻量：一方面验证响应 shape，另一方面通过 asset 字符串测试锁定前端安全边界，无需额外 JS 测试框架。

## 阶段 8 实施发现（2026-05-05）
- 对这个仓库来说，阶段 8 的最低风险切入点不是一上来做跨线程 channel 或复杂流式事务，而是先把“全量 source 结果缓存”改成“单 writer 常驻 + source/shard/page 级即时写入”。这样能显著压低峰值内存，同时不改 `SyncRunWriter` 的事务语义。
- `Codex` / `Claude` 的自然边界是 shard：每个 `spawn_blocking` 任务本来就产出独立 `shard.events`，因此最有效的优化是任务返回后立即 reset/write/drop，而不是再 `extend` 进整个 source 的大向量。
- `OpenCode` 的自然边界是 SQLite page：当前 `OPENCODE_PAGE_SIZE=1000` 已经提供了可用批次，只要把 page 级 `events` 立即写入，就能避免把整个 DB 的消息都放进一个总 `Vec`。
- 此轮没有加入额外内存测量依赖；验证方式转为结构性证明 + 全量回归：旧的跨 source 全量缓存结构 `SourceParseOutput` 已被移除，`run_once` 也不再持有三源完整事件集。

## 架构地图（证据）
| 层 | 主要文件 | 直接证据 | 评价 |
|----|----------|----------|------|
| 入口/分发 | `src/main.rs:1-4`、`src/lib.rs:1-22`、`src/commands/mod.rs:19-90` | `main` 只调用 `llmusage::run()`；`lib::run` 初始化日志、解析 clap、创建 `AppContext` 并 dispatch；CLI 子命令集中在 `Commands` enum | 入口清晰、薄，适合作为 CLI 架构骨架 |
| 应用路径 | `src/app.rs:7-20`、`src/paths.rs:7-41` | `AppContext` 聚合 `AppPaths` 与当前 exe；运行目录统一在 `~/.llmusage` | 边界清晰，但路径策略缺少 rustdoc 说明，例如 HOME/USERPROFILE 优先级 |
| 命令层 | `src/commands/*.rs` | `init/sync/status/doctor/diagnostics/serve/export/uninstall/hook-run` 分文件实现 | 文件粒度较好；命令主要编排 store/query/integration/parser |
| 解析层 | `src/parsers/{codex,claude,opencode}.rs`、`src/parsers/file_state.rs` | Codex/Claude 使用文件游标与 shard 并行；OpenCode 读取本地 SQLite 高水位 | 增量解析模型合理，但 Codex/Claude 存在重复骨架；OpenCode inode 游标未实际使用 |
| 存储层 | `src/store/mod.rs:163-303`、`src/store/mod.rs:854-1284` | schema、迁移、worker lease、run log、cursor、batch writer、bucket/project rollup 全在一个 1284 行文件内 | 功能强但职责过载，是后续维护风险最高模块 |
| 查询层 | `src/query/mod.rs:99-350` | dashboard 的 overview/trends/models/sources/projects/costs/health 分别打开连接查询 | API 简洁，但连接/扫描重复，适合抽公共 query context |
| Web/TUI | `src/web/mod.rs:25-84`、`src/web/assets/*`、`src/tui/mod.rs:19-132` | Web 只监听 127.0.0.1；静态资源通过 manifest 嵌入；TUI 复用 query | 离线/本地边界好；Web API 错误信息和前端测试覆盖偏浅 |

## 正向质量信号
- `src/lib.rs:19-22`、`src/commands/mod.rs:71-90` 保持入口与命令分发简单，CLI 控制流易追踪。
- `src/commands/sync.rs:95-149` 将解析并行化、写入串行化，能降低 SQLite 写冲突。
- `src/store/mod.rs:307-317` 对 SQLite 设置 `busy_timeout`、WAL、`synchronous=NORMAL`、foreign keys、temp memory，符合本地并发读写需求。
- `src/parsers/file_state.rs:58-97` 通过头/尾签名、offset 和 replay mode 区分 append 与 reparse，已有针对增量/重放的回归测试。
- `tests/sync_regression.rs:13-202` 覆盖热运行、重放替换、source bucket 汇总、worker lease、OpenCode 同时间高水位；`tests/local_flow.rs:12-149` 覆盖安装-同步-导出-卸载主流程。

## 主要问题清单（按优先级）

### P0：run lifecycle 失败路径不闭合，失败可能被记录成 running/aborted 而不是 failed
- **证据：** `src/commands/sync.rs:50-60` 在 `record_run_start("sync")` 后直接 `run_once(...).await?`，失败会提前返回，后续 `finish_run(..., "success")` 不执行，也没有 `failed` 分支。
- **证据：** `src/commands/hook_run.rs:31-50` 同样在 run_id 创建后对 `run_once` 使用 `?`，失败不会立即写入 failed。
- **证据：** `src/commands/export.rs:23-30` 在导出失败时也不会 finish run。
- **证据：** `src/store/mod.rs:439-474` 只能在下一次 run 中把遗留 running 标成 `aborted` / `recovered stale running record`。
- **证据：** `src/commands/doctor.rs:86-97` 只检查 `status == "failed"`，不会把 recovered `aborted` 当最近失败报警；而 `query::load_health` 在 `src/query/mod.rs:306-312` 才过滤所有非 success/running。
- **推论：** 用户执行 `doctor` 可能看不到上次失败；run log 对失败根因不够及时。
- **置信度：高。** 代码路径直接支持。

### P1：`src/store/mod.rs` 过大且职责混杂，schema/迁移/锁/run_log/cursor/writer/rollup 难独立演进
- **证据：** `src/store/mod.rs` 约 1284 行；公共结构和方法从 `FileCursor`、`OpencodeCursor`、`IntegrationState` 到 `SyncRunWriter`、schema bootstrap、worker lease、query helpers 全在同一模块。
- **证据：** `src/store/mod.rs:163-303` 嵌入 schema 与兼容性迁移；`src/store/mod.rs:321-404` 管 lease；`src/store/mod.rs:405-489` 管 run log；`src/store/mod.rs:553-653` 管 cursor；`src/store/mod.rs:854-1134` 管写入流水线；`src/store/mod.rs:1140-1284` 管 rollup/DDL helper。
- **推论：** 当前功能集中有利于快速开发，但后续修 run lifecycle、cursor、query 性能时容易产生大范围冲突。
- **置信度：高。** 文件结构和行数直接支持。

### P1：dashboard 查询重复打开连接和重复扫描，数据量上来后 Web/export 会放大 IO 成本
- **证据：** `src/query/mod.rs:99`、`:132`、`:173`、`:202`、`:234`、`:262`、`:314` 各查询函数都独立 `store.open_connection()`。
- **证据：** `src/query/mod.rs:338-350` 构建离线 snapshot 时串行调用 overview、4 个 trends、models、sources、projects、costs、health，意味着至少多次连接与多次扫描。
- **证据：** `src/web/assets/app.js:101-109` live 页面先请求 overview，再并行请求 trends/models/sources/projects/costs/health，服务端会为一次页面刷新打开多条连接。
- **推论：** 对小型本地 DB 问题不明显；当 `usage_bucket_30m`、`usage_event` 增长后，页面刷新和静态导出会受影响。
- **置信度：中高。** 连接/扫描重复为直接证据，实际影响需用大 fixture benchmark 确认。

### P1：OpenCode cursor 设计包含 inode，但同步逻辑没有检测 DB 替换/轮转
- **证据：** `src/store/mod.rs:56-62` 的 `OpencodeCursor` 包含 `inode`；`src/store/mod.rs:598-627` 能加载 inode；`src/store/mod.rs:629-651` 能保存 inode。
- **证据：** `src/util.rs:109-125` 提供 `metadata_inode`，但 `rg metadata_inode|inode` 显示 `metadata_inode` 未被解析逻辑调用。
- **证据：** `src/parsers/opencode.rs:41-47` 定位 `opencode.db` 后只加载旧 cursor；`src/parsers/opencode.rs:119-122` 只更新 `last_time_created`、`last_processed_ids`、`sqlite_status`、`updated_at`，没有读取 DB metadata 或重置 cursor。
- **推论：** 如果 OpenCode SQLite 文件被替换、压缩或重建，旧高水位可能导致新 DB 中较早时间的记录被跳过。
- **置信度：中高。** 字段未用是直接证据；DB 替换场景需新增回归测试证明。

### P1：集成安装/配置修改对畸形配置和带空格路径不够健壮
- **证据：** `src/integrations/claude.rs:164-175` 对 settings 根对象、`hooks` 对象、事件数组连续 `unwrap()`；如果 `settings.json` 顶层不是对象、`hooks` 不是对象、某事件不是数组，会 panic 而不是返回可诊断错误。
- **证据：** `src/integrations/mod.rs:137-152` 的 Windows shell command 是 `cmd /c "{path} --source ..."`，没有对 exe 路径做双层 quoting；若用户目录或路径含空格，Claude/OpenCode 字符串型命令存在解析风险。Codex 的 `platform_notify_args` 在 `src/integrations/mod.rs:155-179` 用 argv 列表更稳。
- **证据：** `src/integrations/mod.rs:40-63` 顺序 `codex::install()?`、`claude::install()?`、`opencode::install()?`，前一个失败会阻断后续集成安装。
- **推论：** 真实用户配置漂移或路径带空格时，init 体验可能从“部分成功 + 明确报告”退化为 panic/整体失败。
- **置信度：中。** 风险点直接可见，具体失败需 fixture 补测。

### P2：公共 API 全无 rustdoc，现有注释以过程性块注释为主，不能替代契约文档
- **证据：** 脚本统计 `src/**/*.rs` 中约 172 个 public item，前置 `///` rustdoc 数为 0。
- **证据：** `src/lib.rs:1-14` 公开了全部模块；`src/models.rs:31-57`、`src/parsers/mod.rs:13-53`、`src/query/mod.rs:11-97` 等公共类型没有字段语义、单位和兼容性说明。
- **证据：** Rust 非空行约 5134 行，注释行约 259 行，其中块注释风格约 225 行，主要是“步骤/目标”说明；例如 `src/commands/sync.rs:21-30`、`src/store/mod.rs:163-172`、`src/web/assets/app.js:30-38`。
- **推论：** 当前注释数量并不少，但偏向“执行步骤”，缺少“为什么这样设计 / public contract / 不变量 / 失败语义”；维护复杂解析和 DB 迁移时仍不够完善。
- **置信度：高。** 统计和文件证据直接支持。

### P2：Web/API 错误信息和前端安全边界偏弱
- **证据：** `src/web/mod.rs:111-166` 每个 API handler 都把内部错误映射为裸 `500`，没有记录具体错误或返回统一 error payload。
- **证据：** `src/web/assets/app.js:72-85` 的 `renderError` 直接把 `String(error?.stack || ...)` 插入 `innerHTML`，没有 HTML escaping。
- **推论：** 这是本地 127.0.0.1 UI，外部安全风险较低；但对调试体验和鲁棒性不利。
- **置信度：中。** 直接代码证据存在，风险等级取决于 threat model。

### P2：解析层批量收集全部事件后再写入，极大历史日志下内存峰值偏高
- **证据：** `src/commands/sync.rs:99-107` 同时并行解析 Codex/Claude/OpenCode，随后才进入 writer；`src/parsers/codex.rs:90-117` 和 `src/parsers/claude.rs:85-112` 都将 shard events/cursors 收集进 Vec。
- **证据：** `src/store/mod.rs:979-1060` writer 已支持 batch 写入，但当前数据源到 writer 之间仍是全量内存 Vec。
- **推论：** 普通本地用量规模可以接受；首次导入超大历史日志时，内存峰值和延迟会被放大。
- **置信度：中。** 架构直接支持，实际阈值需 benchmark。

## 注释完善度结论
- **结论：不完善，但不是“没有注释”。** 当前代码有大量“步骤/目标”块注释，能帮助理解局部流程；但公共类型/函数没有 rustdoc，缺少输入输出契约、时间/金额单位、幂等性、失败语义、隐私/本地边界、不变量说明。
- **最需要补文档的位置：**
  1. `src/models.rs`：`UsageTokens` 各字段是否包含缓存/推理 token、total 的计算规则。
  2. `src/parsers/mod.rs` 与 `src/parsers/file_state.rs`：增量游标、append/reparse、offset/fingerprint/tail_signature 不变量。
  3. `src/store/mod.rs` 拆分后的 schema/run_log/lock/writer 模块：事务边界、失败语义、迁移策略。
  4. `src/query/mod.rs`：各 payload 字段单位、成本估算来源/日期/限制。
  5. `src/integrations/*`：会修改哪些用户配置、备份/恢复语义、局部失败策略。

## 测试覆盖缺口
| 缺口 | 现状证据 | 建议 |
|------|----------|------|
| run failure 记录 | 当前测试名未覆盖失败 run lifecycle | 加 parser/write/export 失败 fixture，断言 run_log 立刻 `failed` 且 doctor/health 可见 |
| OpenCode DB 替换 | 只有 same timestamp high-water 测试 | 加 DB inode/metadata 改变后 cursor 重置测试 |
| 畸形配置 | local_flow 覆盖正常安装卸载 | 加 Claude hooks 非对象/非数组、Codex notify 非数组、路径含空格 fixture |
| 查询性能 | 无 benchmark/大数据测试 | 加大 fixture 或 criterion/轻量 timing smoke，比较 snapshot 构建连接次数/耗时 |
| public docs | clippy 未启用 missing_docs | 分阶段加 rustdoc；最后考虑 crate 内 `#![warn(missing_docs)]` 的窄范围启用 |

## 技术决策
| 决策 | 理由 |
|------|------|
| 优化先从 run lifecycle 入手 | 失败可见性影响 doctor/status/health，且改动可小、可回归测试锁定 |
| `store` 拆分应在行为测试之后做 | 该模块承担 DB 写入和幂等性核心，先补测试可降低重构风险 |
| 查询性能优化先做连接复用，再考虑 schema/index | 连接复用和 query context 风险较小；索引/物化需要用大 fixture 证明收益 |
| 注释优化以 public contract rustdoc 为主，减少无增量“步骤”注释 | 当前注释多但契约少，补 docs 应服务维护和测试，而非增加噪音 |

## 遇到的问题
| 问题 | 解决方案 |
|------|---------|
| `omx explore` 在 Windows outside-tmux surface 不可用 | 记录到本文件和 `progress.md`，改用 PowerShell/rg/Python 只读分析 |

## 资源
- `src/` live codebase
- `tests/local_flow.rs`
- `tests/sync_regression.rs`
- 验证命令：`cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test -- --test-threads=1`

## 视觉/浏览器发现
- 本任务未使用视觉/浏览器发现。

---
*本文件记录本轮分析证据与决策；后续实施前应重新读取。*
