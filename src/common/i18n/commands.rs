//! Human command presentation. Persisted and protocol values remain canonical.
pub(crate) mod pricing;
pub(crate) mod remote_import;
pub mod reports;

// Each arm is a stable presentation id with paired, compile-time format templates.
macro_rules! message {
    (@pair $en:literal, $zh:literal $(, $($args:tt)*)?) => {
        match $crate::common::i18n::current() {
            $crate::common::i18n::Language::English => format!($en $(, $($args)*)?),
            $crate::common::i18n::Language::Chinese => format!($zh $(, $($args)*)?),
        }
    };
    (InitFinished $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Init finished. Run `llmusage sync` to import local usage artifacts.", "初始化完成。运行 `llmusage sync` 导入本地用量文件。" $(, $($args)*)?) };
    (Exported $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Exported HTML bundle to {}", "已导出 HTML 文件包到 {}" $(, $($args)*)?) };
    (StatusTitle $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Status:", "状态：" $(, $($args)*)?) };
    (StatusDb $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- DB: {}", "- 数据库：{}" $(, $($args)*)?) };
    (StatusBuckets $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Buckets: {}", "- 时间桶：{}" $(, $($args)*)?) };
    (LastSync $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Last sync: {}", "- 最近同步：{}" $(, $($args)*)?) };
    (LastExport $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Last export: {}", "- 最近导出：{}" $(, $($args)*)?) };
    (SourceTotal $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Source {}: total={} last={}", "- 来源 {}：合计={} 最近={}" $(, $($args)*)?) };
    (LockHeld $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Worker lock: holder={} expires={}", "- 工作锁：持有者={} 到期={}" $(, $($args)*)?) };
    (LockIdle $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Worker lock: idle", "- 工作锁：空闲" $(, $($args)*)?) };
    (RecentError $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Recent error: {} Original detail: {}", "- 最近错误：{} 原始详情：{}" $(, $($args)*)?) };
    (CatalogApplied $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Catalog overlay applied:", "已应用价格目录覆盖：" $(, $($args)*)?) };
    (CatalogTitle $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Pricing catalog:", "价格目录：" $(, $($args)*)?) };
    (CatalogReset $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Pricing catalog reset:", "已重置价格目录：" $(, $($args)*)?) };
    (CatalogBase $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- base: {} ({}, {} models)", "- 基础：{}（{}，{} 个模型）" $(, $($args)*)?) };
    (CatalogOverlay $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- overlay: {} ({} models)", "- 覆盖：{}（{} 个模型）" $(, $($args)*)?) };
    (CatalogEffective $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- effective: {} ({} models, {} source rules)", "- 生效：{}（{} 个模型，{} 条来源规则）" $(, $($args)*)?) };
    (CatalogRecomputed $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- recomputed events: {}", "- 已重算事件：{}" $(, $($args)*)?) };
    (CatalogNoOverlay $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- overlay: none", "- 覆盖：无" $(, $($args)*)?) };
    (CatalogRebase $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- rebase available: {}", "- 可重新基于基础目录生成：{}" $(, $($args)*)?) };
    (CatalogEffectiveVersion $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- effective: {} ({}, {} models)", "- 生效：{}（{}，{} 个模型）" $(, $($args)*)?) };
    (CatalogRemoved $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- overlay removed: {}", "- 已移除覆盖：{}" $(, $($args)*)?) };
    (CatalogLayer $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- {}: {} ({}, schema {}, {} models, {} source rules)", "- {}：{}（{}，结构版本 {}，{} 个模型，{} 条来源规则）" $(, $($args)*)?) };
    (CatalogFile $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "  file: {}", "  文件：{}" $(, $($args)*)?) };
    (LogsTitle $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Logs:", "日志：" $(, $($args)*)?) };
    (LogFile $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- File: {}", "- 文件：{}" $(, $($args)*)?) };
    (LogExists $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Exists: {}", "- 存在：{}" $(, $($args)*)?) };
    (LogSize $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Size: {} bytes", "- 大小：{} bytes" $(, $($args)*)?) };
    (LogRetained $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Retained: {} files, {} bytes total", "- 保留：{} 个文件，共 {} bytes" $(, $($args)*)?) };
    (LogErrors $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Recent errors: {}", "- 最近错误：{}" $(, $($args)*)?) };
    (LogDropped $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Dropped events: {}", "- 丢弃事件：{}" $(, $($args)*)?) };
    (LogMaintenance $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Maintenance errors: {}", "- 维护错误：{}" $(, $($args)*)?) };
    (LogEntries $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Entries:", "条目：" $(, $($args)*)?) };
    (LogNone $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- none", "- 无" $(, $($args)*)?) };
    (RunLog $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Run log:", "运行日志：" $(, $($args)*)?) };
    (OriginalRun $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Original run detail", "原始运行详情" $(, $($args)*)?) };
    (DiagnosticsWrote $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Wrote diagnostics to {}", "已写出诊断到 {}" $(, $($args)*)?) };
    (ForgotFile $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Marked {} as deleted_by_user for source {}", "已将 {} 标记为 {} 源的 deleted_by_user" $(, $($args)*)?) };
    (RemoteRegistered $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Registered host {} (host_id={}, target={})", "已注册主机 {}（host_id={}，目标={}）" $(, $($args)*)?) };
    (RemoteList $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "{}\thost_id={}\ttransport={}\ttarget={}\tcommand={}\tlast_contacted={}\twatermark={}\terror={}", "{}\thost_id={}\t传输={}\t目标={}\t命令={}\t最近联系={}\t水位={}\t原始错误={}" $(, $($args)*)?) };
    (RemoteRemovedUsage $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Removed host '{}' and deleted its imported usage rows.", "已移除主机 '{}' 并删除其导入的用量记录。" $(, $($args)*)?) };
    (RemoteRemoved $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Removed host '{}'. Imported usage rows were kept.\nTo delete them, run: llmusage remote remove {} --delete-usage --yes", "已移除主机 '{}'，已保留导入的用量记录。\n要删除这些记录，请运行：llmusage remote remove {} --delete-usage --yes" $(, $($args)*)?) };
    (RemoteNone $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "No ssh hosts registered.", "尚未注册 SSH 主机。" $(, $($args)*)?) };
    (RemoteImported $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Imported host {} (shards={}, skipped_lines={})", "已导入主机 {}（分片={}，跳过行={}）" $(, $($args)*)?) };
    (CleanEmpty $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Nothing to clean\n", "没有可清理内容\n" $(, $($args)*)?) };
    (CleanPreview $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Disk unchanged. To delete Deletable items, run llmusage clean --yes.\n", "未修改磁盘。要删除「可删除」项，请运行 llmusage clean --yes。\n" $(, $($args)*)?) };
    (CleanDeleted $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Deleted {} files, freed {}.\n", "已删除 {} 个文件，释放 {}。\n" $(, $($args)*)?) };
    (CleanDirs $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Removed {} empty directories.\n", "已移除 {} 个空目录。\n" $(, $($args)*)?) };
    (CleanFailures $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Deletion failed:\n", "删除失败：\n" $(, $($args)*)?) };
    (CleanTotal $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Total {}\n", "合计 {}\n" $(, $($args)*)?) };
    (CleanDeletable $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Deletable {}\n", "其中可删除 {}\n" $(, $($args)*)?) };
    (CleanBytes $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "{} bytes / {:.2} GB", "{} 字节 / {:.2} GB" $(, $($args)*)?) };
    (UpdateCheck $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Check only: no update was performed.", "仅检查：未执行更新。" $(, $($args)*)?) };
    (UpdateCancelled $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Update cancelled.", "更新已取消。" $(, $($args)*)?) };
    (UpdateStarting $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Starting update...", "正在开始更新..." $(, $($args)*)?) };
    (UpdateCompleted $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Update completed successfully.", "更新成功完成。" $(, $($args)*)?) };
    (UpdateVerify $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Run `llmusage --version` to verify the installed version.", "运行 `llmusage --version` 验证已安装版本。" $(, $($args)*)?) };
    (UpdateVersion $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Current version: {}", "当前版本：{}" $(, $($args)*)?) };
    (UpdateRepo $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Repository: {}", "仓库：{}" $(, $($args)*)?) };
    (UpdateChannel $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Channel: {}", "渠道：{}" $(, $($args)*)?) };
    (UpdateTag $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Release tag: {}", "发布标签：{}" $(, $($args)*)?) };
    (UpdateCommit $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Resolved commit: {}", "已解析提交：{}" $(, $($args)*)?) };
    (UpdateDevWarning $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "WARNING: dev follows the mutable 'dev' branch and is not a verified stable release.", "警告：dev 跟随可变的 'dev' 分支，不是经过验证的稳定版。" $(, $($args)*)?) };
    (UpdateDevReview $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "The resolved commit is shown for review, but the branch can change in the future.", "已显示解析的提交供核对；分支今后仍可能变化。" $(, $($args)*)?) };
    (UpdateCommand $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Command: {}", "命令：{}" $(, $($args)*)?) };
    (UpdateConfirm $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Continue with the update? [Y/n]: ", "是否继续更新？[Y/n]：" $(, $($args)*)?) };
    (UpdateRetry $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Please answer `y` or `n`.", "请输入 `y` 或 `n`。" $(, $($args)*)?) };
    (LocalDashboard $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Local dashboard: {}", "本地看板：{}" $(, $($args)*)?) };
    (RemoteDashboard $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Remote dashboard: {}", "远程看板：{}" $(, $($args)*)?) };
    (PublicWarning $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Warning: `--public` exposes the dashboard and JSON API without authentication or TLS. Restrict access with a firewall, SSH tunnel, or reverse proxy.", "警告：`--public` 会公开无身份验证、无 TLS 的看板和 JSON API。请通过防火墙、SSH 隧道或反向代理限制访问。" $(, $($args)*)?) };
    (SshForward $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "SSH session detected; browser opening skipped. To open locally, forward the port: ssh -L {0}:127.0.0.1:{0} <user>@<server>", "检测到 SSH 会话；已跳过打开浏览器。要在本地访问，请转发端口：ssh -L {0}:127.0.0.1:{0} <user>@<server>" $(, $($args)*)?) };
    (SshSkip $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "SSH session detected; browser opening skipped.", "检测到 SSH 会话；已跳过打开浏览器。" $(, $($args)*)?) };
    (LegacySkipped $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "legacy token accounting; existing history was kept and this source was skipped for this round. Run `llmusage sync --rebuild --source {0}` to repair. If source files are missing, restore them or add `--allow-lossy-rebuild` to that rebuild command to explicitly accept clearing unrebuildable history.", "旧版 token 计数；已保留现有历史并在本轮跳过该来源。运行 `llmusage sync --rebuild --source {0}` 修复。如果来源文件缺失，请先恢复文件，或在重建命令中加入 `--allow-lossy-rebuild` 明确接受清除无法重建的历史。" $(, $($args)*)?) };
    (TuiDeprecated $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "{} `tui` is deprecated, use `{}` instead", "{} `tui` 已弃用，请改用 `{}`" $(, $($args)*)?) };
    (TracerParsed $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Parsed {} files, found {} events", "已解析 {} 个文件，找到 {} 条事件" $(, $($args)*)?) };
    (TracerErrors $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Warning: {} files failed to parse", "警告：{} 个文件解析失败" $(, $($args)*)?) };
    (TracerCount $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Database contains {} events", "数据库包含 {} 条事件" $(, $($args)*)?) };
    (TracerStarting $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Starting Codex Tracer dashboard on port {}...", "正在端口 {} 启动 Codex Tracer 看板..." $(, $($args)*)?) };
    (TracerDashboard $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "🚀 Codex Tracer Dashboard: {}", "🚀 Codex Tracer 看板：{}" $(, $($args)*)?) };
    (TracerBrowser $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Failed to open browser automatically. Please open manually: {}", "无法自动打开浏览器，请手动访问：{}" $(, $($args)*)?) };
    (DoctorTitle $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Doctor:", "健康检查：" $(, $($args)*)?) };
    (DoctorPricing $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Activated catalog `{}` ({}) and recomputed costs for {} events", "已激活 catalog `{}`（{}）并重算 {} 条事件的成本列" $(, $($args)*)?) };
    (DoctorDb $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "OpenCode DB: {}", "OpenCode 数据库：{}" $(, $($args)*)?) };
    (DoctorLogsDisabled $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "structured runtime logging disabled; path: {}", "结构化运行日志已禁用；路径：{}" $(, $($args)*)?) };
    (DoctorLogs $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "structured runtime log: {} ({} bytes)", "结构化运行日志：{}（{} bytes）" $(, $($args)*)?) };
    (DoctorErrors $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "{} ERROR entries in the recent local log scan", "最近本地日志扫描中有 {} 条 ERROR 记录" $(, $($args)*)?) };
    (DoctorDropped $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "{} events dropped by the non-blocking log queue", "非阻塞日志队列丢弃了 {} 条事件" $(, $($args)*)?) };
    (DoctorMaintenance $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "{} runtime log rotation or retention errors", "有 {} 个运行日志轮转或保留错误" $(, $($args)*)?) };
    (DoctorRunsFailed $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Recent runs contain non-success records such as failed/aborted", "最近运行中存在 failed/aborted 等非成功记录" $(, $($args)*)?) };
    (DoctorRunsOk $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "No non-success records such as failed/aborted in recent runs", "最近运行未发现 failed/aborted 等非成功记录" $(, $($args)*)?) };
    (DoctorNoSources $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "no source blockers in the latest source observations", "最新来源观察中没有来源阻塞问题" $(, $($args)*)?) };
    (DoctorNoParse $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "no malformed or oversized parse issues", "没有格式错误或超长解析故障" $(, $($args)*)?) };
    (DoctorParse $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "{} malformed={} oversized={}", "{} 格式错误={} 超长={}" $(, $($args)*)?) };
    (SourceStatusTitle $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Source status:", "来源状态：" $(, $($args)*)?) };
    (SourceHistorical $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "historical usage is retained; no passive reader is available", "已保留历史用量；尚无可用的被动读取器" $(, $($args)*)?) };
    (SourceNative $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "passive native SQLite usage reader; encrypted .pb artifacts and unverified legacy roots are unsupported", "被动读取原生 SQLite 用量；不支持加密 .pb 文件及未经验证的旧版根路径" $(, $($args)*)?) };
    (SourcePassive $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "passive local artifact reader", "被动读取本地文件" $(, $($args)*)?) };
    (PlatformDetected $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "candidate roots detected via {}", "已通过 {} 检测到候选根路径" $(, $($args)*)?) };
    (PlatformUndetected $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "no candidate roots detected; checked {}", "未检测到候选根路径；已检查 {}" $(, $($args)*)?) };
    (HostStatus $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Host {}: status={}", "主机 {}：状态={}" $(, $($args)*)?) };
    (SourceStatus $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Source status {}: status={} quality={} total={} last={} accounting={} ({})", "- 来源状态 {}：状态={} 质量={} 合计={} 最近={} 计数={}（{}）" $(, $($args)*)?) };
    (PlatformStatus $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "- Platform monitor {}: status={} parser={} quality={} roots={}/{} privacy={} ({}) next={}", "- 平台监测 {}：状态={} 解析器={} 质量={} 根路径={}/{} 隐私={}（{}）下一步={}" $(, $($args)*)?) };
    (RemoteAccounting $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Remote host {} source {} token accounting is not current; a full restore is required", "远程主机 {} 来源 {} 的 token 计数规则不是当前版本；需要完整恢复" $(, $($args)*)?) };
    (RetainedHooks $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "{} hook-era Antigravity event(s) are retained in historical totals under their original accounting; the parser accounting version does not certify these records", "历史合计中保留了 {} 条 hook 时代的 Antigravity 事件，沿用其原始计数规则；解析器计数版本不代表这些记录已获认证" $(, $($args)*)?) };
    (StatusWarning $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "  Warning: {}", "  警告：{}" $(, $($args)*)?) };
    (OriginalWarning $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "  Warning; original detail: {}", "  警告；原始详情：{}" $(, $($args)*)?) };
    (CleanupFinished $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Legacy cleanup finished:", "旧版集成清理完成：" $(, $($args)*)?) };
    (Purged $(, $($args:tt)*)?) => { $crate::common::i18n::commands::message!(@pair "Purged {}", "已清除 {}" $(, $($args)*)?) };
}
pub(crate) use message;

/// Private optional command facts; no localized text enters file log metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "surface",
    content = "facts",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum DiagnosticFacts {
    Operations(OperationDiagnostic),
    Reports(reports::DiagnosticFacts),
    Domain(DomainDiagnostic),
    Pricing(pricing::DiagnosticFacts),
    RemoteImport(remote_import::DiagnosticFacts),
}
impl DiagnosticFacts {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Operations(facts) => facts.code(),
            Self::Reports(facts) => facts.code(),
            Self::Domain(facts) => facts.code(),
            Self::Pricing(facts) => facts.code(),
            Self::RemoteImport(facts) => facts.code(),
        }
    }
    pub(crate) fn render(&self, language: super::Language) -> Option<String> {
        match self {
            Self::Operations(facts) => Some(facts.render(language)),
            Self::Reports(facts) => facts.render(language),
            Self::Domain(facts) => Some(facts.render(language)),
            Self::Pricing(facts) => facts.render(language),
            Self::RemoteImport(facts) => Some(facts.render(language)),
        }
    }
}
impl From<OperationDiagnostic> for DiagnosticFacts {
    fn from(facts: OperationDiagnostic) -> Self {
        Self::Operations(facts)
    }
}

impl From<remote_import::DiagnosticFacts> for DiagnosticFacts {
    fn from(facts: remote_import::DiagnosticFacts) -> Self {
        Self::RemoteImport(facts)
    }
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum OperationDiagnostic {
    Registration {
        fact: crate::remote::register::RegistrationFact,
    },
    CleanupFailed {
        failures: Vec<crate::integrations::CleanupFailureFact>,
    },
    CleanRead {
        path: String,
    },
    CleanDelete {
        count: usize,
    },
    ForgetMissing {
        path: String,
    },
    ForgetUnknown {
        source: String,
    },
    ForgetAmbiguous {
        path: String,
        sources: String,
    },
    UpdateRefs {
        channel: String,
        detail: String,
    },
    UpdateEncoding,
    UpdateChanged {
        channel: String,
    },
    UpdateStart {
        channel: String,
        detail: String,
        command: String,
    },
    UpdateAmbiguous {
        version: String,
    },
    UpdateInconsistentTag {
        tag: String,
    },
    UpdateNoStable,
    UpdateNoDirect {
        tag: String,
    },
    UpdateInvalidTag {
        tag: String,
    },
    UpdateInvalidDev,
    UpdateInconsistentDev,
    UpdateNoDev,
    UpdateMissingSha,
    UpdateMissingRef,
    UpdateExtraFields,
    UpdateInvalidSha,
    UpdateEof,
    ServeSignal,
    ServeStopped {
        address: String,
    },
    BrowserExit {
        status: String,
    },
    TracerRemove,
    TracerOpen,
    TracerMissing {
        path: String,
    },
    TracerNoEvents {
        path: String,
    },
    TracerBind {
        address: String,
    },
    TracerServer,
    UpdateFailed {
        channel: String,
        exit_code: Option<i32>,
        command: String,
    },
}
impl OperationDiagnostic {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Registration { .. } => "cli.remote_registration",
            Self::CleanupFailed { .. } => "cli.cleanup_failed",
            Self::CleanRead { .. } => "cli.clean_read",
            Self::CleanDelete { .. } => "cli.clean_delete",
            Self::ForgetMissing { .. } => "cli.forget_missing",
            Self::ForgetUnknown { .. } => "cli.forget_unknown",
            Self::ForgetAmbiguous { .. } => "cli.forget_ambiguous",
            Self::UpdateRefs { .. } => "cli.update_refs",
            Self::UpdateEncoding => "cli.update_encoding",
            Self::UpdateChanged { .. } => "cli.update_changed",
            Self::UpdateStart { .. } => "cli.update_start",
            Self::UpdateAmbiguous { .. } => "cli.update_ambiguous",
            Self::UpdateInconsistentTag { .. } => "cli.update_inconsistent_tag",
            Self::UpdateNoStable => "cli.update_no_stable",
            Self::UpdateNoDirect { .. } => "cli.update_no_direct",
            Self::UpdateInvalidTag { .. } => "cli.update_invalid_tag",
            Self::UpdateInvalidDev => "cli.update_invalid_dev",
            Self::UpdateInconsistentDev => "cli.update_inconsistent_dev",
            Self::UpdateNoDev => "cli.update_no_dev",
            Self::UpdateMissingSha => "cli.update_missing_sha",
            Self::UpdateMissingRef => "cli.update_missing_ref",
            Self::UpdateExtraFields => "cli.update_extra_fields",
            Self::UpdateInvalidSha => "cli.update_invalid_sha",
            Self::UpdateEof => "cli.update_eof",
            Self::ServeSignal => "cli.serve_signal",
            Self::ServeStopped { .. } => "cli.serve_stopped",
            Self::BrowserExit { .. } => "cli.browser_exit",
            Self::TracerRemove => "cli.tracer_remove",
            Self::TracerOpen => "cli.tracer_open",
            Self::TracerMissing { .. } => "cli.tracer_missing",
            Self::TracerNoEvents { .. } => "cli.tracer_no_events",
            Self::TracerBind { .. } => "cli.tracer_bind",
            Self::TracerServer => "cli.tracer_server",
            Self::UpdateFailed { .. } => "cli.update_failed",
        }
    }
    pub(crate) fn render(&self, language: super::Language) -> String {
        match self {
            Self::Registration { fact } => registration_detail(fact, language),
            Self::CleanupFailed { failures } => {
                let details = failures
                    .iter()
                    .map(|failure| {
                        let owner = failure
                            .source
                            .map(|source| source.to_string())
                            .unwrap_or_else(|| {
                                language
                                    .select("legacy hook wrappers", "旧版 hook 包装脚本")
                                    .into()
                            });
                        let mut detail = format!(
                            "{owner}: {}",
                            cleanup_failure_detail(&failure.detail, language)
                        );
                        if let Some(error) = &failure.audit_error {
                            detail.push_str(&format!(
                                "; {}: {error}",
                                language.select(
                                    "cleanup failure audit also failed; original detail",
                                    "清理失败审计也失败；原始详情"
                                )
                            ));
                        }
                        detail
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                format!(
                    "{}: {details}",
                    language.select("legacy hook cleanup failed", "旧版 hook 清理失败")
                )
            }
            Self::CleanRead { path } => match language {
                super::Language::English => format!("Failed to read {path}"),
                super::Language::Chinese => format!("读取 {path} 失败"),
            },
            Self::CleanDelete { count } => match language {
                super::Language::English => format!("Failed to delete {count} paths"),
                super::Language::Chinese => format!("{count} 个路径删除失败"),
            },
            Self::ForgetMissing { path } => match language {
                super::Language::English => format!(
                    "No source found for file_path={path}; specify --source codex|claude|opencode|antigravity|kimi_code|pi|omp|grok"
                ),
                super::Language::Chinese => format!(
                    "找不到 file_path={path} 对应的源；请显式传 --source codex|claude|opencode|antigravity|kimi_code|pi|omp|grok"
                ),
            },
            Self::ForgetUnknown { source } => match language {
                super::Language::English => {
                    format!("Unknown source value in source_file: {source}")
                }
                super::Language::Chinese => {
                    format!("source_file 表里 source 列出现未知值：{source}")
                }
            },
            Self::ForgetAmbiguous { path, sources } => match language {
                super::Language::English => {
                    format!("{path} exists in multiple sources ({sources}); specify --source")
                }
                super::Language::Chinese => {
                    format!("{path} 在多个源（{sources}）中存在，请用 --source 指定")
                }
            },
            Self::UpdateRefs { channel, detail } => match language {
                super::Language::English => {
                    format!("failed to resolve update refs for `{channel}`{detail}")
                }
                super::Language::Chinese => {
                    format!("无法解析 `{channel}` 渠道的更新引用；原始详情{detail}")
                }
            },
            Self::UpdateEncoding => match language {
                super::Language::English => "git ls-remote returned non-UTF-8 output".to_string(),
                super::Language::Chinese => "git ls-remote 返回的输出不是 UTF-8".to_string(),
            },
            Self::UpdateChanged { channel } => match language {
                super::Language::English => format!(
                    "resolved update target changed after confirmation; rerun `llmusage update {channel}` to review the new target"
                ),
                super::Language::Chinese => format!(
                    "确认后更新目标发生变化；请重新运行 `llmusage update {channel}` 核对新目标"
                ),
            },
            Self::UpdateStart {
                channel,
                detail,
                command,
            } => match language {
                super::Language::English => format!(
                    "failed to start Cargo update from `{channel}`: {detail}\nManual command: {command}"
                ),
                super::Language::Chinese => format!(
                    "无法从 `{channel}` 启动 Cargo 更新；原始详情：{detail}\n手动命令：{command}"
                ),
            },
            Self::UpdateAmbiguous { version } => match language {
                super::Language::English => format!("ambiguous stable release tags for {version}"),
                super::Language::Chinese => format!("稳定发布标签存在歧义：{version}"),
            },
            Self::UpdateInconsistentTag { tag } => match language {
                super::Language::English => format!("release tag `{tag}` resolved inconsistently"),
                super::Language::Chinese => format!("发布标签 `{tag}` 解析结果不一致"),
            },
            Self::UpdateNoStable => match language {
                super::Language::English => {
                    "no stable semantic-version release tag was found".to_string()
                }
                super::Language::Chinese => "未找到稳定语义版本的发布标签".to_string(),
            },
            Self::UpdateNoDirect { tag } => match language {
                super::Language::English => {
                    format!("release tag `{tag}` has a peeled commit without a tag ref")
                }
                super::Language::Chinese => format!("发布标签 `{tag}` 存在剥离提交但缺少标签引用"),
            },
            Self::UpdateInvalidTag { tag } => match language {
                super::Language::English => {
                    format!("release tag `{tag}` resolved to an invalid commit SHA")
                }
                super::Language::Chinese => format!("发布标签 `{tag}` 解析出的提交 SHA 无效"),
            },
            Self::UpdateInvalidDev => match language {
                super::Language::English => {
                    "dev branch resolved to an invalid commit SHA".to_string()
                }
                super::Language::Chinese => "dev 分支解析出的提交 SHA 无效".to_string(),
            },
            Self::UpdateInconsistentDev => match language {
                super::Language::English => "dev branch resolved inconsistently".to_string(),
                super::Language::Chinese => "dev 分支解析结果不一致".to_string(),
            },
            Self::UpdateNoDev => match language {
                super::Language::English => "official dev branch was not found".to_string(),
                super::Language::Chinese => "未找到官方 dev 分支".to_string(),
            },
            Self::UpdateMissingSha => match language {
                super::Language::English => "missing SHA in git ref output".to_string(),
                super::Language::Chinese => "git 引用输出缺少 SHA".to_string(),
            },
            Self::UpdateMissingRef => match language {
                super::Language::English => "missing ref name in git ref output".to_string(),
                super::Language::Chinese => "git 引用输出缺少引用名称".to_string(),
            },
            Self::UpdateExtraFields => match language {
                super::Language::English => "unexpected fields in git ref output".to_string(),
                super::Language::Chinese => "git 引用输出包含多余字段".to_string(),
            },
            Self::UpdateInvalidSha => match language {
                super::Language::English => "invalid SHA in git ref output".to_string(),
                super::Language::Chinese => "git 引用输出中的 SHA 无效".to_string(),
            },
            Self::UpdateEof => match language {
                super::Language::English => {
                    "confirmation input reached EOF; update was not started".to_string()
                }
                super::Language::Chinese => "确认输入已到 EOF；未开始更新".to_string(),
            },
            Self::ServeSignal => match language {
                super::Language::English => "Failed to wait for Ctrl+C".to_string(),
                super::Language::Chinese => "等待 Ctrl+C 失败".to_string(),
            },
            Self::ServeStopped { address } => match language {
                super::Language::English => {
                    format!("Web server at {address} stopped before a shutdown signal")
                }
                super::Language::Chinese => {
                    format!("位于 {address} 的 Web 服务在收到关闭信号前停止")
                }
            },
            Self::BrowserExit { status } => match language {
                super::Language::English => format!("browser launcher exited with status {status}"),
                super::Language::Chinese => format!("浏览器启动程序已退出，状态为 {status}"),
            },
            Self::TracerRemove => match language {
                super::Language::English => "Failed to remove existing database".to_string(),
                super::Language::Chinese => "无法移除现有数据库".to_string(),
            },
            Self::TracerOpen => match language {
                super::Language::English => "Failed to open codex-tracer database".to_string(),
                super::Language::Chinese => "无法打开 codex-tracer 数据库".to_string(),
            },
            Self::TracerMissing { path } => match language {
                super::Language::English => format!(
                    "Codex rollout directory not found: {path}\nPlease ensure Codex is installed and has been used at least once.\nYou can set CODEX_HOME to specify a custom location."
                ),
                super::Language::Chinese => format!(
                    "未找到 Codex rollout 目录：{path}\n请确认已安装并至少使用过一次 Codex。\n可以通过 CODEX_HOME 指定自定义位置。"
                ),
            },
            Self::TracerNoEvents { path } => match language {
                super::Language::English => format!(
                    "No events found in {path}\nPlease ensure you have used Codex at least once."
                ),
                super::Language::Chinese => {
                    format!("在 {path} 中未找到事件。\n请确认至少使用过一次 Codex。")
                }
            },
            Self::TracerBind { address } => match language {
                super::Language::English => format!("Failed to bind to {address}"),
                super::Language::Chinese => format!("无法绑定到 {address}"),
            },
            Self::TracerServer => match language {
                super::Language::English => "Server error".to_string(),
                super::Language::Chinese => "服务错误".to_string(),
            },
            Self::UpdateFailed {
                channel,
                exit_code,
                command,
            } => {
                let detail = exit_code.map_or_else(
                    || {
                        language
                            .select("terminated without an exit code", "终止且没有退出码")
                            .to_owned()
                    },
                    |code| match language {
                        super::Language::English => format!("exit code {code}"),
                        super::Language::Chinese => format!("退出码 {code}"),
                    },
                );
                match language {
                    super::Language::English => format!(
                        "Cargo update from `{channel}` failed with {detail}.\nManual command: {command}"
                    ),
                    super::Language::Chinese => format!(
                        "从 `{channel}` 执行 Cargo 更新失败（{detail}）。\n手动命令：{command}"
                    ),
                }
            }
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DomainDiagnostic {
    NotInitialized,
    Io {
        detail: String,
    },
    Db {
        detail: String,
    },
    Migration {
        version: u32,
        name: String,
        detail: String,
    },
    Parse {
        context: String,
        detail: String,
    },
    SchemaTooNew {
        db_version: u32,
        binary_version: u32,
    },
    SchemaCorrupt {
        raw: String,
    },
    LockBusy {
        holder: String,
    },
    LockLost,
    Config {
        detail: String,
    },
    Cancelled {
        operation: String,
    },
    PricingMissing {
        source_id: String,
        model: String,
    },
}
impl DomainDiagnostic {
    pub(crate) fn from_error(error: &crate::LlmusageError) -> Self {
        use crate::LlmusageError::*;
        match error {
            NotInitialized => Self::NotInitialized,
            Io(error) => Self::Io {
                detail: error.to_string(),
            },
            Db(error) => Self::Db {
                detail: error.to_string(),
            },
            MigrationFailed {
                version,
                name,
                source,
            } => Self::Migration {
                version: *version,
                name: (*name).into(),
                detail: format!("{source:#}"),
            },
            Parse { context, source } => Self::Parse {
                context: (*context).into(),
                detail: source.to_string(),
            },
            SchemaTooNew {
                db_version,
                binary_version,
            } => Self::SchemaTooNew {
                db_version: *db_version,
                binary_version: *binary_version,
            },
            SchemaVersionCorrupt { raw } => Self::SchemaCorrupt { raw: raw.clone() },
            LockBusy { holder } => Self::LockBusy {
                holder: holder.clone(),
            },
            LockLost => Self::LockLost,
            ConfigInvalid { detail } => Self::Config {
                detail: detail.clone(),
            },
            Cancelled { operation } => Self::Cancelled {
                operation: (*operation).into(),
            },
            PricingMissing { source_id, model } => Self::PricingMissing {
                source_id: source_id.clone(),
                model: model.clone(),
            },
        }
    }
    fn code(&self) -> &'static str {
        match self {
            Self::NotInitialized => "domain.not_initialized",
            Self::Io { .. } => "domain.io",
            Self::Db { .. } => "domain.db",
            Self::Migration { .. } => "domain.migration",
            Self::Parse { .. } => "domain.parse",
            Self::SchemaTooNew { .. } => "domain.schema_too_new",
            Self::SchemaCorrupt { .. } => "domain.schema_corrupt",
            Self::LockBusy { .. } => "domain.lock_busy",
            Self::LockLost => "domain.lock_lost",
            Self::Config { .. } => "domain.config",
            Self::Cancelled { .. } => "domain.cancelled",
            Self::PricingMissing { .. } => "domain.pricing_missing",
        }
    }
    fn render(&self, language: super::Language) -> String {
        let text = |en, zh| language.select(en, zh);
        let original = text("Original detail", "原始详情");
        match self {
            Self::NotInitialized => text(
                "Not initialized; run `llmusage init`",
                "尚未初始化；请运行 `llmusage init`",
            )
            .into(),
            Self::Io { detail } => format!(
                "{}; {original}: {detail}",
                text("Filesystem operation failed", "文件系统操作失败")
            ),
            Self::Db { detail } => format!(
                "{}; {original}: {detail}",
                text("Database operation failed", "数据库操作失败")
            ),
            Self::Migration {
                version,
                name,
                detail,
            } => format!(
                "{} {version} ({name}); {original}: {detail}",
                text("Migration failed:", "迁移失败：")
            ),
            Self::Parse { context, detail } => format!(
                "{} {context}; {original}: {detail}",
                text("Could not parse", "无法解析")
            ),
            Self::SchemaTooNew {
                db_version,
                binary_version,
            } => format!(
                "{} db={db_version}, binary={binary_version}",
                text(
                    "Database schema is too new; upgrade llmusage.",
                    "数据库版本过新；请升级 llmusage。"
                )
            ),
            Self::SchemaCorrupt { raw } => format!(
                "{} {raw:?}",
                text(
                    "Database schema_version is corrupt:",
                    "数据库 schema_version 损坏："
                )
            ),
            Self::LockBusy { holder } => {
                format!("{} {holder}", text("Worker lock busy:", "工作锁被占用："))
            }
            Self::LockLost => {
                text("Worker lock lost; stop writing", "工作锁已失效；停止写入").into()
            }
            Self::Config { detail } => format!(
                "{}: {detail}",
                text(
                    "Invalid configuration; original detail",
                    "配置无效；原始详情"
                )
            ),
            Self::Cancelled { operation } => {
                format!("{} {operation}", text("Cancelled:", "已取消："))
            }
            Self::PricingMissing { source_id, model } => format!(
                "{} {source_id}:{model}",
                text("Pricing missing:", "缺少价格：")
            ),
        }
    }
}

pub(crate) fn status_label(code: &str) -> &str {
    if super::current() == super::Language::English {
        return code;
    }
    match code {
        "passive_ready" => "被动读取已就绪（passive_ready）",
        "passive_no_data" => "暂无被动数据（passive_no_data）",
        "historical_only" => "仅历史数据（historical_only）",
        "current" => "当前（current）",
        "legacy" => "旧版（legacy）",
        "unversioned" => "未标记版本（unversioned）",
        "unknown" => "未知（unknown）",
        "precise" => "精确（precise）",
        "total_only" => "仅合计（total_only）",
        "estimated" => "估算（estimated）",
        "detected" => "已检测到（detected）",
        "unavailable" => "不可用（unavailable）",
        "registered" => "已注册（registered）",
        "planned" => "计划中（planned）",
        "blocked_no_samples" => "缺少样本（blocked_no_samples）",
        "blocked_no_usage" => "缺少用量信息（blocked_no_usage）",
        "unsupported" => "不支持（unsupported）",
        "external_only" => "仅外部（external_only）",
        "local_artifacts" => "本地文件（local_artifacts）",
        "local_database" => "本地数据库（local_database）",
        "sensitive_local" => "本地敏感数据（sensitive_local）",
        "idle" => "空闲（idle）",
        "unreachable" => "不可达（unreachable）",
        "never_contacted" => "尚未联系（never_contacted）",
        "ok" => "正常（ok）",
        "warn" => "警告（warn）",
        "restored" => "已恢复（restored）",
        "skipped" => "已跳过（skipped）",
        _ => code,
    }
}

pub(crate) fn cleanup_detail(fact: &crate::integrations::CleanupFact) -> &'static str {
    use crate::integrations::CleanupFact::*;
    let (en, zh) = match fact {
        CodexNotify => (
            "legacy Codex notify cleanup completed",
            "旧版 Codex notify 清理完成",
        ),
        CodexResidue => (
            "recovered legacy Codex atomic-write residue",
            "已恢复旧版 Codex 原子写入残留",
        ),
        CodexAbsent => ("no legacy Codex notify found", "未发现旧版 Codex notify"),
        ClaudeHooks => (
            "removed legacy llmusage Claude hooks",
            "已移除旧版 llmusage Claude hooks",
        ),
        ClaudeResidue => (
            "recovered legacy Claude atomic-write residue",
            "已恢复旧版 Claude 原子写入残留",
        ),
        ClaudeAbsent => ("no legacy Claude hooks found", "未发现旧版 Claude hooks"),
        OpenCodeAbsent { residue: true } => (
            "recovered legacy OpenCode atomic-write residue",
            "已恢复旧版 OpenCode 原子写入残留",
        ),
        OpenCodeAbsent { residue: false } => (
            "no legacy OpenCode plugin found",
            "未发现旧版 OpenCode 插件",
        ),
        OpenCodeUnowned { residue: true } => (
            "recovered residue; preserved unowned OpenCode plugin",
            "已恢复残留；已保留不属于 llmusage 的 OpenCode 插件",
        ),
        OpenCodeUnowned { residue: false } => (
            "preserved unowned OpenCode plugin",
            "已保留不属于 llmusage 的 OpenCode 插件",
        ),
        OpenCodePlugin => (
            "removed legacy llmusage OpenCode plugin",
            "已移除旧版 llmusage OpenCode 插件",
        ),
        Antigravity { hooks: true, .. } => (
            "removed legacy llmusage Antigravity/Gemini hooks",
            "已移除旧版 llmusage Antigravity/Gemini hooks",
        ),
        Antigravity {
            hooks: false,
            residue: true,
        } => (
            "recovered legacy Antigravity/Gemini atomic-write residue",
            "已恢复旧版 Antigravity/Gemini 原子写入残留",
        ),
        Antigravity {
            hooks: false,
            residue: false,
        } => (
            "no legacy Antigravity/Gemini hooks found",
            "未发现旧版 Antigravity/Gemini hooks",
        ),
    };
    super::text(en, zh)
}

fn cleanup_failure_detail(
    fact: &crate::integrations::CleanupFailureDetail,
    language: super::Language,
) -> String {
    use crate::integrations::CleanupFailureDetail::*;
    match fact {
        Original { detail } => format!(
            "{}: {detail}",
            language.select("Original detail", "原始详情")
        ),
        MissingCodexConfig { backup, config } => match language {
            super::Language::English => {
                format!("cannot restore {backup} because {config} is missing")
            }
            super::Language::Chinese => format!("无法恢复 {backup}，因为 {config} 缺失"),
        },
        ConfigShape {
            label,
            field,
            array,
        } => {
            let field = if field.is_empty() {
                language.select("top level", "顶层").to_string()
            } else {
                format!("{field} {}", language.select("field", "字段"))
            };
            let required = if *array {
                language.select("an array", "数组")
            } else {
                language.select("an object", "对象")
            };
            match language {
                super::Language::English => format!("{label} {field} must be {required}"),
                super::Language::Chinese => format!("{label} 的 {field} 必须为{required}"),
            }
        }
        Wrappers { removed, failures } => format!(
            "{}; {}: {}",
            if removed.is_empty() {
                language
                    .select(
                        "no legacy hook wrappers removed",
                        "未移除旧版 hook 包装脚本",
                    )
                    .to_string()
            } else {
                format!(
                    "{}: {}",
                    language.select("removed legacy hook wrapper(s)", "已移除旧版 hook 包装脚本"),
                    removed.join(", ")
                )
            },
            language.select("failures; original detail", "失败；原始详情"),
            failures.join("; ")
        ),
    }
}

fn registration_detail(
    fact: &crate::remote::register::RegistrationFact,
    language: super::Language,
) -> String {
    use crate::remote::{SHARD_PROTOCOL_VERSION, register::RegistrationFact::*};
    match fact {
        EmptyLabel {} => language
            .select(
                "host label normalizes to an empty host_id",
                "主机标签规范化后的 host_id 为空",
            )
            .into(),
        SourceCollision { host, source } => match language {
            super::Language::English => format!(
                "host_id '{host}' collides with source name '{source}'; choose a different label"
            ),
            super::Language::Chinese => {
                format!("host_id '{host}' 与来源名称 '{source}' 冲突；请使用其他标签")
            }
        },
        Duplicate { host } => match language {
            super::Language::English => {
                format!("host_id '{host}' is already registered; choose a different label")
            }
            super::Language::Chinese => format!("host_id '{host}' 已注册；请使用其他标签"),
        },
        SshTarget { target } => match language {
            super::Language::English => format!(
                "ssh_target {target:?} must not start with '-'; OpenSSH would treat it as an option"
            ),
            super::Language::Chinese => {
                format!("ssh_target {target:?} 不能以 '-' 开头；OpenSSH 会将其当作选项")
            }
        },
        EmptyCommand {} => language
            .select("remote command is empty", "远程命令为空")
            .into(),
        Version { status, detail } => match language {
            super::Language::English => format!(
                "remote llmusage was not found or --version failed (exit {status}); Original detail: {detail}"
            ),
            super::Language::Chinese => format!(
                "未找到远程 llmusage 或 --version 失败（退出码 {status}）；原始详情：{detail}"
            ),
        },
        Handshake { status, detail } => match language {
            super::Language::English => {
                format!("remote handshake failed (exit {status}); Original detail: {detail}")
            }
            super::Language::Chinese => {
                format!("远程握手失败（退出码 {status}）；原始详情：{detail}")
            }
        },
        Protocol { remote, schema } => match language {
            super::Language::English => format!(
                "shard protocol mismatch: local={SHARD_PROTOCOL_VERSION} remote={remote} (remote schema_version={schema}); upgrade llmusage so both sides share shard_protocol {SHARD_PROTOCOL_VERSION}"
            ),
            super::Language::Chinese => format!(
                "分片协议不一致：本地={SHARD_PROTOCOL_VERSION} 远程={remote}（远程 schema_version={schema}）；请升级 llmusage，使两端使用相同的 shard_protocol {SHARD_PROTOCOL_VERSION}"
            ),
        },
    }
}
pub(crate) fn platform_action(id: &str, canonical: &str) -> String {
    if super::current() == super::Language::English {
        return canonical.into();
    }
    match id {
        "antigravity" => "被动读取 CLI conversations/*.db 用量元数据；不支持加密 .pb 文件".into(),
        "antigravity_ide" => {
            "被动读取 IDE conversations/*.db 用量元数据；不支持加密 .pb 文件或未经验证的旧版根路径"
                .into()
        }
        "zcode" => "由已注册的 ZCode 来源解析器读取 model_usage 表".into(),
        "deepseek_harness" => {
            "由已注册的 DeepSeek Harness 来源解析器读取 session.jsonl.zstd".into()
        }
        "codex" | "claude" | "opencode" | "kimi_code" | "pi" | "omp" | "grok" => {
            format!("由已注册的 {id} 来源解析器读取")
        }
        "reasonix" => "仅监测；当前会话缺少可重放的逐轮用量".into(),
        "gemini" => "仅监测；需要脱敏的 Gemini CLI 样本及 token 语义".into(),
        "cursor" => "仅监测；需要稳定的本地用量导出样本".into(),
        "copilot" => "仅监测；需要隐私审查和 token 语义".into(),
        "zed" => "仅监测；需要 Zed 测试样本覆盖".into(),
        "kiro" | "goose" | "codebuff" | "warp_oz" | "amp" | "gajae_code" => {
            format!("仅监测；需要 {id} 会话样本")
        }
        "kimi" => "仅监测；需要 Kimi/Qwen token 语义".into(),
        "roo_kilo_cline" => "仅监测；需要按扩展审查测试样本".into(),
        "crush" | "hermes" => format!("仅监测；需要审查 {id} 数据库结构"),
        "trae" => "仅监测；需要 Trae 身份验证、隐私审查及样本".into(),
        "openclaw_pi_droid" => "仅监测；需要各客户端的样本及 token 语义".into(),
        "synthetic" => "不是真实本地来源；不纳入 llmusage 导入".into(),
        _ => format!("原始详情：{canonical}"),
    }
}
