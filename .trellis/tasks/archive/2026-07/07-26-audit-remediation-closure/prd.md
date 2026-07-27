# 审计整改二次闭环

## Goal

对已归档的 `07-24-audit-remediation` 做第二次证据闭环，修复复审确认仍存在的九项问题，并用可在旧实现上失败、在新实现上通过的验收证据替代“任务已归档即完成”的判断。

## Background

- 复审范围：`3def75c..HEAD`，重点核对 14 个已归档的 `07-24-*` 整改子任务。
- 当前结论为 No-Go：关键正确性、安全性、契约与验证问题仍未闭环。
- 当前工作区基线干净，但 `cargo fmt --check` 失败；Rust subprocess 测试还有一个 `os error 2` 未归因。
- 本任务只接管复审确认的问题，不重新打开已经有充分证据证明完成的旧审计项。

## Requirements

- **R1 写入互斥**：失锁 worker 必须停止，旧 generation 不得进入任何写事务，`bootstrap()` 与 mutation 必须纳入同一排他边界。
- **R2 外部配置安全**：Windows 配置覆盖不能先删除目标；业务记录失败时不得留下无法追踪的外部文件变更。
- **R3 Job 契约**：公开 `JobRegistry` 入口统一校验，未知 source 不得退化为全量；`recent_days` 必须真正限制读取工作量与语义范围。
- **R4 更新信任锚**：stable 更新必须解析并安装不可变 tag/commit，预览和确认必须展示准确目标。
- **R5 日志有界**：单个长生命周期进程内日志体积与保留量持续有界，并能观测 non-blocking writer 丢弃量。
- **R6 JSONL 健壮性**：共享 reader 必须限制单行大小、报告 malformed 完整行，并把取消信号带入 blocking 解析循环。
- **R7 验证诚实性**：声明的 MSRV 与 lockfile/CI 实际可构建版本一致；格式门和 subprocess 集测必须得到确定结论。
- **R8 分层约束**：`sync` 不得依赖 `commands`，CI 必须识别 `use`、全限定路径、别名等所有依赖形式。
- **R9 Public 读边界**：public 模式默认不得暴露原始日志、本地路径和内部 job/diagnostics 细节；loopback 行为保持兼容。
- 每个子任务必须独立实现、验证、提交和归档；父任务不承载产品代码修改，只负责范围、顺序与最终集成复审。
- 每个修复必须先有复现或回归测试；无法在旧实现稳定失败的测试必须说明替代证据。

## Key Decisions

- Windows integration 原子覆盖允许新增仅 Windows target 编译的直接 `windows-sys` 依赖，以调用 `ReplaceFileW`/相关系统 API；不采用自维护 unsafe FFI。
- `recent_days` 按原产品含义实现真实 bounded import，不通过改名或 `applied=false` 弱化契约。
- ARCH-002 只闭环已确认的反向依赖和 CI 漏检，不扩展为 God-module 全量拆分。
- 父任务只负责跨子任务验收；任何未完成 child 都阻止父任务归档。

## Child Task Map

| Requirement | Child task |
| --- | --- |
| R1 | `07-26-write-fencing-closure` |
| R2 | `07-26-integration-atomic-replace` |
| R3 | `07-26-sync-job-contract-closure` |
| R4 | `07-26-immutable-self-update` |
| R5 | `07-26-runtime-log-bounds` |
| R6 | `07-26-bounded-jsonl-reader` |
| R7 | `07-26-msrv-validation-baseline` |
| R8 | `07-26-arch-dependency-enforcement` |
| R9 | `07-26-public-read-security-boundary` |

## Acceptance Criteria

- [x] 九个子任务全部有针对原缺陷的负向/回归证据，且独立通过各自 focused gate。
- [x] `cargo fmt --check`、Clippy、Rust docs、完整 Rust tests、dashboard JS tests 与 VitePress build 全部通过。
- [x] MSRV job 使用与 `Cargo.toml` 相同的固定版本，并执行 `cargo check --locked --all-features` 成功。
- [x] 共享 DB 上两个独立 `Store`/connection 的确定性 lock-steal 测试证明旧 worker 在下一次 heartbeat/write 前停止且不能提交；该证据验证跨进程共用的 SQLite fencing 边界，但不表述为 OS subprocess 测试。
- [x] Windows failpoint 测试证明 replace/记录失败后目标文件始终是完整 old 或完整 new，且可恢复。
- [x] public security 测试通过真实 TCP socket 证明敏感 read routes 在 public 默认模式不可访问。
- [x] `json_events_subprocess_emits_ndjson_per_event` 的 `os error 2` 已修复，且该用例在独立运行和完整测试套件中稳定通过；否则继续阻止父任务归档。
- [x] 父任务最终复审逐条映射 R1-R9 到代码、测试和提交；任何未完成项都阻止父任务归档。

## Out of Scope

- 不重做已通过复审的旧整改项，也不扩展到新来源、新 UI 或无关重构。
- 不执行真实 self-update、发布、push 或远程仓库操作。
- 不借 R8 扩展为旧任务中 3-8 周的 God-module 全量拆分或整个 public API 收口。
