# 重构 tests 目录并补齐缺失测试

## Goal

将当前堆积在 `tests/` 根目录的集成测试重组为按稳定业务边界分层的测试套件，
同时根据项目已记录的合同和高风险路径补齐缺失的回归测试。重组后应当更容易
定位用例、复用夹具、按领域运行测试；除已授权的敏感 `safe_preview` 最小收敛外，
不改变其他产品行为。

## Background and Confirmed Facts

- `tests/` 根目录当前有 14 个 Rust 集成测试入口，只有
  `tests/fixtures/architecture/` 使用了领域子目录。
- 迁移前的主要入口混合多个职责：基线 `sync_regression` target 同时包含
  通用 sync、锁/迁移、OpenCode、Kimi、Pi/Oh My Pi、Grok、Antigravity、ZCode
  和 DeepSeek Harness 场景；基线 `report_commands` target 混合报表、日志、
  diagnostics、doctor、catalog、source-status 和 statusline；
  基线 `m2_raw_archive_logs` target 混合 raw archive、近期窗口、reset、job 生命周期、
  取消和子进程输出。
- `cargo test --locked --all-features -- --list` 在当前工作树列出 797 个
  library unit tests 和 202 个 `tests/` integration tests，共 999 个用例。
  该命令只证明用例可发现，不证明缺口已覆盖。
- `Cargo.toml` 没有显式 `[[test]]` 目标；当前依赖 Cargo 对 `tests/*.rs`
  的自动发现。子目录重组必须保留少量根 harness，或显式声明 test target，
  不能仅把 `.rs` 文件移入子目录。
- CI 通过 `scripts/ci-rust.py` 运行 locked/all-features/单线程 Rust 测试；
  `.github/workflows/ci.yml:148` 还单独指定 `architecture_dependencies` test target，
  该入口名必须保持或同步修改 CI 契约。
- 现有 code-spec 已列出可用于补缺的场景矩阵：来源第二次同步、append、
  rewrite/truncate、删除历史/重建保护、missing root、home override、recent-window、
  取消/失败不推进 cursor，以及 CLI/Web/JobRegistry 共享校验错误码
  （`.trellis/spec/llmusage/backend/source-sync-contracts.md:267-309`）；完整重建还必须
  证明 parserless event/bucket/behavior/cursor/source-file 全部存活
  （`.trellis/spec/llmusage/backend/token-accounting-contracts.md:195-223`）。
- 现有 integration 场景在不同来源之间不对称；例如 Kimi/ZCode/DSH 已有多个
  生命周期用例，而 Codex/Claude/OpenCode 的高层集成用例更集中于少数性能或
  增量路径。需要先建立“合同场景 × 来源/消费者”矩阵，区分已有 unit
  或 integration 证据与真正缺口，不按文件行数盲目增测。
- 用户已确认以现有 code-spec/ADR 和高风险跨层场景矩阵为“缺失测试”的
  验收准则。本机未安装 `cargo-llvm-cov`；本任务不安装覆盖率工具、新增依赖
  或设置行/分支百分比门槛。
- 实时检查确认 `src/parsers/behavior.rs:457-486` 的 `safe_tool_preview` 会把
  `file_path`、`path` 和 shell `command` 截断后直接写入 preview。现有
  迁移前的 OMP integration 基线只断言长度上限和 tool-result secret 不泄露，
  没有断言原始路径/命令不持久化。新增该高风险合同测试将在当前代码上失败。
  用户已授权把通过此类高风险测试所直接需要的最小生产修复纳入任务；当前确认的
  修复边界仅为不再将原始路径/命令写入 `safe_preview`，不扩展 schema、API 或其他行为。

## Requirements

### R1 — 建立可追溯的覆盖基线

- 盘点 `tests/` 与 `src/**` 的现有 unit/integration 用例，建立领域、合同场景、
  已有证据、缺口和拟新增用例之间的矩阵。
- 优先覆盖会造成数据丢失/重计、隐私泄露、锁/cursor 错误推进、公共绑定面扩张、
  取消/失败被误报成功的缺口，其次是边界输入和表现投影。
- 不为达到数量而复制已有 unit test；新增 integration test 必须证明跨层行为
  或一个现有测试无法防止的回归。

### R2 — 重组为领域子目录

- 将集成测试拆分为 `tests/api/`、`tests/architecture/`、`tests/cli/`、
  `tests/query/`、`tests/remote/`、`tests/store/`、`tests/sync/`、`tests/tui/`
  和 `tests/support/`；测试根目录不再平铺业务逻辑文件。
- 每个领域使用可独立聚焦运行的 Cargo test target；测试逻辑、来源 fixture 和
  通用 helper 进入对应子目录，不使用一个新的万行级 `main.rs`/`mod.rs`
  替代旧大文件。
