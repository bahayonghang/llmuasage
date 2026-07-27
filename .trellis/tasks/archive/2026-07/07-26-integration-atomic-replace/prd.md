# Windows integration 原子替换闭环

## Goal

让 integration 外部配置更新在 Windows 崩溃、权限失败和 action 记录失败时保持可恢复，不出现先删目标导致的缺失窗口。

## Confirmed Evidence

- `src/integrations/mod.rs:185-192` 的 Windows 分支先 `remove_file(target)` 再 `rename(temp, target)`。
- crash 发生在两步之间时目标配置消失。
- 各 integration 在文件写入成功后调用 `record_action`；记录失败时没有补偿或回滚。
- crate 当前没有直接 Windows API binding 依赖。

## Requirements

- Windows 已存在目标使用 OS 原子 replace primitive，不得先删除目标。
- temp file 必须位于 sibling directory，写入后 flush/fsync，并在成功/失败后清理。
- 保留目标权限；必要时同步 parent directory 的 durability 语义并说明平台限制。
- 外部文件变更与 action 记录采用可恢复协议：记录失败时恢复旧内容，或预写 intent 后完成/补偿。
- 测试全部使用临时配置根，不接触真实用户配置。
- 允许新增仅 Windows target 编译的直接 `windows-sys` 依赖，以使用经过维护的 Win32 API binding；不采用自维护 unsafe FFI。

## Acceptance Criteria

- [ ] failpoint 覆盖 write、flush、replace、record_action；每个失败点后目标 digest 为完整 old 或完整 new，不缺失、不截断。
- [ ] Windows 实测覆盖“目标存在”和“目标不存在”两条路径。
- [ ] action 记录失败后状态可恢复且下次运行能识别未完成 operation。
- [ ] Claude、Codex、OpenCode、Antigravity integration 安装/卸载回归通过。
- [ ] temp/backup 文件无无界残留。
