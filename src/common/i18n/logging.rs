// Stable, paired presentation catalog. Original tracing messages are never rewritten.
use super::Language;

#[derive(Debug)]
pub struct EventTemplate {
    pub code: &'static str,
    pub english: &'static str,
    pub chinese: &'static str,
}

pub const EVENTS: &[EventTemplate] = &[
    EventTemplate {
        code: "commands.blocks.01",
        english: "starting blocks report output",
        chinese: "开始输出用量窗口",
    },
    EventTemplate {
        code: "commands.blocks.02",
        english: "finished blocks report output",
        chinese: "完成输出用量窗口",
    },
    EventTemplate {
        code: "commands.codex_tracer.ingest.01",
        english: "Failed to ingest Codex tracer file",
        chinese: "导入 Codex 跟踪文件失败",
    },
    EventTemplate {
        code: "commands.codex_tracer.mod.01",
        english: "Starting codex-tracer",
        chinese: "正在启动 codex-tracer",
    },
    EventTemplate {
        code: "commands.codex_tracer.mod.02",
        english: "Rebuild requested, removing existing database",
        chinese: "已请求重建，正在移除现有数据库",
    },
    EventTemplate {
        code: "commands.codex_tracer.mod.03",
        english: "Parsing Codex JSONL files",
        chinese: "正在解析 Codex JSONL 文件",
    },
    EventTemplate {
        code: "commands.codex_tracer.mod.04",
        english: "Finished parsing JSONL files",
        chinese: "JSONL 文件解析完成",
    },
    EventTemplate {
        code: "commands.codex_tracer.mod.05",
        english: "Database already contains events",
        chinese: "数据库中已有事件",
    },
    EventTemplate {
        code: "commands.codex_tracer.server.01",
        english: "Codex Tracer dashboard listening",
        chinese: "Codex Tracer 看板正在监听",
    },
    EventTemplate {
        code: "commands.codex_tracer.server.02",
        english: "Failed to query calls",
        chinese: "查询调用记录失败",
    },
    EventTemplate {
        code: "commands.codex_tracer.server.03",
        english: "Failed to serialize payload",
        chinese: "序列化数据失败",
    },
    EventTemplate {
        code: "commands.codex_tracer.server.04",
        english: "Failed to query calls",
        chinese: "查询调用记录失败",
    },
    EventTemplate {
        code: "commands.codex_tracer.server.05",
        english: "Refresh requested",
        chinese: "已请求刷新",
    },
    EventTemplate {
        code: "commands.codex_tracer.server.06",
        english: "Codex rollout directory not found",
        chinese: "未找到 Codex rollout 目录",
    },
    EventTemplate {
        code: "commands.codex_tracer.server.07",
        english: "Failed to refresh Codex tracer events",
        chinese: "刷新 Codex 跟踪事件失败",
    },
    EventTemplate {
        code: "commands.codex_tracer.server.08",
        english: "Refresh completed",
        chinese: "刷新完成",
    },
    EventTemplate {
        code: "commands.codex_tracer.server.09",
        english: "Failed to count events",
        chinese: "统计事件数量失败",
    },
    EventTemplate {
        code: "commands.daily.01",
        english: "starting daily report output",
        chinese: "开始输出日报",
    },
    EventTemplate {
        code: "commands.daily.02",
        english: "finished daily report output",
        chinese: "完成输出日报",
    },
    EventTemplate {
        code: "commands.daily.03",
        english: "finished daily report output",
        chinese: "完成输出日报",
    },
    EventTemplate {
        code: "commands.dash.01",
        english: "Starting the terminal dashboard",
        chinese: "开始启动本地 TUI",
    },
    EventTemplate {
        code: "commands.dash.02",
        english: "Terminal dashboard closed",
        chinese: "完成本地 TUI 会话",
    },
    EventTemplate {
        code: "commands.diagnostics.01",
        english: "Exporting diagnostics JSON",
        chinese: "开始导出 diagnostics JSON",
    },
    EventTemplate {
        code: "commands.diagnostics.02",
        english: "Diagnostics JSON exported",
        chinese: "完成 diagnostics JSON 导出",
    },
    EventTemplate {
        code: "commands.diagnostics.03",
        english: "Registering an ignored file",
        chinese: "开始登记 forget-file",
    },
    EventTemplate {
        code: "commands.diagnostics.04",
        english: "Ignored file registered",
        chinese: "完成 forget-file 登记",
    },
    EventTemplate {
        code: "commands.doctor.01",
        english: "Refreshing the pricing snapshot",
        chinese: "刷新定价快照",
    },
    EventTemplate {
        code: "commands.doctor.02",
        english: "Pricing snapshot saved and costs recalculated",
        chinese: "定价快照已落盘并重算",
    },
    EventTemplate {
        code: "commands.doctor.03",
        english: "Starting health checks",
        chinese: "开始执行 doctor 健康检查",
    },
    EventTemplate {
        code: "commands.doctor.04",
        english: "Health checks completed",
        chinese: "完成 doctor 健康检查",
    },
    EventTemplate {
        code: "commands.export.01",
        english: "Exporting the offline HTML report",
        chinese: "开始导出静态 HTML 报告",
    },
    EventTemplate {
        code: "commands.export.02",
        english: "Offline HTML report exported",
        chinese: "完成静态 HTML 报告导出",
    },
    EventTemplate {
        code: "commands.focused.01",
        english: "starting focused daily report output",
        chinese: "开始输出单来源日报",
    },
    EventTemplate {
        code: "commands.focused.02",
        english: "starting focused report output",
        chinese: "开始输出单来源报表",
    },
    EventTemplate {
        code: "commands.focused.03",
        english: "starting focused session report output",
        chinese: "开始输出单来源会话报表",
    },
    EventTemplate {
        code: "commands.init.01",
        english: "bootstrapping local runtime",
        chinese: "正在初始化本地运行时",
    },
    EventTemplate {
        code: "commands.init.02",
        english: "local runtime bootstrap completed",
        chinese: "本地运行时初始化完成",
    },
    EventTemplate {
        code: "commands.mod.01",
        english: "run started",
        chinese: "命令已开始",
    },
    EventTemplate {
        code: "commands.mod.02",
        english: "run finished",
        chinese: "命令已结束",
    },
    EventTemplate {
        code: "commands.mod.03",
        english: "run failed",
        chinese: "命令失败",
    },
    EventTemplate {
        code: "commands.mod.04",
        english: "run failed",
        chinese: "命令失败",
    },
    EventTemplate {
        code: "commands.mod.05",
        english: "run failed",
        chinese: "命令失败",
    },
    EventTemplate {
        code: "commands.monthly.01",
        english: "starting monthly report output",
        chinese: "开始输出月报",
    },
    EventTemplate {
        code: "commands.monthly.02",
        english: "finished monthly report output",
        chinese: "完成输出月报",
    },
    EventTemplate {
        code: "commands.monthly.03",
        english: "finished monthly report output",
        chinese: "完成输出月报",
    },
    EventTemplate {
        code: "commands.remote.01",
        english: "importing remote shards",
        chinese: "正在导入远端分片",
    },
    EventTemplate {
        code: "commands.serve.01",
        english: "Starting the local Web server",
        chinese: "开始启动本地 Web UI 服务",
    },
    EventTemplate {
        code: "commands.serve.02",
        english: "Could not open the browser; the dashboard server is still running",
        chinese: "打开本地 dashboard 浏览器失败，服务将继续运行",
    },
    EventTemplate {
        code: "commands.serve.03",
        english: "Browser opening skipped by --no-open",
        chinese: "serve 已通过 --no-open 跳过浏览器启动",
    },
    EventTemplate {
        code: "commands.serve.04",
        english: "Browser opening skipped in this SSH session",
        chinese: "serve 在 SSH 会话中跳过浏览器启动",
    },
    EventTemplate {
        code: "commands.serve.05",
        english: "Ctrl+C received; stopping the local Web server",
        chinese: "收到 Ctrl+C，准备停止本地 Web UI 服务",
    },
    EventTemplate {
        code: "commands.serve.06",
        english: "Legacy token accounting detected; history kept and source skipped",
        chinese: "serve 检测到 legacy token accounting，已保留历史并跳过该来源",
    },
    EventTemplate {
        code: "commands.session.01",
        english: "starting session report output",
        chinese: "开始输出会话报表",
    },
    EventTemplate {
        code: "commands.session.02",
        english: "finished session report output",
        chinese: "完成输出会话报表",
    },
    EventTemplate {
        code: "commands.session.03",
        english: "finished session report output",
        chinese: "完成输出会话报表",
    },
    EventTemplate {
        code: "commands.status.01",
        english: "Preparing the status summary",
        chinese: "开始输出状态摘要",
    },
    EventTemplate {
        code: "commands.status.02",
        english: "Status summary completed",
        chinese: "完成状态摘要输出",
    },
    EventTemplate {
        code: "commands.statusline.01",
        english: "starting statusline output",
        chinese: "开始输出状态行",
    },
    EventTemplate {
        code: "commands.statusline.02",
        english: "finished statusline output",
        chinese: "完成输出状态行",
    },
    EventTemplate {
        code: "commands.sync.01",
        english: "Starting local source sync",
        chinese: "开始执行全量本地真源同步",
    },
    EventTemplate {
        code: "commands.sync.02",
        english: "bootstrap finished",
        chinese: "初始化完成",
    },
    EventTemplate {
        code: "commands.sync.03",
        english: "progress render cost",
        chinese: "进度渲染耗时",
    },
    EventTemplate {
        code: "commands.sync.04",
        english: "Local source sync completed",
        chinese: "完成全量本地真源同步",
    },
    EventTemplate {
        code: "commands.uninstall.01",
        english: "Starting legacy integration cleanup",
        chinese: "开始执行本地卸载",
    },
    EventTemplate {
        code: "commands.uninstall.02",
        english: "Legacy integration cleanup completed",
        chinese: "完成本地卸载",
    },
    EventTemplate {
        code: "commands.weekly.01",
        english: "starting weekly report output",
        chinese: "开始输出周报",
    },
    EventTemplate {
        code: "commands.weekly.02",
        english: "finished weekly report output",
        chinese: "完成输出周报",
    },
    EventTemplate {
        code: "commands.weekly.03",
        english: "finished weekly report output",
        chinese: "完成输出周报",
    },
    EventTemplate {
        code: "domain.provider_map.01",
        english: "CCR provider activation log not found; syncing without provider labels",
        chinese: "未找到 CCR 提供商激活日志；本轮不附加提供商标签",
    },
    EventTemplate {
        code: "domain.provider_map.02",
        english: "failed to load default CCR provider activation log; syncing without provider labels",
        chinese: "读取默认 CCR 提供商激活日志失败；本轮不附加提供商标签",
    },
    EventTemplate {
        code: "domain.provider_map.03",
        english: "skipping malformed provider activation line",
        chinese: "跳过格式错误的提供商激活行",
    },
    EventTemplate {
        code: "domain.provider_map.04",
        english: "skipping provider activation for unknown platform",
        chinese: "跳过未知平台的提供商激活记录",
    },
    EventTemplate {
        code: "domain.provider_map.05",
        english: "skipping provider activation with invalid timestamp",
        chinese: "跳过时间无效的提供商激活记录",
    },
    EventTemplate {
        code: "domain.provider_map.06",
        english: "skipping provider activation with unknown event",
        chinese: "跳过未知类型的提供商激活事件",
    },
    EventTemplate {
        code: "parsers.antigravity.01",
        english: "Antigravity post-read fingerprint unavailable",
        chinese: "无法取得 Antigravity 读取后指纹",
    },
    EventTemplate {
        code: "parsers.claude.01",
        english: "Syncing Claude project files",
        chinese: "开始同步 Claude 项目真源",
    },
    EventTemplate {
        code: "parsers.claude.02",
        english: "Claude project files parsed",
        chinese: "完成 Claude 项目真源解析",
    },
    EventTemplate {
        code: "parsers.codex.01",
        english: "Syncing Codex rollout files",
        chinese: "开始同步 Codex rollout 真源",
    },
    EventTemplate {
        code: "parsers.codex.02",
        english: "Codex rollout files parsed",
        chinese: "完成 Codex rollout 真源解析",
    },
    EventTemplate {
        code: "parsers.driver.01",
        english: "Starting source parsers",
        chinese: "开始驱动 SourceParser 列表",
    },
    EventTemplate {
        code: "parsers.driver.02",
        english: "source parse finished",
        chinese: "来源解析完成",
    },
    EventTemplate {
        code: "parsers.driver.03",
        english: "source inventory incomplete; skipping missing sweep",
        chinese: "来源清单不完整；跳过缺失文件清理",
    },
    EventTemplate {
        code: "parsers.driver.04",
        english: "Missing-file inventory updated",
        chinese: "标记 missing 文件完成",
    },
    EventTemplate {
        code: "parsers.driver.05",
        english: "Source parsers completed",
        chinese: "完成 SourceParser 列表驱动",
    },
    EventTemplate {
        code: "parsers.driver.06",
        english: "parse issues",
        chinese: "解析诊断",
    },
    EventTemplate {
        code: "parsers.dsh.01",
        english: "Syncing DeepSeek Harness session logs",
        chinese: "开始同步 DeepSeek Harness 会话日志真源",
    },
    EventTemplate {
        code: "parsers.dsh.02",
        english: "DeepSeek Harness session logs parsed",
        chinese: "完成 DeepSeek Harness 会话日志真源解析",
    },
    EventTemplate {
        code: "parsers.grok.01",
        english: "starting Grok Build passive session sync",
        chinese: "开始被动同步 Grok Build 会话",
    },
    EventTemplate {
        code: "parsers.grok.02",
        english: "Grok session has a missing tracked sidecar; preserving prior usage",
        chinese: "Grok 会话缺少已跟踪的辅助文件；保留既有用量",
    },
    EventTemplate {
        code: "parsers.grok.03",
        english: "finished Grok Build passive session sync",
        chinese: "Grok Build 被动会话同步完成",
    },
    EventTemplate {
        code: "parsers.kimi_code.01",
        english: "Syncing Kimi Code wire.jsonl files",
        chinese: "开始同步 Kimi Code wire.jsonl 真源",
    },
    EventTemplate {
        code: "parsers.kimi_code.02",
        english: "Kimi Code wire.jsonl files parsed",
        chinese: "完成 Kimi Code wire.jsonl 真源解析",
    },
    EventTemplate {
        code: "parsers.opencode.01",
        english: "Syncing OpenCode SQLite data",
        chinese: "开始同步 OpenCode SQLite 真源",
    },
    EventTemplate {
        code: "parsers.opencode.02",
        english: "OpenCode cursor anchor missing; resetting the high watermark",
        chinese: "检测到 OpenCode DB cursor 锚点缺失，重置高水位",
    },
    EventTemplate {
        code: "parsers.opencode.03",
        english: "OpenCode SQLite data parsed",
        chinese: "完成 OpenCode SQLite 真源解析",
    },
    EventTemplate {
        code: "parsers.pi.01",
        english: "Syncing Pi-format sessions",
        chinese: "开始同步 Pi 格式会话真源",
    },
    EventTemplate {
        code: "parsers.pi.02",
        english: "Pi-format sessions parsed",
        chinese: "完成 Pi 格式会话真源解析",
    },
    EventTemplate {
        code: "parsers.source_files.01",
        english: "failed to enumerate source file inventory",
        chinese: "枚举来源文件清单失败",
    },
    EventTemplate {
        code: "parsers.source_files.02",
        english: "failed to enumerate source file inventory",
        chinese: "枚举来源文件清单失败",
    },
    EventTemplate {
        code: "parsers.zcode.01",
        english: "Syncing ZCode SQLite data",
        chinese: "开始同步 ZCode SQLite 真源",
    },
    EventTemplate {
        code: "parsers.zcode.02",
        english: "ZCode cursor anchor missing; resetting the high watermark",
        chinese: "检测到 ZCode DB cursor 锚点缺失，重置高水位",
    },
    EventTemplate {
        code: "parsers.zcode.03",
        english: "ZCode SQLite data parsed",
        chinese: "完成 ZCode SQLite 真源解析",
    },
    EventTemplate {
        code: "query.reports.01",
        english: "loaded usage blocks",
        chinese: "用量窗口加载完成",
    },
    EventTemplate {
        code: "remote.importer.01",
        english: "skipped non-JSON remote shard lines",
        chinese: "已跳过远端分片中的非 JSON 行",
    },
    EventTemplate {
        code: "store.lock.01",
        english: "Worker lock lost to another process; this worker must stop writing",
        chinese: "SQLite worker 锁已被其他进程抢占（fencing generation 不匹配），当前 worker 应停止写入",
    },
    EventTemplate {
        code: "store.lock.02",
        english: "Worker lock heartbeat renewal failed",
        chinese: "SQLite worker 锁 heartbeat 续租失败",
    },
    EventTemplate {
        code: "store.lock.03",
        english: "Waiting for the SQLite worker lock",
        chinese: "开始等待 SQLite worker 锁",
    },
    EventTemplate {
        code: "store.lock.04",
        english: "SQLite worker lock acquired",
        chinese: "完成 SQLite worker 锁申请",
    },
    EventTemplate {
        code: "store.lock.05",
        english: "SQLite worker coordination table is being updated concurrently",
        chinese: "SQLite worker 锁协调表正被并发更新",
    },
    EventTemplate {
        code: "store.lock.06",
        english: "Trying to acquire the SQLite worker lock",
        chinese: "尝试申请 SQLite worker 锁",
    },
    EventTemplate {
        code: "store.lock.07",
        english: "SQLite worker lock is occupied",
        chinese: "SQLite worker 锁已被占用",
    },
    EventTemplate {
        code: "store.migrations.01",
        english: "Starting schema migration",
        chinese: "开始执行 schema migration",
    },
    EventTemplate {
        code: "store.migrations.02",
        english: "Schema migration completed",
        chinese: "完成 schema migration",
    },
    EventTemplate {
        code: "store.mod.01",
        english: "Recalculating event pricing",
        chinese: "开始重算定价目录",
    },
    EventTemplate {
        code: "store.mod.02",
        english: "Pricing recalculation is still running; progress will continue",
        chinese: "定价重算仍在进行",
    },
    EventTemplate {
        code: "store.mod.03",
        english: "Pricing recalculation page committed",
        chinese: "定价重算页已提交",
    },
    EventTemplate {
        code: "store.mod.04",
        english: "Reconciling pricing summary buckets",
        chinese: "开始对账定价汇总桶",
    },
    EventTemplate {
        code: "store.mod.05",
        english: "Pricing bucket reconciliation is taking longer than expected; work continues",
        chinese: "定价汇总桶对账耗时超过预期",
    },
    EventTemplate {
        code: "store.mod.06",
        english: "Pricing recalculation completed",
        chinese: "完成定价目录重算",
    },
    EventTemplate {
        code: "store.mod.07",
        english: "Pricing recalculation failed",
        chinese: "定价目录重算失败",
    },
    EventTemplate {
        code: "store.pricing_catalog.01",
        english: "Interrupted pricing recalculation detected; recovering consistent pricing",
        chinese: "发现定价重算未完成标记，正在恢复一致性重算",
    },
    EventTemplate {
        code: "store.schema.01",
        english: "Initializing runtime directories and SQLite schema",
        chinese: "开始初始化本地目录与 SQLite schema",
    },
    EventTemplate {
        code: "store.schema.02",
        english: "Runtime directories and SQLite schema initialized",
        chinese: "完成本地目录与 SQLite schema 初始化",
    },
    EventTemplate {
        code: "store.schema.03",
        english: "Resetting reconstructable usage for one source",
        chinese: "开始按源清空可重建用量数据",
    },
    EventTemplate {
        code: "store.schema.04",
        english: "Source reconstructable usage reset",
        chinese: "完成按源清空可重建用量数据",
    },
    EventTemplate {
        code: "store.schema.05",
        english: "Resetting reconstructable usage for selected sources",
        chinese: "开始按源批量清空可重建用量数据",
    },
    EventTemplate {
        code: "store.schema.06",
        english: "Selected sources reconstructable usage reset",
        chinese: "完成按源批量清空可重建用量数据",
    },
    EventTemplate {
        code: "store.schema.07",
        english: "Resetting reconstructable usage",
        chinese: "开始清空可重建用量数据",
    },
    EventTemplate {
        code: "store.schema.08",
        english: "Reconstructable usage reset",
        chinese: "完成清空可重建用量数据",
    },
    EventTemplate {
        code: "store.sync_writer.01",
        english: "Opening the sync writer",
        chinese: "开始建立 sync 单写入端",
    },
    EventTemplate {
        code: "store.sync_writer.02",
        english: "Sync writer opened",
        chinese: "完成 sync 单写入端建立",
    },
    EventTemplate {
        code: "store.sync_writer.03",
        english: "Removing old events for replay",
        chinese: "开始清理重放旧事件",
    },
    EventTemplate {
        code: "store.sync_writer.04",
        english: "Old replay events removed",
        chinese: "完成重放旧事件清理",
    },
    EventTemplate {
        code: "store.sync_writer.05",
        english: "Writing usage_event batch",
        chinese: "开始批量写入 usage_event",
    },
    EventTemplate {
        code: "store.sync_writer.06",
        english: "usage_event batch written",
        chinese: "完成批量写入 usage_event",
    },
    EventTemplate {
        code: "store.sync_writer.07",
        english: "Updating cursor batch",
        chinese: "开始批量刷新 cursor",
    },
    EventTemplate {
        code: "store.sync_writer.08",
        english: "Cursor batch updated",
        chinese: "完成批量刷新 cursor",
    },
    EventTemplate {
        code: "store.sync_writer.09",
        english: "Sync writer finished",
        chinese: "完成 sync 单写入端收尾",
    },
    EventTemplate {
        code: "store.sync_writer.10",
        english: "Committing a shard",
        chinese: "开始提交 shard",
    },
    EventTemplate {
        code: "store.sync_writer.11",
        english: "Shard committed",
        chinese: "完成 shard 提交",
    },
    EventTemplate {
        code: "store.sync_writer.12",
        english: "resetting pre-split pi rows before first omp shard",
        chinese: "首次导入 omp 分片前，重置拆分前的 pi 记录",
    },
    EventTemplate {
        code: "sync.engine.01",
        english: "run started",
        chinese: "命令已开始",
    },
    EventTemplate {
        code: "sync.engine.02",
        english: "run finished",
        chinese: "命令已结束",
    },
    EventTemplate {
        code: "sync.engine.03",
        english: "run failed",
        chinese: "命令失败",
    },
    EventTemplate {
        code: "sync.engine.04",
        english: "Starting the three-stage sync pipeline",
        chinese: "开始执行 sync 三阶段流水线",
    },
    EventTemplate {
        code: "sync.engine.05",
        english: "skipped legacy token accounting sources",
        chinese: "检测到旧版 token 计数来源；当前保留历史，跳过常规导入",
    },
    EventTemplate {
        code: "sync.engine.06",
        english: "Missing-file inventory updated",
        chinese: "标记 missing 文件完成",
    },
    EventTemplate {
        code: "sync.engine.07",
        english: "source inventory incomplete; skipping missing sweep",
        chinese: "来源清单不完整；跳过缺失文件清理",
    },
    EventTemplate {
        code: "sync.engine.08",
        english: "Antigravity lossy rebuild failed",
        chinese: "Antigravity 有损重建失败",
    },
    EventTemplate {
        code: "sync.engine.09",
        english: "source inventory incomplete; skipping missing sweep",
        chinese: "来源清单不完整；跳过缺失文件清理",
    },
    EventTemplate {
        code: "sync.engine.10",
        english: "driver finished",
        chinese: "解析驱动完成",
    },
    EventTemplate {
        code: "sync.engine.11",
        english: "skipped unreachable remote hosts",
        chinese: "已跳过无法连接的远端主机",
    },
    EventTemplate {
        code: "sync.engine.12",
        english: "stored_events queries finished",
        chinese: "库存事件查询完成",
    },
    EventTemplate {
        code: "sync.engine.13",
        english: "sync pipeline finished",
        chinese: "同步流水线完成",
    },
    EventTemplate {
        code: "sync.engine.14",
        english: "Three-stage sync pipeline completed",
        chinese: "完成 sync 三阶段流水线",
    },
    EventTemplate {
        code: "sync.engine.15",
        english: "Remote import skipped non-JSON shard lines",
        chinese: "远端导入已跳过非 JSON 分片行",
    },
    EventTemplate {
        code: "sync.engine.16",
        english: "source inventory incomplete; skipping missing sweep",
        chinese: "来源清单不完整；跳过缺失文件清理",
    },
    EventTemplate {
        code: "sync.engine.17",
        english: "Missing-file inventory updated",
        chinese: "标记 missing 文件完成",
    },
    EventTemplate {
        code: "sync.pricing_refresh.01",
        english: "pricing source refresh failed; using fallback catalog",
        chinese: "刷新价格来源失败；使用回退价格目录",
    },
    EventTemplate {
        code: "web.mod.01",
        english: "Dashboard query orphan settled",
        chinese: "看板遗留查询已结束",
    },
    EventTemplate {
        code: "web.mod.02",
        english: "detached Web server stopped unexpectedly",
        chinese: "后台 Web 服务意外停止",
    },
    EventTemplate {
        code: "web.mod.03",
        english: "Binding the local Web listener",
        chinese: "开始绑定本地 Web UI 监听端口",
    },
    EventTemplate {
        code: "web.mod.04",
        english: "Building local Web routes",
        chinese: "开始组装本地 Web UI 路由",
    },
    EventTemplate {
        code: "web.mod.05",
        english: "Local Web routes built",
        chinese: "完成本地 Web UI 路由组装",
    },
    EventTemplate {
        code: "web.mod.06",
        english: "Local Web listener bound",
        chinese: "完成本地 Web UI 监听端口绑定",
    },
    EventTemplate {
        code: "web.mod.07",
        english: "forget failed",
        chinese: "忽略文件失败",
    },
    EventTemplate {
        code: "web.mod.08",
        english: "Dashboard query timed out waiting for a permit",
        chinese: "看板查询等待执行许可超时",
    },
    EventTemplate {
        code: "web.mod.09",
        english: "Dashboard query timed out before SQLite became interruptible",
        chinese: "看板查询在 SQLite 可中断前超时",
    },
    EventTemplate {
        code: "web.mod.10",
        english: "Dashboard query timed out before awaiting blocking work",
        chinese: "看板查询在等待后台工作前超时",
    },
    EventTemplate {
        code: "web.mod.11",
        english: "Dashboard query timed out",
        chinese: "看板查询超时",
    },
    EventTemplate {
        code: "web.mod.12",
        english: "Dashboard query completed",
        chinese: "看板查询完成",
    },
    EventTemplate {
        code: "web.mod.13",
        english: "Web API response serialized",
        chinese: "Web API 响应序列化完成",
    },
    EventTemplate {
        code: "web.mod.14",
        english: "Web API response serialization failed",
        chinese: "Web API 响应序列化失败",
    },
    EventTemplate {
        code: "web.mod.15",
        english: "Web API handler failed",
        chinese: "Web API 请求处理失败",
    },
];

