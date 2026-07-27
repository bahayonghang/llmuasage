# Hook 实时同步机制代码面梳理（探索代理报告，2026-07-27）

> 结论：hook 机制 = 1 个隐藏 CLI 命令（`hook-run`）+ 4 个 install/probe/uninstall 适配器 + 2 张 SQLite 表（`trigger_state`、`integration_install`）+ 5 个第三方配置写入点。**`sync` 与 `serve` 不读这两张表，无迁移依赖，可停写不删表。** 两个真正的阻塞点：Antigravity 无解析器（hooks 是其唯一数据路径）；web 看板与 TUI 渲染 `integration_install` 行。
>
> 规划裁定（2026-07-27，优先于下方初始推论）：`trigger_state` 完全停写；`integration_install` 仅保留实际遗留 cleanup 的审计写入；Antigravity 进入 `historical_only`；历史 `backups/*.bak` 默认保留，只有成功恢复后的 `codex_notify_original.json` 被精确消费。

## 1. CLI 命令

均在 `src/commands/mod.rs`，无独立 `integration` 子命令：

- `init`（`:77-82`，分发 `:302`）：`--best-effort`；`init.rs:26` 调 `integrations::install_all`。
- `uninstall`（`:174-177`，分发 `:364`）：`--purge` 连带删运行时根目录；`uninstall.rs:23` 调 `uninstall_all`。
- `hook-run`（`:200-208`，分发 `:371-375`，`hide=true`）：`--source/--trigger/--auto`。实现 `commands/hook_run.rs:16-93`：先 upsert `trigger_state`（信号不丢），非阻塞抢 worker 锁失败即静默返回（`:35`）；worker 最多循环 3 次做源过滤 sync（`:53-81`）；结束后链式调用被顶掉的 Codex 原 `notify`（`:95-128`）。

## 2. src/integrations/ 模块

- `mod.rs`：`probe_all`/`install_all`/`uninstall_all` 按 `registry::registered_integrations()` 扇出（`:40-79`）；`write_hook_wrappers` 生成 `.cmd`/`.sh` 包装（`:81-107`）；`backup_file` 纳秒+计数器防撞名（`:117-147`）；`record_probe`/`record_action` 写 `integration_install`（`:149-177`）。
- `integration.rs`：`Integration` trait（source/probe/install/uninstall，`:12-25`）。
- `hook_target.rs`：唯一 `cfg!(windows)` 分支点（`:28-40`）；`shell_command()`/`notify_args()`；`quote_posix`（`:108-117`）是 SEC-002 注入加固。
- `atomic.rs`：崩溃安全写入。临时文件 `.{name}.llmusage-tmp.{pid}.{nanos}.{counter}`（`:308`）、pending 标记、recovery 快照（`:358-411`）；Windows 用 `ReplaceFileW`，绝不先删目标。

### 第三方配置写入点（精确路径与键）

1. **Claude** `~/.claude/settings.json`（`claude.rs:171-174`）：`hooks.Stop[]`/`hooks.SessionEnd[]` 追加 `{"hooks":[{"type":"command","command":"<wrapper> --source claude --trigger ... --auto"}]}`。⚠️ `resolve_claude_settings` 忽略 `app` 参数直接用 `resolve_home_dir()`——`--home` 沙箱不隔离它。
2. **Codex** `$CODEX_HOME/config.toml` 或 `~/.codex/config.toml`（`codex.rs:201-207`）：**覆写**顶层 `notify` 数组（`:100`）；被顶掉的原值存 `~/.llmusage/backups/codex_notify_original.json`（`:83-93`），卸载时恢复（`:135-150`）。
3. **Antigravity** `~/.gemini/config/hooks.json`（`antigravity.rs:213-216`）：顶层 `Stop[]`，条目为扁平 `{"type":"command","command":"..."}`（无 Claude 那层 `hooks` 包装）。
4. **Legacy Gemini** `~/.gemini/settings.json`（`antigravity.rs:218-221`）：仅清理——移除命令串同时含 `llmusage-hook` 与 `--source gemini` 的 `hooks.SessionEnd[]` 条目（`:285-287`, `:345-373`）。
5. **OpenCode** `$OPENCODE_CONFIG_DIR/plugin/llmusage-tracker.js` 或 `{config_dir}/opencode/plugin/llmusage-tracker.js`（`opencode.rs:298-307`）：整文件 llmusage 所有，`// LLMUSAGE_LOCAL_PLUGIN` 标记（`:16`），卸载仅在标记存在时删除（`:129-133`）；JS 内嵌命令走 `escape_js_template_literal`（`:314-318`）。

