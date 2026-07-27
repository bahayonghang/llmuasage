# 执行计划：移除 hooks 实时同步，收敛为被动 only

前置：`prd.md`（需求与已确认的 D1/D2）、`design.md`（终态与拆除顺序）、`research/`（代码面全图 + 本机实证）。步骤是执行顺序，不是强制 commit 边界；只在相关测试已绿的完整切片上提交。

## Step 0 — 契约预读与决策确认（开工门禁）

- [x] 用户已于 2026-07-27 认可 D1（Antigravity 数据停摆、历史保留）与 D2（uninstall 转型遗留清理）。
- [x] 读 `.trellis/spec/llmusage/backend/{integration-file-contracts,write-fencing-contracts,source-sync-contracts}.md` 与 ADR-0001/0008/0009。
- [x] `dev` 分支开工。

## Step 1 — 呈现层拆除

- [x] `query/mod.rs:517-529`：`HealthPayload`/`HealthSummaryPayload` 删 `integrations` 字段；随编译错误清 doctor（`:70-84, :153-169` 的 hook 检查项，被动 probe 检查保留）、`status.rs:57`、`diagnostics.rs:52-58,71`、`source_status.rs`（`degraded_hook_missing`、activation_label）、`tui/panels/stats.rs:379-381`。
- [x] web JS/CSS（用 `apply_patch` 精确编辑）：`shell.rs:537-538`、`assets/data/derive.js`、`assets/render/hero.js`、`assets/render/costs.js`、`assets/copy.js`、`assets/components.css:1916-1993`；`web/mod.rs:2089` 脱敏测试改断言字段不存在。
- 验证：`cargo clippy --all-targets --all-features -- -D warnings` + 仓库 `node --check`/`node --test`。目视检查如启动 `cargo run -- serve`，检查完成后必须停止该进程并确认监听端口释放。

## Step 2 — 命令层拆除

- [x] 删 `commands/hook_run.rs` 与 `mod.rs:200-208, :371-375` 注册。
- [x] `init.rs`：摘 `install_all` 与 `--best-effort`，保留 store bootstrap；更新完成文案（指引 `sync`）。
- [x] `uninstall.rs` 在本步暂时保留可编译的旧 dispatch；与 Step 3 的 `cleanup_all` 在同一完整切片切换，不制造未定义符号的中间状态。
- [x] `help.rs:322, 330, 364` 文案。
- [x] `sync.rs:115, 243, 349` `recover_running_runs(&["sync","hook-run"])` 与 `query/mod.rs:837, 2197, 2229`、`home_overview.rs:407, 415` 的 `'hook-run'` 字面量**保留**，各加一行历史标签注释。
- [x] 同一切片内删除或改写被命令删除直接影响的 hook 测试，保持测试集与命令面同步。
- 验证：`cargo test --all-features -- --test-threads=1`；不得把预期红测留到后续提交。

## Step 3 — 遗留清理转型（uninstall 加固）

- [x] `integrations/` 收敛：删 install/probe/`write_hook_wrappers`；`Integration` trait 收敛为 cleanup 面（或直接函数化 `cleanup_all`）。
- [x] 同一切片把 `uninstall.rs` 切换到 `integrations::cleanup_all`，并确认 `--purge` 仍在 cleanup 成功后执行。
- [x] **宽匹配加固**：Claude/Antigravity/Gemini 条目匹配改"command 含 `llmusage-hook`"；覆盖本机实证的两种引号变体 + legacy `--source gemini`；OpenCode 保持 `LLMUSAGE_LOCAL_PLUGIN` 标记；Codex 保持备份恢复、无备份时仅在现值含 `llmusage` 才清。
- [x] **崩溃残留清扫**：目标文件所在目录的 `.{name}.llmusage-tmp.*`/`-pending`/`-recovery`，先执行既有 recovery 协议再清扫。
- [x] Codex：有 `codex_notify_original.json` 时恢复原值并在成功后精确删除该文件；无备份时仅在当前 notify 含 llmusage 标识时清理；其他 notify 保持不变。
- [x] 备份：实际配置变更前创建恢复备份；历史 `*.bak` 与数据库升级备份保留，不做 `backups/*.bak` 扫描删除。
- [x] 幂等：无安装物 → skipped；除命令级 `run_log` 外，不改第三方配置、不创建备份、不写 `integration_install`。
- [x] `atomic.rs` 保留；`hook_target.rs` 若确认清理路径不再生成命令串则删除（ADR 记指引）。
- 验证：`cargo test integrations -- --test-threads=1`