pub fn template(code: &str, language: Language) -> Option<&'static str> {
    EVENTS
        .iter()
        .find(|entry| entry.code == code)
        .map(|entry| language.select(entry.english, entry.chinese))
}

// serde's unit enum variants may ignore extra fields even with deny_unknown_fields.
// Compare the entire recognized shape so future facts are never silently discarded.
fn complete_facts<T: serde::de::DeserializeOwned + serde::Serialize>(raw: &str) -> Option<T> {
    let original: serde_json::Value = serde_json::from_str(raw).ok()?;
    let facts: T = serde_json::from_value(original.clone()).ok()?;
    (serde_json::to_value(&facts).ok()? == original).then_some(facts)
}

/// Shared console/log replay projection. File log readers keep their original payload.
pub fn project(
    language: Language,
    level: &str,
    message: Option<&str>,
    fields: &serde_json::Value,
    detailed: bool,
) -> String {
    let code = fields.get("event_code").and_then(serde_json::Value::as_str);
    let known = code.and_then(|code| template(code, language));
    let failure = fields
        .get("failure_facts")
        .and_then(serde_json::Value::as_str)
        .and_then(complete_facts::<crate::sync::failure::FailureKind>)
        .filter(|kind| {
            fields
                .get("failure_code")
                .and_then(serde_json::Value::as_str)
                == Some(kind.code())
        });
    let diagnostic = fields
        .get("diagnostic_facts")
        .and_then(serde_json::Value::as_str)
        .and_then(complete_facts::<super::commands::DiagnosticFacts>)
        .filter(|facts| {
            matches!(
                code,
                Some("commands.mod.03" | "commands.mod.04" | "commands.mod.05")
            ) && fields
                .get("diagnostic_code")
                .and_then(serde_json::Value::as_str)
                == Some(facts.code())
        })
        .and_then(|facts| facts.render(language));
    let inventory_kind = fields
        .get("inventory_error_kind")
        .and_then(serde_json::Value::as_str)
        .filter(|kind| {
            (*kind == "traversal"
                || (*kind == "external_io"
                    && fields
                        .get("inventory_error_detail")
                        .and_then(serde_json::Value::as_str)
                        .is_some()))
                && matches!(
                    code,
                    Some("parsers.source_files.01" | "parsers.source_files.02")
                )
        });
    let status = match level {
        "ERROR" => language.select("ERROR", "错误"),
        "WARN" => language.select("WARN", "警告"),
        "INFO" => language.select("INFO", "信息"),
        "DEBUG" => language.select("DEBUG", "调试"),
        "TRACE" => language.select("TRACE", "跟踪"),
        _ => level,
    };
    let mut line = if let Some(text) = known {
        format!("[{status}] {text}")
    } else {
        format!(
            "[{status}] {}: {}",
            language.select("Original log", "原始日志"),
            safe_text(message.unwrap_or_default())
        )
    };
    if known.is_none()
        && let Some(code) = code
    {
        line.push_str(&format!(" · event_code={}", safe_text(code)));
    }
    if let Some(failure) = &failure {
        line.push_str(": ");
        line.push_str(&safe_text(&super::sync::failure(failure, language)));
    }
    if let Some(diagnostic) = &diagnostic {
        line.push_str(": ");
        line.push_str(&safe_text(diagnostic));
    }
    if let Some(kind) = inventory_kind {
        line.push_str(": ");
        line.push_str(language.select("Cannot enumerate source inputs", "无法枚举来源输入"));
        if kind == "external_io"
            && let Some(detail) = fields
                .get("inventory_error_detail")
                .and_then(serde_json::Value::as_str)
        {
            line.push_str(&format!(
                " · {}: {}",
                language.select("Original OS detail", "原始系统详情"),
                safe_text(detail)
            ));
        }
    }
    if let Some(object) = fields.as_object() {
        for (key, value) in object {
            if (diagnostic.is_some()
                && matches!(
                    key.as_str(),
                    "diagnostic_code" | "diagnostic_facts" | "error"
                ))
                || (inventory_kind.is_some()
                    && matches!(
                        key.as_str(),
                        "error" | "inventory_error_kind" | "inventory_error_detail"
                    ))
                || key == "message"
                || key == "event_code"
                || (failure.is_some()
                    && matches!(key.as_str(), "failure_code" | "failure_facts" | "error"))
            {
                continue;
            }
            let visible = detailed
                || (diagnostic.is_none()
                    && matches!(key.as_str(), "diagnostic_code" | "diagnostic_facts"))
                || known.is_none()
                || (failure.is_none() && matches!(key.as_str(), "failure_code" | "failure_facts"))
                || (inventory_kind.is_none()
                    && matches!(
                        key.as_str(),
                        "inventory_error_kind" | "inventory_error_detail"
                    ))
                || matches!(
                    key.as_str(),
                    "source"
                        | "sources"
                        | "command"
                        | "label"
                        | "addr"
                        | "port"
                        | "processed_events"
                        | "total_events"
                        | "updated_events"
                        | "bucket_count"
                        | "deleted_orphan_buckets"
                        | "elapsed_ms"
                        | "wait_ms"
                        | "count"
                        | "inserted"
                        | "batch"
                        | "files"
                        | "files_processed"
                        | "changed_files"
                        | "skipped_files"
                        | "events_seen"
                        | "bytes_scanned"
                        | "malformed_lines"
                        | "oversized_lines"
                        | "missing_files"
                        | "protected_events"
                        | "errors"
                        | "events"
                        | "rows"
                        | "swept"
                        | "skipped"
                        | "skipped_lines"
                        | "malformed"
                        | "oversized"
                        | "accounting"
                        | "error"
                        | "reason"
                        | "detail"
                        | "records_imported"
                        | "files_scanned"
                        | "sources_count"
                        | "total"
                        | "processed"
                );
            if visible {
                let value = value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string());
                line.push_str(&format!(
                    " · {}={}",
                    field_label(key, language),
                    safe_text(&value)
                ));
            }
        }
    }
    line
}