## 3. Store 层

- **TriggerStore**（`store/trigger.rs`）：`trigger_state` 表（source PK、last_signal_at、trigger、worker 起止、updated_at）。生产读写仅 `hook_run.rs`（另有 `tests/sync_regression.rs:2552-2557` 测试助手）。⚠️ `upsert_trigger_state` 内联 `CREATE TABLE IF NOT EXISTS`（`:28-38`），走控制面、绕过写栅栏（`write-fencing-contracts.md:40-42` 明文豁免）。
- **IntegrationStateStore**（`store/integration.rs`）：`integration_install` 表。读者更广：`query/mod.rs:517-518, 528-529` 把 `Vec<IntegrationState>` 放进 `HealthPayload`/`HealthSummaryPayload`，抵达看板与 TUI。
- 访问器：`store.triggers()`（`store/mod.rs:357-360`）、`store.integration_state()`（`:342-345`）。`HolderKind{Cli,Library,Hook}` 在 `store/mod.rs:213-233`；`HolderKind::Hook` 唯一生产调用点是废弃的非阻塞 acquire（`lock.rs:214-219`）。

## 4. 领域层与各源激活模式

`source_descriptor.rs`：`ActivationMode`（`:11-20`）、`HookActivation`（`:24-31`）、`HybridActivation`（`:51-54`）、`SourceCapabilities.hook_signal`（`:64`）。

| Source | 行 | 激活 | hook 事件 | parser | integration | hook_signal | passive_probe |
|---|---|---|---|---|---|---|---|
| Codex | `:111-135` | Hybrid | `notify`（单例，可被动回退） | 有 | 有 | 有 | 无 |
| Claude | `:136-160` | Hybrid | `Stop`/`SessionEnd`（可被动回退） | 有 | 有 | 有 | 无 |
| OpenCode | `:161-177` | Plugin | `session.updated` | 有 | 有 | 有 | 无 |
| Antigravity | `:178-196` | **Hook** | `Stop`，`passive_fallback: false` | **无** | 有 | 有 | 无 |
| Kimi Code | `:197-217` | Passive | — | 有 | 无 | 无 | 有 |
| Pi | `:218-239` | Passive | — | 有 | 无 | 无 | 有 |

## 5. 概念泄漏点

- **registry**：`registry.rs:54-60` 返回 4 个 Integration；`:78-107`、`:168-180` 漂移测试断言 descriptor.capabilities.integration 与注册表严格一致。
- **doctor**：`doctor.rs:60` probe_all；`:70-84` `hook_cmd_path`/`hook_sh_path` 存在性检查；`:153-169` 检查 id `codex.notify`/`claude.hooks`/`opencode.plugin`/`antigravity.hooks`/`kimi_code.passive`/`pi.passive`。
- **source-status**：`source_status.rs:190`（Hook|Plugin|Hybrid 匹配）、`:194`（`degraded_hook_missing`）、`:231-237`（activation_label）、测试 `:259-318`。
- **status/diagnostics**：`status.rs:57`；`diagnostics.rs:52-58, 71`（`hook_cmd_path`/`hook_sh_path`/`bin_dir`/`integrations`/`integration_records`）。
- **sync**：`sync.rs:115, 243, 349` 的 `recover_running_runs(&["sync","hook-run"])` 是唯一耦合；sync 不碰两张表。
- **web 看板**：`web/shell.rs:537-538`（`#integrations-rows`）；`assets/data/derive.js:304, 340, 413, 417-418`；`assets/render/hero.js:79-122`；`assets/render/costs.js:135-153`；`assets/copy.js` i18n；`assets/components.css:1916-1993`。测试 `web/mod.rs:2089`（公开投影脱敏断言）。
- **TUI**：`tui/panels/stats.rs:379-381`。
- **help**：`help.rs:322, 330, 364`。
- **测试**：`tests/local_flow.rs` 8 个 hook 测试（含 `hook_run_syncs_only_triggered_source:489`、`init_writes_quoted_windows_string_commands_for_spaced_paths:312`、`antigravity_install_cleans_legacy_gemini_hooks:379`）；`tests/sync_regression.rs` 5 个（`:681, :744, :1437` 等）；`tests/tui_panels_prop.rs:20, 483`。`architecture_dependencies.rs`、`public_api.rs` 零引用。
- **文档**：`docs/guide/install-and-init.md:54-59`（集成表）及 zh 镜像 `:50-55`；`docs/architecture/index.md:19-38` 多处；`docs/reference/cli.md:320-322`（hook-run 隐藏命令节）；`docs/index.md:21`（落地页价值主张即 "Hooks and plugins trigger local parsing"）；`docs/safety/index.md:18-20`；`README.md:61, 71-75`；`README.zh-CN.md:59, 69-73`。
- **ADR**：ADR-0001 §5 把 `HookTarget` 定为平台分支聚合点（`:82-84`）；ADR-0008 有 `## Hook-run consequence` 节（`:31-33`）并定义激活模式枚举（`:22`）；ADR-0009 确立 Antigravity integration-only（`:21-22`）。三者需**新 ADR 取代**而非就地改。
- **spec**：`integration-file-contracts.md` 是第三方写入的规范文件（三个入口函数、同级临时文件规则、ReplaceFileW、崩溃恢复协议、record_action 失败补偿、5 类必备测试）；`write-fencing-contracts.md:40-42` 给 `trigger_state` 的控制面豁免。

