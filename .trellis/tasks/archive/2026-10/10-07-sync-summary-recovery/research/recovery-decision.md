# 2026-10-08 恢复范围决定

用户对规划审阅里两条互斥路线的回复是 `1A 2A`。这里只记录已经确认的产品边界。实现细节在 `design.md`。

## TPR-02 · 只限 Antigravity CLI 和 IDE

接受丢失并在同一次普通 sync 里重建，只适用于 `antigravity` 和 `antigravity_ide`。

- 这两个产品走现有 staged snapshot。失败时该产品的事件、cursor、inventory 和记账版本一起留下，不宣称修复完成。
- 不调用其他来源使用的 `reset_sources_for_rebuild`。那次删除在独立事务里提交，后续读失败不能把它滚回去。
- Codex、Grok、Pi 以及其他旧记账来源保持现在的整源跳过。表后仍给出 `llmusage sync --rebuild --source <source>`。本任务不给它们补原子回滚。
- Web、TUI、HTTP 和 `llmusage serve` 不能借这次选择放宽 `--allow-lossy-rebuild`。

## TPR-04 · 有时间窗口时不询问

带 `--recent-days` 的普通 sync 发现可恢复缺口时，不弹出选择，也不把该来源悄悄改成无窗口重建。

- 有窗口的重建不会替换完整历史，也不推进完整 cursor。
- 交互终端只在表后说明：历史已保留，去掉 `--recent-days` 后再运行 `llmusage sync` 才会出现选择。
- 非交互运行不能指望下一次普通 sync 会询问。表后仍给出不带 `--recent-days` 的显式命令：`llmusage sync --rebuild --source <source> --allow-lossy-rebuild`。

## 其余审阅路线

以下没有第二条产品路线，按审阅报告改规划：

- 旧记账来源被移出写入集合后，提示数量来自本轮只读覆盖，不用来自磁盘已经变化的旧 `source_issues`。
- 询问还要求 stdout 是终端。stdout 重定向、stderr 重定向、stdin 管道和 `--json-events` 都不读 stdin。
- 验收命令按 Cargo 的单个过滤参数拆开，并带上仓库要求的 `--locked --all-features -- --test-threads=1`。
- 先改 `token-accounting-contracts.md` 的交互例外，再改 engine。
- 双语 README 和仍在描述这条 sync 行为的 docs 一起改。不包含安装或发布。
