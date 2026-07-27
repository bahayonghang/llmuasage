# 执行计划：Grok Build 被动源（grok）

前置：`prd.md`（需求）、`design.md`（技术设计）、`research/grok-build-evidence.md`（样本证据）。按序执行，每个检查点后运行对应验证命令。

> 2026-07-27 修订：定价步骤取消（MVP unpriced）；幂等改会话原子重放；真机验证改 `--home` 隔离；补 updates 追加与 terminal/ 遍历测试。

## Step 0 — 契约预读（开工门禁）

- [x] 读 `.trellis/spec/llmusage/backend/source-sync-contracts.md`、`token-accounting-contracts.md`、`write-fencing-contracts.md`。
- [x] 读 `src/parsers/kimi_code.rs`（reset_path_hashes 用法）与 `git show 4d6b04e --stat` 确认接入模式。
- [x] 确认 token-accounting 契约对"子通道全 0 + total 权威"的既有语义；若无先例，先补契约表述再写码。
- [x] 确认在 `dev` 分支上开工（task.json base_branch 已修正为 dev）。

## Step 1 — 类型与注册骨架

- [x] `src/domain/models.rs`：新增 `SourceKind::Grok`（id `grok`）。
- [x] `src/registry.rs`、`src/domain/source_descriptor.rs`（`quality: TotalOnly`）、`src/domain/platform_monitor.rs`：注册被动 descriptor（`GROK_HOME` → `~/.grok`）与 probe。
- [x] `src/store/schema.rs`：`SourceKind::Grok` 归入 `TOKEN_ACCOUNTING_VERSION` 分支。
- [x] 编译通过 + 枚举匹配处（sync/doctor/tui/web/query）全部补齐。
- 验证：`cargo clippy --all-targets --all-features -- -D warnings`

## Step 2 — fixture 准备（写解析器之前）

- [x] 依 research 中的真实结构手工构造脱敏 fixture：
  - updates 含 totalTokens 逐轮样本（移植 tokscale 测试样本，method 名用 `_x.ai/session/update`）；
  - 本机 0.2.112 形态：updates 无 token 字段 + signals.json（`contextTokensUsed`/`primaryModelId`）；
  - 混合形态（updates 部分计数 + signals 更大总量 → 对账差额）；
  - 空会话 / 中断会话（无 signals、无计数）；
  - 含 `terminal/` 子目录（放置嵌套哨兵文件）与 `*.lock` 文件的目录布局。
- 存放位置与现有 parser fixture 约定一致（参照 pi/kimi_code 测试的 tempfile 构造模式）。

## Step 3 — 会话枚举 + 解析器实现

- [x] `src/parsers/source_files.rs`：grok 专用**固定两级枚举**函数（`sessions/*/*/` + 白名单文件名直接 join；不走 WalkDir 递归），输出会话目录及当前白名单 sidecar 集，供会话级变化/缺失判定。
- [x] `src/parsers/grok.rs`：按 design.md 实现：
  - 会话原子重放：sidecar `FileCursor` 仅变化检测；任一变化 → 全量重parse → `reset_path_hashes=[session_hash]` + 全量事件；
  - 路径 A 逐轮增量（单调化、切轮、fallback 聚合）+ 路径 B signals 差额（时间戳锚定最后活动）；
  - 已追踪 sidecar 缺失时保留旧事件并记录 missing，不提交破坏性 reset；恢复后完整重放；
  - 元数据回退链、秒/毫秒/RFC3339 时间戳规则、percent-decode、`provider_label` 空串、子通道全 0。
- [x] `src/parsers/mod.rs`：注册 `bounded_contract_parse`。
- [x] 单元测试（模块内，对齐 tokscale 用例矩阵 + 审核新增项）：逐轮切分、单调化、无模型回退、signals 对账、对账时间戳锚定、秒/毫秒解析、updates 已覆盖 signals 时跳过、固定两级枚举候选集与嵌套哨兵隔离。
- 验证：`cargo test grok -- --test-threads=1`

## Step 4 — sync/呈现接线