## Step 4 — 领域/注册/store 拆除

- [x] `source_descriptor.rs`：删 `HookActivation`/`HybridActivation`/`hook_signal`/`integration` 能力字段；Codex/Claude/OpenCode 改纯被动；Antigravity descriptor 保留并进入显式 `historical_only`，`platform_monitor.rs` 保留独立 monitor-only / `blocked_no_samples` 条目。
- [x] `registry.rs`：删 `registered_integrations()`；漂移测试（`:168-180`）更新。
- [x] store：删 `trigger.rs` 与 `store.triggers()`；`IntegrationStateStore` 保留（清理审计用）；`HolderKind::Hook` 保留 + deprecated 注释；`lock.rs:214-219` 死函数删除。
- [x] **迁移零改动**（`migrations.rs` 不碰）。
- 验证：`cargo clippy --all-targets --all-features -- -D warnings`

## Step 5 — 测试收敛

- [x] 逐项分类 `tests/local_flow.rs`、`tests/sync_regression.rs`、`tests/tui_panels_prop.rs` 的旧 hook 覆盖：删除 install/probe/hook-run 专属用例与 trigger 助手；把 uninstall 安全用例改写进遗留 cleanup 矩阵；更新已移除 integrations 面板的断言。不得按数量整批删除而丢失清理回归。
- [x] 增（对应 PRD AC）：
  - 遗留清理矩阵：临时 home + env（`CODEX_HOME`/`OPENCODE_CONFIG_DIR`）fixture 预置两种引号变体、legacy gemini、OpenCode 标记文件、无标记文件（不删）、崩溃残留文件、用户自有 hook 条目（结构和值相等断言）、Codex 备份标记消费、历史 `*.bak` 保留、幂等二次运行。
  - 旧库兼容：预置 `trigger_state`/`integration_install` 行 + `hook-run` run_log 行 + `holder_kind='hook'` 历史锁行的库上 sync/serve/报表正常、最近同步时间不变。
  - Antigravity 历史事件照常聚合、来源状态为 `historical_only`、平台探针为 monitor-only / `blocked_no_samples`。
- 验证：`cargo test --all-features -- --test-threads=1`

## Step 6 — 文档 / ADR / spec

- [x] 新 ADR：被动 only 决策，supersede ADR-0001 §5、ADR-0008、ADR-0009 相关结论；记 `hook_target.rs`（SEC-002）从 git 历史找回的指引。
- [x] README 中英价值主张、`docs/index.md:21`、`docs/guide/install-and-init.md` 集成表、`docs/architecture/index.md`、`docs/reference/cli.md`（删 hook-run 节、更新 init/uninstall 语义、升级指引"装过 hooks 的机器升级后运行一次 `llmusage uninstall`"）、`docs/safety/index.md`、全部 zh 镜像。
- [x] spec：`integration-file-contracts.md` 收窄到遗留清理写入；`write-fencing-contracts.md:40-42` trigger_state 豁免标注历史；`source-sync-contracts.md` 激活模式表述更新。
- [x] `docs/agents/passive-source-candidates.md`：Antigravity 行补注"hook 接入已移除，历史数据保留"。
- 验证：`npm --prefix docs run docs:build`

## Step 7 — 收尾

- [x] 全仓 `rg -n "hook[._-]run|hook-run"`：仅剩历史兼容字面量与注释。
- [x] `just ci` 全绿（提交前 `cargo fmt`，注意全局 formatter hook 对 .rs import 的干扰）。
- [x] 仅对已通过相应测试的完整切片做 Conventional Commits（中文 scope），如 `refactor(集成): [AI] ♻️ 移除 hook 实时同步机制` 系列。

## 回滚点

- 使用已完成的任务提交做精确 `git revert`，或只恢复本任务明确触及的文件；不得使用会覆盖无关工作树改动的仓库级 checkout/reset。
- 数据层零改动（无迁移、无删表、无删行），任何回滚都不涉及用户数据。

## 与 grok 任务的顺序约束

本任务改动 `source_descriptor.rs`/`registry.rs`/`source-status` 等文件，与 `07-27-grok-build-passive-source` 触点重叠。**本任务先执行并完成，Grok 任务随后基于 passive-only 终态接入**；不并行。该依赖写入两任务，不依赖树形结构表达。