- 保持每个测试的独立临时 home/DB/环境变量恢复边界；共享 helper 必须属于
  `tests/support/` 且不隐式读取真实用户数据。
- 保留 `architecture_dependencies` test target 的 CI 可定址性，并同步修正所有指向旧测试
  路径的活跃 spec/ADR/开发文档。归档 Trellis 任务保持不变。

### R3 — 补齐确认的高价值缺口

- 按 R1 矩阵为每个确认缺口先添加能区分错误实现的定向用例。用户已授权
  对新增高风险合同测试暴露的现有缺陷做“使该用例通过所必需”的最小生产
  修复；任何 schema/API/同步语义或无关行为变更仍必须返回规划。
- 每个新用例要么对应已编号的 code-spec/ADR 条款，要么在矩阵中记录实际
  风险和证据；禁止无差别的 snapshot 或只断言“返回 Ok”。
- 新增覆盖至少包含正常路径、相应高风险错误/取消路径和状态不变式，不仅
  检查某个类别“曾出现”。
- 本轮已确认的必补缺口为：成功全量 rebuild 保留 parserless 的 event/bucket/
  behavior/cursor/source-file 全部行；Claude、OpenCode、Kimi 和 Grok 的真实来源
  recent-window 过滤、不推进全历史游标与后续全量恢复；Oh My Pi 现有 recent
  用例扩展到同一不变式；以及 Oh My Pi 工具参数的原始路径/命令不进入
  `safe_preview` 持久层。

### R4 — 保持行为、可定址性与门禁

- 结构重组本身不修改 CLI/API/DB/schema/序列化/用户可见行为；移动前后测试清单
  与旧用例计数必须可对账，除非记录了合并/删除理由。
- 保留按领域 test target 和按单个 test name 的聚焦运行方式；更新必要的
  `justfile`、CI 和 code-spec 命令。
- 先运行新结构的 focused suites，再运行
  `cargo test --locked --all-features -- --test-threads=1`、`python scripts/ci-rust.py`、
  `just ci` 和 `git diff --check`。

## Acceptance Criteria

- [x] AC1：产出覆盖矩阵，对每个领域/合同场景标记现有 unit、integration、
  已确认缺口、新增用例或明确延后理由；不将“无行覆盖数据”冒充为已全面覆盖。
- [x] AC2：`tests/` 形成领域子目录，Cargo 使用显式领域 test targets 发现用例；
  原有三个超大入口
  `sync_regression.rs`、`report_commands.rs`、`tui_panels_prop.rs` 的测试逻辑被拆到
  可独立理解的领域模块，且没有单一新模块重现同等的多领域堆积。
- [x] AC3：纯结构迁移检查点中，移动前的 999 个基线用例全部仍可发现；旧用例如有
  合并/删除，覆盖矩阵记录一对一理由；补充覆盖后最终数量只增不减。
- [x] AC4：所有新增测试都可追溯至 code-spec/ADR 或矩阵中的具体风险，并包含
  能区分错误实现的结果/状态断言。
- [x] AC5：成功 full rebuild 保留 parserless 来源在六类表中的全部行；五个待补的
  真实来源 recent-window 路径都证明窗口过滤、不推进全历史状态且后续全量可恢复。
- [x] AC6：Oh My Pi 工具参数继续持久不可逆的 `input_fingerprint`，但原始
  `file_path`、`path`、`cmd` 和 `command` 值不进入 `safe_preview`；对应 unit 与
  sync-to-store integration 测试先红后绿，生产修复仅限 `safe_tool_preview` 敏感字段处理。
- [x] AC7：用户 home、真实 `~/.llmusage/` 数据、活动数据库和外部网络不被测试
  读写；环境变量、子进程、端口、锁和临时文件在成功/失败路径都得到恢复。
- [x] AC8：`architecture_dependencies` 及其 fixture 门禁仍可通过旧 target 名独立运行；
  所有活跃 spec/ADR/开发命令不再引用失效的测试路径。
- [x] AC9：focused suites、locked/all-features/single-thread Cargo 测试、`python scripts/ci-rust.py`、
  `just ci` 与 `git diff --check` 全部通过；失败、跳过和未取得的证据分开报告。

## Out of Scope

- 为追求数字覆盖率而重写大量已有 unit tests，或增加与产品风险无关的同义用例。
- 修改 CLI/API/DB/schema/计价/同步语义，或超出新增高风险合同测试所必需的生产修复。
- 安装用户全局工具、引入新依赖、修改 GitHub branch protection、推送远程或创建 PR。
- 修改 `.trellis/tasks/archive/` 中的历史任务路径记录。