fn field_label(key: &str, language: Language) -> &str {
    if language == Language::English {
        return key;
    }
    match key {
        "source" => "来源",
        "sources" => "来源",
        "command" => "命令",
        "label" => "主机",
        "addr" => "地址",
        "port" => "端口",
        "sources_count" => "来源数量",
        "skipped_lines" => "已跳过行数",
        "processed_events" | "processed" => "已处理",
        "total_events" | "total" => "总数",
        "updated_events" => "已更新事件",
        "bucket_count" => "汇总桶",
        "deleted_orphan_buckets" => "已清理孤立桶",
        "elapsed_ms" => "已用毫秒",
        "wait_ms" => "等待毫秒",
        "count" => "数量",
        "inserted" => "新增",
        "batch" => "批次条数",
        "files" => "文件",
        "files_processed" => "已处理文件",
        "changed_files" => "变更文件",
        "skipped_files" => "跳过文件",
        "events_seen" => "读取事件",
        "bytes_scanned" => "读取字节",
        "malformed_lines" => "格式错误行数",
        "oversized_lines" => "超长行数",
        "missing_files" => "缺失文件",
        "protected_events" => "保留事件",
        "errors" => "错误数量",
        "events" => "事件",
        "rows" => "行数",
        "swept" => "已标记缺失",
        "skipped" => "跳过",
        "malformed" => "格式错误",
        "oversized" => "超长",
        "accounting" => "计数提示",
        "error" | "reason" | "detail" => "原始详情",
        "records_imported" => "已导入",
        "files_scanned" => "已扫描",
        _ => key,
    }
}

