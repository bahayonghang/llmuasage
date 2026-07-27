# 移除 hooks 实时同步机制，仅保留被动 sync 读取

## Goal

llmusage 当前有两条数据路径：被动解析（`sync` 读本地产物）与 hook/plugin 实时触发（安装进 Claude/Codex/OpenCode/Antigravity 的配置，会话结束时调 `llmusage hook-run` 增量同步）。用户裁定只保留被动 sync；本任务移除 hook 实时机制的安装、触发与呈现面，并保留一条**修复过匹配缺陷的遗留清理路径**，供任何装过旧版 hooks 的机器摘除残留。

依据：`research/hook-surface-map.md`（代码面全图）、`research/local-hooks-inventory.md`（本机安装现状 + 精确匹配卸载缺陷证据）。本机（开发者机器）的已装 hooks 已于 2026-07-27 手动清理完毕。

## 已确认的产品决策

- **D1（Antigravity 数据停摆，2026-07-27 已确认）**：Antigravity 是唯一纯 hook 数据源（`parser: false`、`passive_fallback: false`），hooks 移除后不再产生新事件。历史事件完整保留并继续可查可看；Antigravity 进入显式 `historical_only` 来源状态，同时平台探针保持 monitor-only / `blocked_no_samples`，未来若有被动解析方案另开任务。**不**用一个尚无 token fixture 的 Antigravity 被动解析器阻塞本任务。
- **D2（保留遗留清理，2026-07-27 已确认）**：`uninstall` 命令保留并转型为"遗留 hook 清理 + 可选 purge"；`init` 保留但只做数据库引导，不再安装任何集成。cleanup 仅在实际清理或清理失败时写 `integration_install` 审计；历史 `*.bak` 默认保留。

## Requirements

### 移除面

- R1. 删除 `hook-run` 隐藏命令（`src/commands/hook_run.rs` 及 `mod.rs` 注册）与 `~/.llmusage/bin` hook 包装脚本生成（`integrations/mod.rs::write_hook_wrappers`）。
- R2. 删除四个集成的 install/probe 路径（`integrations/{claude,codex,antigravity,opencode}.rs` 的 install 与 probe、`registry.rs:54-60` 注册、`Integration` trait 的 install 面）；`init` 不再调用 `install_all`。
- R3. `trigger_state` 完全停止写入；`integration_install` 停止 install/probe 写入，仅保留 cleanup 实际修改或失败时的审计写入。**不改任何迁移、不 drop 表**（历史库兼容）。`HolderKind::Hook` 枚举变体保留（持久化列含历史值），标注 deprecated。`recover_running_runs` 与 `command IN ('sync','hook-run')` 查询中的 `'hook-run'` 字面量**保留**（历史行仍带该标签，删除会改变既有库的最近同步报告）。
- R4. 领域模型收敛：删除 hook/plugin 激活类型与 `SourceCapabilities.hook_signal` / `integration`；Codex/Claude/OpenCode descriptor 表达纯被动解析。Antigravity 的 `SourceKind` 与 descriptor 必须保留以解析历史数据库行，但明确标记为 `historical_only`，不得落入 `passive_ready` / `passive_no_data`；平台探针独立显示 monitor-only / `blocked_no_samples`。registry 与 source-status 测试同步更新。
- R5. 呈现面清理：doctor 的 `hook_cmd_path`/`hook_sh_path` 检查与 `*.notify`/`*.hooks`/`*.plugin` 检查 id、source-status 的 `degraded_hook_missing` 与 activation 标签、status/diagnostics 的 integration 字段、web 看板 integrations 面板（shell.rs + 6 个 JS/CSS 文件 + `HealthPayload`/`HealthSummaryPayload` 字段）、TUI stats 面板、help 文案。
- R6. 测试清理：删除过时的 install/probe/hook-run 测试；原 uninstall 覆盖改写为遗留 cleanup 矩阵，不能把清理安全回归一并删除。同步更新 `tests/local_flow.rs`、`tests/sync_regression.rs`、`tests/tui_panels_prop.rs` 的相关用例与助手。

### 保留的遗留清理路径

