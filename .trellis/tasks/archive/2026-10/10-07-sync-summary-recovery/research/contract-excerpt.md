# 本任务要遵守的现有契约

全文仍以 `.trellis/spec/llmusage/backend/source-sync-contracts.md` 和 `token-accounting-contracts.md` 为准。这里只保留实施时会直接碰到的条款。

## 普通同步不重建

`token-accounting-contracts.md` 第 24–35 行：普通 `llmusage sync` 发现旧记账后，从写入集合移除该来源，保留历史，不推进 marker，也不发出 `TokenAccountingRepairFinished`。显式修复是 `llmusage sync --rebuild --source <source>`。缺文件时还要 `--allow-lossy-rebuild`。

2026-10-08 确认的例外只限无窗口人读 sync：stdin、stdout、stderr 都是终端，用户明确接受 `antigravity` 或 `antigravity_ide` 的丢失。这等价于同一进程里对这两个产品执行无窗口的 `sync --rebuild --source <source> --allow-lossy-rebuild`。未确认、有 `--recent-days`、任一标准流不是终端、`--json-events`，以及其他旧记账来源，继续只跳过。契约文件要先写下这个例外，再改 engine。

## 人读摘要

`source-sync-contracts.md` 第 305–314 行：`Sync finished` 是对齐表，列包含 files/changed/skipped/seen/committed/stored、bytes、parse、write。颜色只在 stdout 终端、宽度计算之后应用。表以 `TOTAL` 结束。窄终端只压缩来源名。显示变化不改变 `SyncEvent` / `SourceSyncStats` 的线格式。

当前实现把 `last_error` 和 parse issue 插在来源行后面。本任务把它们移到 `TOTAL` 之后，表本身仍遵守这些列和宽度规则。

## Antigravity 历史保护

`source-sync-contracts.md` 第 156–184 行：

- 缺成员、读失败或取消时保留该组旧事件和 cursor。
- 全部选中产品已被阻断时，不打开 usage 表、不解码。发现和指纹成本仍可发生，但不能报成零 I/O 的假象。
- 已跟踪路径不在本次发现中时，要区分文件真的没了、文件还在但不在发现范围、以及无法读取。旧 JSON 和换根路径保留历史。
- `--allow-lossy-rebuild` 不豁免发现失败或读取失败。
- 显式重建不走 engine 的预删除。一次事务替换有路径归属的 parser 行、cursor 和 marker；空路径 hook 行保留。

## 诊断分类

`source-sync-contracts.md` 第 720–732 行附近：malformed 和 oversized 才用警告色表示故障。源级阻断不能记成 malformed 样本。doctor 对只有 skipped 的来源不告警，对来源失败要告警。