## 6. 移除影响判定

**不破坏**：`sync`/`serve` 不读两张表；`sync_writer` 完全解耦；迁移只在 migration 1 建表（`migrations.rs:292-299, 311-318`）、migration 13 改写 gemini→antigravity（`:600-601`），从不 drop——干净路径是**保留迁移、停止写入**。`schema.rs:261` 的 `reset_usage_data` 本就排除这两张表。

**破坏（按工作量排序）**：
1. **Antigravity 失去唯一数据路径**（parser:false + passive_fallback:false）——产品决策而非重构。
2. **看板+TUI** 渲染 `integration_install`（6 个 JS/CSS 文件 + stats.rs）；`HealthPayload` 字段删除需 Rust+JS 协同；⚠️ 全局 prettier hook 与仓库单引号 JS 风格冲突，JS 文件须用 Bash 而非 Edit 工具改。
3. **registry 漂移测试**（`:168-180`）两侧必须同步改。
4. **13 个左右 hook 测试**直接删除。
5. **`backups_dir` 必须保留**——`schema.rs:286-287` 用它存 `llmusage.db.pre-0.5.0` 备份，与集成无关；只有 `bin_dir` 是 hook 专属。
6. **`HolderKind::Hook`** 成死变体但 `holder_kind` 是持久化列——保留变体。
7. `recover_running_runs` 与 `command IN ('sync','hook-run')` 查询（`query/mod.rs:837, 2197, 2229`；`home_overview.rs:407, 415`）中的 `'hook-run'` 字面量成死值但**不能删**——历史行仍带该标签，删了会悄悄改变既有库的"最近同步"报告。

## 7. 用户机器清理路径（卸载必须访问的位置）

1. `~/.claude/settings.json` → `hooks.Stop[]`/`hooks.SessionEnd[]` 匹配条目
2. `$CODEX_HOME/config.toml` 或 `~/.codex/config.toml` → 用 `~/.llmusage/backups/codex_notify_original.json` 恢复/移除 `notify`
3. `~/.gemini/config/hooks.json` → `Stop[]` 匹配条目（`--source antigravity` 与 legacy `--source gemini` 两种）
4. `~/.gemini/settings.json` → legacy `hooks.SessionEnd[]` llmusage 条目
5. OpenCode plugin 文件 → 带 `LLMUSAGE_LOCAL_PLUGIN` 标记才删
6. 自有运行时：删除 `~/.llmusage/bin/llmusage-hook.{cmd,sh}`；历史 `~/.llmusage/backups/*.bak` 保留。成功恢复 Codex notify 后只删除一次性标记 `codex_notify_original.json`。
7. **崩溃残留**：上述 1-5 旁的 `.{name}.llmusage-tmp.*`、`.{name}.llmusage-pending`、`.{name}.llmusage-recovery`——若删掉原子写入器而不做残留清扫，恢复协议消失后这些文件将永久搁浅。

⚠️ 路径 1-5 经 `resolve_home_dir()` 与环境变量（`CODEX_HOME`/`OPENCODE_CONFIG_DIR`/`OPENCODE_HOME`/`OPENCODE_DB`）解析，不走 `app.paths`——`--home` 不能沙箱化它们，只有环境变量覆盖可以。
