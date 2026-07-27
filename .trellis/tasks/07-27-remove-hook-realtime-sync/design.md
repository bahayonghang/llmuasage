# 技术设计：移除 hooks 实时同步，收敛为被动 only

前置阅读：`research/hook-surface-map.md`（逐文件代码面）、`research/local-hooks-inventory.md`（本机实证与匹配缺陷）、`.trellis/spec/llmusage/backend/{integration-file-contracts,write-fencing-contracts,source-sync-contracts}.md`。

## 设计原则

1. **删实时行为、不删数据**：SQLite 表、迁移、历史行、`reset_usage_data` 排除清单全部不动；`trigger_state` 停写，`integration_install` 只保留实际遗留清理的审计写入。
2. **保留一条会写第三方配置的路径**（遗留清理），因此 `atomic.rs` 原子写入协议与 `integration-file-contracts.md` 继续有效，只是范围收窄。
3. **每步可编译可测**，按依赖方向从外向内拆除：呈现层 → 命令层 → 领域/注册层 → 集成模块内部。

## 目标终态

```
之前：sync(被动) + hook-run(实时) 双路径；integrations 负责 install/probe/uninstall
之后：sync(被动) 单路径；integrations 收敛为 legacy_cleanup（原 uninstall 逻辑，宽匹配加固）
```

### 命令终态

| 命令 | 终态 |
| --- | --- |
| `init` | 只做 store bootstrap + 首次 sync 提示；删除 `install_all` 调用与 `--best-effort`（无集成安装即无部分失败语义） |
| `uninstall` | 语义改为"清理遗留 hook 安装物"；`--purge` 行为不变（删 `~/.llmusage`） |
| `hook-run` | 整体删除（命令、分发、`commands/hook_run.rs`） |
| `status`/`doctor`/`source-status`/`diagnostics` | 移除 integration/hook 字段与检查项；被动 probe 保留 |

### 领域模型终态

- `ActivationMode`：全部现役源均为 Passive。若枚举仅剩一个变体，直接删除枚举与 `activation` 字段（source-status 的 activation_label 一并删）；保留单变体枚举没有信息量。
- 删除 `HookActivation`、`HybridActivation`、`SourceCapabilities { integration, hook_signal }`（字段级删除，`parser`/`passive_probe` 保留）。
- Codex/Claude/OpenCode descriptor：改纯被动（它们的解析器本就是被动读文件/库）。
- **Antigravity**：`SourceKind` 与 descriptor 不可移除，因为 `SourceKind::parse_id` 通过 descriptor 解析历史数据库行。移除 integration 字段后，不新增单独生命周期抽象；source-status 由现有能力组合 `parser: false && passive_probe: false` 推导 `historical_only`，不能复用 passive 状态。`platform_monitor.rs` 同时保留独立 monitor-only / `blocked_no_samples` 探针，标注"历史数据保留，实时接入已移除，等待被动样本"。查询/报表继续按 SourceKind 聚合历史行。
- `registry.rs`：删除 `registered_integrations()`；漂移测试改为断言"registry 无集成注册"或直接删除。

### store 终态

- `TriggerStore`（`trigger.rs`）与 `hook_run.rs` 一起删除；`trigger_state` 表留在库中成为孤表（迁移 1 继续创建它——**不改迁移**，新库也带这张空表，代价可忽略，换取迁移历史零风险）。
- `IntegrationStateStore` **保留**：仅当 cleanup 实际修改外部配置或清理失败时，`record_action` 写 `integration_install`；纯 no-op 不写。`load_integration_states` 不再进入 `HealthPayload`。
- `HolderKind::Hook` 变体保留 + `#[deprecated]` 或注释说明（列有历史值；serde/解析路径必须继续接受）。
- `lock.rs:214-219` 废弃非阻塞 acquire：唯一调用方（hook_run）消失后按 clippy 指引删除该函数，但保留 `HolderKind` 解析兼容。
- `query/mod.rs` 与 `home_overview.rs` 中 `command IN ('sync','hook-run')` **保持原样**，加一行注释说明 `'hook-run'` 是历史 run_log 标签。

### 遗留清理（uninstall 转型）设计

保留各集成文件的 uninstall 函数，收敛进 `integrations/legacy_cleanup.rs`（或维持分文件、mod.rs 只导出 `cleanup_all`），并做四处加固：