- [x] `src/commands/{sync,sync_progress,sync_summary,doctor,diagnostics,help}.rs`、`src/tui/report_table.rs`（来源色）、`src/query/mod.rs`、`src/web/mod.rs`、`src/api/error.rs`。
- [x] **不加 grok 定价行**；确认 grok 事件 `pricing_status=unpriced` 在成本视图中的显示与其他 unpriced 模型一致。
- 验证：`cargo test -- --test-threads=1`

## Step 5 — 集成测试（onboarding 门禁必列项 + 审核补强）

- [x] fixture → `UsageEvent` 端到端解析测试（total 正确、子通道 0、unpriced）。
- [x] sync-twice：sidecar 无变化时第二次 sync 零删除零新增。
- [x] **updates 追加回归**：追加新轮后重扫，总量正确、事件键无碰撞、旧事件被重放替换。
- [x] **signals 增长回归**：调大 `contextTokensUsed` 后重扫，收敛到新总量、恰一条对账事件。
- [x] **对账回收回归**：先只有 signals，再补 updates 逐轮数据，重扫后不双算。
- [x] cursor 回归：updates.jsonl 截断/重建；已追踪 sidecar 删除时常规 sync 保留事件并标 missing、无损 rebuild 被 guard 拒绝、恢复后重放收敛。
- [x] probe/status：无 `~/.grok` → `passive_no_data`；有样本 → `passive_ready`。
- 位置：`tests/grok_flow.rs` 或并入 `tests/sync_regression.rs`（与现有组织方式一致者优先）。
- 验证：`cargo test --all-features -- --test-threads=1`

## Step 6 — 真机验证（review gate，隔离运行）

- [x] 保留真实 `GROK_HOME`（默认 `~/.grok`），用临时目录隔离 llmusage 数据库：
  `cargo run -- --home <tmpdir> sync`，随后 `cargo run -- --home <tmpdir> source-status`。
- [x] 确认：当前真实样本写入 2 条 grok-4.5 事件、155,329 token，`pricing_status=unpriced`，无解析错误；sync 全程无卡顿（间接验证未触达 `terminal/`）。
- [x] `cargo run -- --home <tmpdir> serve` 看板来源 API 返回 grok，页面来源清单包含 grok。
- [x] 验证完毕删除临时目录。**默认 `~/.llmusage` 用户库全程不被触碰。**
- ⚠️ 已知风险：路径 A（updates 逐轮）无真机数据可验，仅有 fixture 覆盖——在 PR 描述中如实说明。

## Step 7 — 文档与候选表

- [x] `docs/agents/passive-source-candidates.md`：Grok 行更新为 parser-backed 决定，写明证据、total_only/unpriced 限制、0.2.112 覆盖缺口。
- [x] `README.md`、`README.zh-CN.md`、`docs/{architecture/index,guide/first-sync,reference/cli,dashboard/index,index}.md` 及 `docs/zh/` 对应页。
- 验证：`npm --prefix docs run docs:build`

## Step 8 — 收尾

- [x] `just ci` 全绿（含 `cargo fmt`；注意全局 formatter hook 对 .rs import 排序的干扰，提交前统一 `cargo fmt`）。
- [x] spec 更新：`source-sync-contracts.md` / `token-accounting-contracts.md` 增补 grok 契约（对齐 4d6b04e 先例）。
- [x] Conventional Commit（中文 scope，如 `feat(同步): [AI] ✨ 新增 Grok Build 来源`）。

## 回滚点

- Step 1-5 期间只恢复本任务明确触及的文件，或对已形成的任务提交执行精确 `git revert`；不得使用会覆盖无关工作树改动的仓库级 checkout/reset。
- Step 6 使用 `--home` 临时目录，验证残留随目录删除；无需触碰任何用户数据。

## 与 remove-hook-realtime-sync 任务的顺序约束

`07-27-remove-hook-realtime-sync` 同样改动 `source_descriptor.rs`/`registry.rs`/`source-status`/`platform_monitor.rs`。**先完成 remove-hook 任务，再启动本任务**；Grok 直接基于 passive-only 终态接入，不并行、不为已删除的 activation/integration 字段写兼容层。
