# Integration 文件写入安全（SEC-002、REL-002/003/004）

## Goal

使 integration 安装/卸载对外部工具配置文件的修改具备正确的 quoting、crash-atomic 写入、唯一备份与诚实的退出码。

## 覆盖发现（已核实）

- **SEC-002（P2，条件触发）**：`src/integrations/hook_target.rs:99-110` `quote_unix_path` 用双引号包裹且仅 escape `"`，未防 `$()`、backtick、backslash——路径含特殊字符时 shell 会展开；`src/integrations/opencode.rs:304-319` 再把整条 command 插入 JS template literal，二次注入面。exe/home 位于恶意特殊字符路径时可改变 shell/JS 语义，潜在本地代码执行。
- **REL-002（P2）**：`src/integrations/mod.rs:47-71` `install_all` 把每个 install error 转成 `IntegrationAction{status:error}` 后整体仍返回 `Ok(Vec)`；`src/commands/init.rs:24-39` 打印后返回 0。自动化认为 init 成功但 hooks/plugin 可能未装，后续 sync 数据缺失难定位。
- **REL-003（P2）**：`src/integrations/claude.rs:82-108,127-153`、`src/integrations/opencode.rs:68-99`、`src/integrations/antigravity.rs:131,176,239,253` 先 backup 后直接 `fs::write` 目标 config/plugin/wrapper；无 temp+fsync+rename、无 permission 保留、无失败回滚。crash/disk-full/杀软干扰可留下截断配置。
- **REL-004（P2）**：`src/integrations/mod.rs:105-111` `backup_file` 文件名仅用秒级 `now_utc()` 文本，`fs::copy` 默认覆盖已存在目标。同 stem 同秒执行会覆盖真正的"原始备份"。

## Requirements

1. 尽量用 argv（已有 `notify_args`）；必须生成 shell 字符串处改用 POSIX 单引号 escaper；Windows 用成熟 quoting；注入 JS 处通过 JSON string literal/参数数组；对 quoting 增加 property tests。
2. `AtomicConfigWriter`：sibling temp、flush/fsync、preserve mode、atomic rename；record_action 失败时回滚；覆盖 Windows replace 兼容性。
3. required integration 任一失败返回 typed `PartialFailure` 与非零 exit code；提供显式 `--best-effort` 保留旧行为。
4. 备份文件名加 nanosecond/UUID/content hash 保证唯一；`create_new(true)` 防覆盖；manifest 记录 source、digest、time。

## Acceptance Criteria

- [ ] 含 `$()`、backtick、`${}`、空格、引号的可执行路径生成的配置语义保持 literal（property test，在旧实现上稳定失败）。
- [ ] disk-full/failpoint/permission 测试：写入失败后原文件 digest 不变。
- [ ] init 局部失败退出码非零且输出定位到具体 integration；`--best-effort` 时保持 0。
- [ ] 同秒重复安装/卸载不覆盖既有备份。
- [ ] 三平台（或明确声明的平台范围）安装/卸载回归通过。

## Notes

- 审计报告 §3.2.5；§6.3 quoting 测试场景。
- 复杂任务：启动前需补 design.md（AtomicConfigWriter 协议与 Windows replace 策略）+ implement.md。