1. **宽匹配**（修复 `local-hooks-inventory.md` 实证缺陷）：Claude/Antigravity/Gemini 条目匹配改为"`command` 字符串包含 `llmusage-hook`"（这是包装脚本文件名，历史所有版本共有、且不会出现在用户自己的 hook 里；比拼完整命令串稳健）。OpenCode 维持 `LLMUSAGE_LOCAL_PLUGIN` 标记校验。Codex 维持 notify 恢复逻辑：备份存在则恢复原值，否则仅当现值含 `llmusage` 时清空。
2. **崩溃残留清扫**：对 §7 的 1-5 每个目标文件，额外扫其所在目录下 `.{name}.llmusage-tmp.*` / `.{name}.llmusage-pending` / `.{name}.llmusage-recovery` 并删除（先跑一次现有 recovery 协议再清扫，避免丢真正未落盘的恢复数据）。
3. **幂等**：所有步骤"无安装物→skipped"。纯 no-op 与二次运行允许命令级 `run_log`，但不改第三方配置、不创建备份、不写 `integration_install`。
4. **备份所有权**：不扫描或批量删除 `backups/*.bak`。每次实际变更前的新恢复备份与历史 integration `*.bak` 均保留；数据库升级备份始终保留。`codex_notify_original.json` 是一次性恢复标记，不是历史备份：成功恢复 notify 后精确删除，后续运行不得再次覆盖用户 notify。

环境变量解析（`resolve_home_dir`、`CODEX_HOME`、`OPENCODE_CONFIG_DIR` 等）原样保留——测试沙箱依赖 env 覆盖（`--home` 不隔离这些路径，见 research §7 警告）。

### web/TUI 呈现清理

- Rust 侧：`HealthPayload`/`HealthSummaryPayload` 删 `integrations` 字段（`query/mod.rs:517-529`）；`web/shell.rs:537-538` 面板结构删除；`web/mod.rs:2089` 脱敏测试改为断言字段不存在。
- JS/CSS 侧使用 `apply_patch` 精确编辑：`assets/data/derive.js`（`ready_integrations`/`total_integrations`）、`assets/render/hero.js:79-122`、`assets/render/costs.js:135-153`、`assets/copy.js` i18n 键、`assets/components.css:1916-1993`。不运行全局 formatter；跑仓库自带 `node --check`/`node --test` 验证。
- TUI：`tui/panels/stats.rs:379-381`。

## 拆除顺序（依赖方向）

1. 呈现层（web/TUI/doctor/status/diagnostics/source-status 的 hook 字段）—— 消费者先走，`HealthPayload` 字段删除会编译器指路。
2. 命令层：删 `hook-run`；`init` 摘 `install_all`；`uninstall` 换 `cleanup_all`。
3. 领域/注册层：descriptor 收敛、registry 摘集成、漂移测试更新。
4. integrations 模块内部：删 install/probe/`write_hook_wrappers`/`hook_target.rs`（若遗留清理不再需要生成命令串——Claude 宽匹配不需要；Codex 恢复不需要；确认后删）、trait 收敛。
5. store：删 TriggerStore；处理 lock.rs 死代码。
6. 测试增删、文档、ADR、spec。

## 风险与权衡

- **Antigravity 用户感知**：看板"来源分布"仍显示其历史量但不再增长。文档与 source-status 文案必须解释，否则像 bug。
- **hook_target.rs 的去留**：SEC-002 注入加固逻辑若彻底删除，遗留清理不受影响（清理不生成命令串）；但若未来重新引入任何命令写入，需从 git 历史找回。在 ADR 中记一句指引。
- **孤表**：新库仍创建 `trigger_state` 空表；`integration_install` 只记录实际遗留清理审计。接受；改历史迁移的风险更高。
- **回滚**：整任务单方向删除；仅在一个完整切片的相关测试已绿后提交，回滚使用精确任务提交或明确文件列表，不依赖工作树级 checkout/reset。

## 兼容性

- 旧库（含 trigger/integration 行、`hook-run` run_log 行、`holder_kind='hook'` 锁历史）：读路径全部兼容，验收标准有专项测试。
- 已装 hooks 的其他机器：升级后 hook 包装脚本仍被第三方工具调用但 `hook-run` 命令已不存在 → 包装脚本调用会失败。**缓解**：`uninstall` 文案与 CHANGELOG/README 明确指引升级后运行一次 `llmusage uninstall`；hook 失败在 Claude/Codex 侧是非阻塞的（hook 失败不影响工具本身），可接受过渡期。