- R7. `uninstall`（不带 `--purge`）保留并覆盖：Claude settings、Codex notify、Antigravity hooks.json（`--source antigravity` 与 legacy `--source gemini`）、legacy Gemini settings、OpenCode plugin（`LLMUSAGE_LOCAL_PLUGIN` 标记校验）、自有 bin 包装，以及第三方配置旁的原子写入崩溃残留（`.{name}.llmusage-tmp.*`/`-pending`/`-recovery`）。Codex 成功恢复原 notify 后消费并删除精确文件 `codex_notify_original.json`，保证二次 uninstall 不会再次覆盖用户后续修改。
- R8. **修复精确匹配缺陷**：条目匹配从"当前版本命令串精确相等"改为宽匹配（命令串含 `llmusage-hook` 或等价稳定标识），覆盖历史引号格式变体（本机实证存在两种，见 `local-hooks-inventory.md`）；只删 llmusage 自有条目，用户其他 hook 一律不动。
- R9. 遗留清理继续走 `integration-file-contracts.md` 的原子写入协议（`atomic.rs` 保留），含变更前备份与 record 失败补偿语义。历史 integration `*.bak` 与 `llmusage.db.pre-0.5.0` 默认保留；不得用 `backups/*.bak` 宽匹配删除。无安装物时可写命令级 `run_log`，但不得改第三方配置、创建备份或写 `integration_install`。

### 行为与文档

- R10. `sync`/`serve`/报表行为不变（两者本就不读 hook 表）；被动源（claude/codex/opencode/kimi_code/pi，及新任务的 grok）不受影响。
- R11. 文档全面更新：README 中英（价值主张改被动 sync）、`docs/index.md:21`、`docs/guide/install-and-init.md` 集成表、`docs/architecture/index.md`、`docs/reference/cli.md`（hook-run 节删除、uninstall 语义更新）、`docs/safety/index.md` 及全部 zh 镜像。
- R12. ADR：新增一篇 ADR 记录"被动 only"决策，**取代**（supersede）ADR-0001 §5（HookTarget 聚合点）、ADR-0008（hook-run 后果与激活模式）、ADR-0009（Antigravity integration-only）的相关结论；不就地改旧 ADR。
- R13. spec 同步：`integration-file-contracts.md` 收窄到遗留清理写入；`write-fencing-contracts.md:40-42` 的 `trigger_state` 控制面豁免标注为历史；`source-sync-contracts.md` 激活模式表述更新。

## Constraints

- 数据零丢失：不删表、不删历史事件、不动 `reset_usage_data` 排除清单、不改迁移。
- JS/CSS 文件使用 `apply_patch` 精确编辑；不运行会改写无关文件的全局 formatter，改后执行仓库既有 `node --check` / `node --test`。
- 实施基线 `dev` 分支。
- 分阶段可编译可测：任何提交点都必须通过该阶段声明的检查，不允许提交已知红测。
- 执行顺序：本任务先于 `07-27-grok-build-passive-source` 完成，使后者直接基于 passive-only descriptor/source-status 终态接入。

## Out of Scope

- 不为 Antigravity 猜测或新建无 fixture 支撑的被动解析器。
- 不删除或重写历史 SQLite 迁移、表、事件、run_log 或 holder kind。
- 不批量删除 `backups_dir` 中的历史 `*.bak` 或数据库升级备份。
- 不改变 `uninstall --purge` 删除整个 llmusage runtime root 的既有显式语义。

## Acceptance Criteria

- [x] `llmusage init` 只引导数据库，不写任何第三方配置；`llmusage --help` 与 `docs/reference/cli.md` 无 hook-run。
- [x] 在预置全部历史格式变体的临时 home/env fixture 上运行 `uninstall`：llmusage 条目全部摘除；用户其他配置的结构和值保持不变（允许 JSON pretty-print 改变空白）；Codex notify 从备份恢复且 `codex_notify_original.json` 被消费；历史 `*.bak` 保留；残留文件清扫；二次运行不改第三方配置、不创建备份、不写 `integration_install`。
- [x] 含历史 `trigger_state`/`integration_install` 行与 `hook-run` run_log 行的旧库：sync/serve/报表正常，最近同步时间报告不变。
- [x] Antigravity 历史事件在报表与看板中照常出现；来源状态明确为 `historical_only`，平台探针独立为 monitor-only / `blocked_no_samples`；文档说明其数据停止增长的原因。
- [x] 看板与 TUI 无 integrations 面板残留；`HealthPayload` 无 integration 字段；web 公开投影脱敏测试相应更新。
- [x] 全仓 `rg -n "hook[._-]run|hook-run"` 仅剩查询 IN 列表、迁移、ADR/spec 的历史兼容字面量与解释性注释。
- [x] `just ci` 全绿；`npm --prefix docs run docs:build` 通过。