pub fn safe_text(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| {
            if character.is_control() {
                character.escape_default().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, path::Path};
    use syn::visit::Visit;

    #[derive(Default)]
    struct Sites {
        codes: Vec<String>,
    }

    impl<'ast> Visit<'ast> for Sites {
        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            if item.attrs.iter().any(|attribute| {
                attribute.path().is_ident("cfg")
                    && attribute
                        .meta
                        .require_list()
                        .is_ok_and(|list| list.tokens.to_string() == "test")
            }) {
                return;
            }
            syn::visit::visit_item_mod(self, item);
        }
        fn visit_macro(&mut self, item: &'ast syn::Macro) {
            let name = item.path.segments.last().unwrap().ident.to_string();
            if matches!(name.as_str(), "trace" | "debug" | "info" | "warn" | "error") {
                let mut tokens = item.tokens.clone().into_iter();
                assert_eq!(
                    tokens.next().unwrap().to_string(),
                    "event_code",
                    "unregistered {name}!({})",
                    item.tokens
                );
                assert_eq!(tokens.next().unwrap().to_string(), "=");
                let value = tokens.next().unwrap().to_string();
                self.codes
                    .push(syn::parse_str::<syn::LitStr>(&value).unwrap().value());
            }
            syn::visit::visit_macro(self, item);
        }
    }

    fn scan(path: &Path, sites: &mut Sites) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().unwrap() != "tests" {
                    scan(&path, sites);
                }
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                if matches!(
                    path.file_name().unwrap().to_str(),
                    Some("tests.rs" | "profiling.rs")
                ) {
                    continue;
                }
                let file = syn::parse_file(&std::fs::read_to_string(&path).unwrap()).unwrap();
                sites.visit_file(&file);
            }
        }
    }

    #[test]
    fn production_tracing_sites_have_unique_paired_catalog_entries() {
        let mut sites = Sites::default();
        scan(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut sites,
        );
        let codes: HashSet<_> = sites.codes.iter().map(String::as_str).collect();
        assert_eq!(
            codes.len(),
            sites.codes.len(),
            "each emission has its own stable identity"
        );
        let catalog: HashSet<_> = EVENTS.iter().map(|entry| entry.code).collect();
        assert_eq!(
            codes, catalog,
            "production emitters and catalog must cover each other"
        );
        for entry in EVENTS {
            assert!(!entry.english.is_empty() && !entry.chinese.is_empty());
            assert_ne!(entry.english, entry.chinese, "{}", entry.code);
        }
    }

    #[test]
    fn typed_failure_log_projection_preserves_host_counts_and_canonical_file_detail() {
        use crate::sync::failure::{FailureKind, RebuildRisk, SyncFailure};
        let kinds = [
            FailureKind::LossyRebuild {
                risks: vec![
                    RebuildRisk {
                        host_id: "local".into(),
                        source: crate::models::SourceKind::Codex,
                        missing_files: 17,
                        protected_events: 919,
                    },
                    RebuildRisk {
                        host_id: "devbox".into(),
                        source: crate::models::SourceKind::Codex,
                        missing_files: 3,
                        protected_events: 818,
                    },
                ],
            },
            FailureKind::OmpBeforePiSplit,
            FailureKind::UnsupportedRebuild {
                source: crate::models::SourceKind::Antigravity,
            },
        ];
        for kind in kinds {
            let error: anyhow::Error =
                SyncFailure::new(kind.clone(), "canonical policy detail".into()).into();
            let (code, facts) = crate::sync::failure::log_facts(&error).unwrap();
            let fields = serde_json::json!({"event_code":"commands.mod.03", "error":error.to_string(), "failure_code":code, "failure_facts":facts});
            let original = fields.clone();
            for language in [Language::English, Language::Chinese] {
                let line = project(language, "ERROR", Some("run failed"), &fields, false);
                assert!(line.contains(language.select("[ERROR]", "[错误]")));
                assert!(!line.contains("canonical policy detail"));
                if matches!(kind, FailureKind::LossyRebuild { .. }) {
                    for fact in ["local", "devbox", "17", "919", "3", "818"] {
                        assert!(line.contains(fact), "{line}");
                    }
                }
                assert_eq!(fields, original);
                assert_eq!(error.to_string(), "canonical policy detail");
            }
        }
    }

    #[test]
    fn unknown_or_mismatched_failure_metadata_remains_available_as_original_detail() {
        for (code, facts) in [
            ("future_failure", r#"{"code":"future_failure","count":73}"#),
            ("lossy_rebuild", "{incomplete"),
            ("lossy_rebuild", r#"{"code":"omp_before_pi_split"}"#),
        ] {
            for event_code in ["commands.mod.03", "future.event"] {
                let fields = serde_json::json!({
                    "event_code": event_code, "error": "original failure detail",
                    "failure_code": code, "failure_facts": facts
                });
                for language in [Language::English, Language::Chinese] {
                    for detailed in [false, true] {
                        let line =
                            project(language, "ERROR", Some("original event"), &fields, detailed);
                        assert!(line.contains(code) && line.contains(facts), "{line}");
                        assert!(line.contains("original failure detail"), "{line}");
                        if event_code == "future.event" {
                            assert!(line.contains(language.select("Original log", "原始日志")));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn inventory_log_uses_neutral_detail_and_keeps_original_file_payload() {
        let fields = serde_json::json!({"event_code":"parsers.source_files.02", "error":"source file inventory error: private/path: permission denied", "inventory_error_kind":"external_io", "inventory_error_detail":"permission denied"});
        let original = fields.clone();
        for language in [Language::English, Language::Chinese] {
            for detailed in [false, true] {
                let line = project(
                    language,
                    "WARN",
                    Some("failed to enumerate source file inventory"),
                    &fields,
                    detailed,
                );
                assert!(
                    line.contains("permission denied")
                        && line.contains(language.select("Original OS detail", "原始系统详情"))
                );
                assert!(
                    !line.contains("source file inventory error") && !line.contains("private/path")
                );
                assert_eq!(fields, original);
            }
            for kind in ["future_kind", "external_io"] {
                let malformed = serde_json::json!({"event_code":"parsers.source_files.02", "error":"original payload", "inventory_error_kind":kind});
                for detailed in [false, true] {
                    let line = project(language, "WARN", None, &malformed, detailed);
                    assert!(line.contains("original payload") && line.contains(kind));
                }
            }
        }
    }

    #[test]
    fn pricing_projection_keeps_warning_level_counts_and_omits_default_hashes() {
        let fields = serde_json::json!({"event_code":"store.mod.02", "message":"定价重算仍在进行", "operation":"pricing_recompute", "phase":"events", "from_version":"public-0123456789abcdef0123456789abcdef", "to_version":"public-fedcba9876543210fedcba9876543210", "processed_events":70000, "total_events":399560, "elapsed_ms":31100});
        for language in [Language::English, Language::Chinese] {
            let line = project(language, "WARN", Some("定价重算仍在进行"), &fields, false);
            assert!(line.contains("70000") && line.contains("399560") && line.contains("31100"));
            assert!(line.contains(language.select("[WARN]", "[警告]")));
            assert!(
                !line.contains("0123456789abcdef")
                    && !line.contains("phase=")
                    && !line.contains("operation=")
            );
            let detailed = project(language, "DEBUG", None, &fields, true);
            assert!(
                detailed.contains("public-0123456789abcdef0123456789abcdef")
                    && detailed.contains("phase=events")
            );
        }
    }

    #[test]
    fn default_known_events_keep_source_and_protection_counts_visible() {
        let fields = serde_json::json!({
            "event_code": "parsers.codex.02",
            "files_processed": 101, "changed_files": 102, "skipped_files": 103,
            "events_seen": 104, "bytes_scanned": 105, "malformed_lines": 106,
            "oversized_lines": 107, "missing_files": 108, "protected_events": 109,
            "deleted_orphan_buckets": 110, "errors": 111,
            "path_hash": "private-hash", "from_version": "public-long-hash"
        });
        for language in [Language::English, Language::Chinese] {
            let projected = project(language, "INFO", None, &fields, false);
            for count in 101..=111 {
                assert!(projected.contains(&format!("={count}")), "{projected}");
            }
            assert!(!projected.contains("private-hash") && !projected.contains("public-long-hash"));
            assert!(projected.contains(language.select("events_seen=104", "读取事件=104")));
            assert!(projected.contains(language.select("protected_events=109", "保留事件=109")));
        }
    }

    #[test]
    fn known_remote_warning_uses_neutral_counts_and_legacy_logs_keep_original_detail() {
        let fields = serde_json::json!({"event_code":"sync.engine.15", "label":"devbox", "skipped_lines":3, "message":"skipped 3 non-JSON shard lines from host devbox"});
        let chinese = project(
            Language::Chinese,
            "WARN",
            fields["message"].as_str(),
            &fields,
            false,
        );
        assert!(chinese.contains("非 JSON") && chinese.contains("devbox") && chinese.contains('3'));
        assert!(!chinese.contains("skipped 3"));
        let legacy = project(
            Language::Chinese,
            "ERROR",
            Some("old\u{1b}[31m detail"),
            &serde_json::json!({"error":"OS failure"}),
            false,
        );
        assert!(
            legacy.contains("原始日志")
                && legacy.contains("OS failure")
                && !legacy.contains('\u{1b}')
        );
        let unknown = project(
            Language::English,
            "WARN",
            Some("future detail"),
            &serde_json::json!({"event_code":"future.01"}),
            false,
        );
        assert!(unknown.contains("future detail") && unknown.contains("future.01"));
    }
}

#[cfg(test)]
mod command_projection_tests {
    use super::*;

    #[test]
    fn domain_facts_translate_only_complete_shapes_including_unit_variants() {
        use super::super::commands::{DiagnosticFacts, DomainDiagnostic};
        for fact in [
            DomainDiagnostic::NotInitialized,
            DomainDiagnostic::LockLost,
            DomainDiagnostic::Io {
                detail: "OS_DETAIL".into(),
            },
        ] {
            let facts = DiagnosticFacts::Domain(fact);
            let known = serde_json::to_value(&facts).unwrap();
            let fields = serde_json::json!({"event_code":"commands.mod.04", "error":"CANONICAL_DOMAIN", "diagnostic_code":facts.code(), "diagnostic_facts":known.to_string()});
            for language in [Language::English, Language::Chinese] {
                let output = project(language, "ERROR", Some("run failed"), &fields, false);
                assert!(
                    output.contains(&facts.render(language).unwrap()),
                    "{output}"
                );
                assert!(!output.contains("CANONICAL_DOMAIN"), "{output}");
                for nested in [false, true] {
                    let mut future = known.clone();
                    if nested {
                        future["facts"]["future_extra"] = serde_json::json!("future detail");
                    } else {
                        future["future_extra"] = serde_json::json!("future detail");
                    }
                    let mut fields = fields.clone();
                    fields["diagnostic_facts"] = serde_json::json!(future.to_string());
                    for detailed in [false, true] {
                        let output =
                            project(language, "ERROR", Some("run failed"), &fields, detailed);
                        assert!(
                            output.contains("CANONICAL_DOMAIN") && output.contains("future detail"),
                            "{output}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn known_command_facts_project_only_with_complete_matching_metadata() {
        use super::super::commands::{DiagnosticFacts, OperationDiagnostic};
        let facts = DiagnosticFacts::Operations(OperationDiagnostic::CleanRead {
            path: "配置目录".into(),
        });
        let original = serde_json::json!({"event_code":"commands.mod.04", "error":"读取 配置目录 失败", "diagnostic_code":facts.code(), "diagnostic_facts":serde_json::to_string(&facts).unwrap()});
        for language in [Language::English, Language::Chinese] {
            let projected = project(language, "ERROR", Some("run failed"), &original, false);
            assert!(
                projected
                    .contains(language.select("Failed to read 配置目录", "读取 配置目录 失败"))
            );
            assert!(!projected.contains("diagnostic_facts"));
            for (key, value) in [
                ("diagnostic_code", "future"),
                ("diagnostic_facts", "not JSON"),
                ("event_code", "future.event"),
            ] {
                let mut fields = original.clone();
                fields[key] = serde_json::json!(value);
                for detailed in [false, true] {
                    let output = project(language, "ERROR", Some("run failed"), &fields, detailed);
                    assert!(
                        output.contains("读取 配置目录 失败")
                            && output.contains("diagnostic_facts"),
                        "{output}"
                    );
                }
            }
        }
        assert_eq!(original["error"], "读取 配置目录 失败");
    }

    #[test]
    fn incomplete_fact_shapes_keep_original_metadata_in_normal_and_detailed_logs() {
        let sync = serde_json::json!({"code":"lossy_rebuild", "risks":[{"host_id":"remote", "source":"codex", "missing_files":1, "protected_events":12}]});
        let mut sync_top = sync.clone();
        sync_top["future_extra"] = serde_json::json!("new");
        let mut sync_nested = sync;
        sync_nested["risks"][0]["future_extra"] = serde_json::json!("new");
        let command = serde_json::json!({"surface":"operations","facts":{"kind":"update_eof"}});
        let mut command_top = command.clone();
        command_top["future_extra"] = serde_json::json!("new");
        let mut command_nested = command;
        command_nested["facts"]["future_extra"] = serde_json::json!("new");
        for (key, code_key, code, facts) in [
            ("failure_facts", "failure_code", "lossy_rebuild", sync_top),
            (
                "failure_facts",
                "failure_code",
                "lossy_rebuild",
                sync_nested,
            ),
            (
                "diagnostic_facts",
                "diagnostic_code",
                "cli.update_eof",
                command_top,
            ),
            (
                "diagnostic_facts",
                "diagnostic_code",
                "cli.update_eof",
                command_nested,
            ),
        ] {
            let fields = serde_json::json!({"event_code":"commands.mod.03", "error":"CANONICAL_ERROR", code_key:code, key:facts.to_string()});
            for language in [Language::English, Language::Chinese] {
                for detailed in [false, true] {
                    let output = project(language, "ERROR", Some("run failed"), &fields, detailed);
                    assert!(
                        output.contains("CANONICAL_ERROR") && output.contains("future_extra"),
                        "{output}"
                    );
                }
            }
        }
    }
}
