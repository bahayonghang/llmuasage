"""Freeze the reviewed English/Chinese tracing catalog; runtime lookup uses event_code only."""
import json
from pathlib import Path

HERE = Path(__file__).parent
ROOT = Path(__file__).resolve().parents[4]
events = json.loads((HERE/'event-inventory.json').read_text(encoding='utf-8'))
english = {
    '开始启动本地 TUI': 'Starting the terminal dashboard', '完成本地 TUI 会话': 'Terminal dashboard closed',
    '开始导出 diagnostics JSON': 'Exporting diagnostics JSON', '完成 diagnostics JSON 导出': 'Diagnostics JSON exported',
    '开始登记 forget-file': 'Registering an ignored file', '完成 forget-file 登记': 'Ignored file registered',
    '刷新定价快照': 'Refreshing the pricing snapshot', '定价快照已落盘并重算': 'Pricing snapshot saved and costs recalculated',
    '开始执行 doctor 健康检查': 'Starting health checks', '完成 doctor 健康检查': 'Health checks completed',
    '开始导出静态 HTML 报告': 'Exporting the offline HTML report', '完成静态 HTML 报告导出': 'Offline HTML report exported',
    '开始启动本地 Web UI 服务': 'Starting the local Web server',
    '打开本地 dashboard 浏览器失败，服务将继续运行': 'Could not open the browser; the dashboard server is still running',
    'serve 已通过 --no-open 跳过浏览器启动': 'Browser opening skipped by --no-open',
    'serve 在 SSH 会话中跳过浏览器启动': 'Browser opening skipped in this SSH session',
    '收到 Ctrl+C，准备停止本地 Web UI 服务': 'Ctrl+C received; stopping the local Web server',
    'serve 检测到 legacy token accounting，已保留历史并跳过该来源': 'Legacy token accounting detected; history kept and source skipped',
    '开始输出状态摘要': 'Preparing the status summary', '完成状态摘要输出': 'Status summary completed',
    '开始执行全量本地真源同步': 'Starting local source sync', '完成全量本地真源同步': 'Local source sync completed',
    '开始执行本地卸载': 'Starting legacy integration cleanup', '完成本地卸载': 'Legacy integration cleanup completed',
    '开始同步 Claude 项目真源': 'Syncing Claude project files', '完成 Claude 项目真源解析': 'Claude project files parsed',
    '开始同步 Codex rollout 真源': 'Syncing Codex rollout files', '完成 Codex rollout 真源解析': 'Codex rollout files parsed',
    '开始驱动 SourceParser 列表': 'Starting source parsers', '完成 SourceParser 列表驱动': 'Source parsers completed',
    '标记 missing 文件完成': 'Missing-file inventory updated',
    '开始同步 DeepSeek Harness 会话日志真源': 'Syncing DeepSeek Harness session logs', '完成 DeepSeek Harness 会话日志真源解析': 'DeepSeek Harness session logs parsed',
    '开始同步 Kimi Code wire.jsonl 真源': 'Syncing Kimi Code wire.jsonl files', '完成 Kimi Code wire.jsonl 真源解析': 'Kimi Code wire.jsonl files parsed',
    '开始同步 OpenCode SQLite 真源': 'Syncing OpenCode SQLite data', '完成 OpenCode SQLite 真源解析': 'OpenCode SQLite data parsed',
    '检测到 OpenCode DB cursor 锚点缺失，重置高水位': 'OpenCode cursor anchor missing; resetting the high watermark',
    '开始同步 Pi 格式会话真源': 'Syncing Pi-format sessions', '完成 Pi 格式会话真源解析': 'Pi-format sessions parsed',
    '开始同步 ZCode SQLite 真源': 'Syncing ZCode SQLite data', '完成 ZCode SQLite 真源解析': 'ZCode SQLite data parsed',
    '检测到 ZCode DB cursor 锚点缺失，重置高水位': 'ZCode cursor anchor missing; resetting the high watermark',
    'SQLite worker 锁已被其他进程抢占（fencing generation 不匹配），当前 worker 应停止写入': 'Worker lock lost to another process; this worker must stop writing',
    'SQLite worker 锁 heartbeat 续租失败': 'Worker lock heartbeat renewal failed',
    '开始等待 SQLite worker 锁': 'Waiting for the SQLite worker lock', '完成 SQLite worker 锁申请': 'SQLite worker lock acquired',
    'SQLite worker 锁协调表正被并发更新': 'SQLite worker coordination table is being updated concurrently',
    '尝试申请 SQLite worker 锁': 'Trying to acquire the SQLite worker lock', 'SQLite worker 锁已被占用': 'SQLite worker lock is occupied',
    '开始执行 schema migration': 'Starting schema migration', '完成 schema migration': 'Schema migration completed',
    '开始重算定价目录': 'Recalculating event pricing', '定价重算仍在进行': 'Pricing recalculation is still running; progress will continue',
    '定价重算页已提交': 'Pricing recalculation page committed', '开始对账定价汇总桶': 'Reconciling pricing summary buckets',
    '定价汇总桶对账耗时超过预期': 'Pricing bucket reconciliation is taking longer than expected; work continues',
    '完成定价目录重算': 'Pricing recalculation completed', '定价目录重算失败': 'Pricing recalculation failed',
    '发现定价重算未完成标记，正在恢复一致性重算': 'Interrupted pricing recalculation detected; recovering consistent pricing',
    '开始初始化本地目录与 SQLite schema': 'Initializing runtime directories and SQLite schema',
    '完成本地目录与 SQLite schema 初始化': 'Runtime directories and SQLite schema initialized',
    '开始按源清空可重建用量数据': 'Resetting reconstructable usage for one source',
    '完成按源清空可重建用量数据': 'Source reconstructable usage reset',
    '开始按源批量清空可重建用量数据': 'Resetting reconstructable usage for selected sources',
    '完成按源批量清空可重建用量数据': 'Selected sources reconstructable usage reset',
    '开始清空可重建用量数据': 'Resetting reconstructable usage', '完成清空可重建用量数据': 'Reconstructable usage reset',
    '开始清理重放旧事件': 'Removing old events for replay', '完成重放旧事件清理': 'Old replay events removed',
    '开始建立 sync 单写入端': 'Opening the sync writer', '完成 sync 单写入端建立': 'Sync writer opened',
    '开始批量写入 usage_event': 'Writing usage_event batch', '完成批量写入 usage_event': 'usage_event batch written',
    '开始批量刷新 cursor': 'Updating cursor batch', '完成批量刷新 cursor': 'Cursor batch updated',
    '完成 sync 单写入端收尾': 'Sync writer finished', '开始提交 shard': 'Committing a shard', '完成 shard 提交': 'Shard committed',
    '开始执行 sync 三阶段流水线': 'Starting the three-stage sync pipeline', '完成 sync 三阶段流水线': 'Three-stage sync pipeline completed',
    '开始绑定本地 Web UI 监听端口': 'Binding the local Web listener', '开始组装本地 Web UI 路由': 'Building local Web routes',
    '完成本地 Web UI 路由组装': 'Local Web routes built', '完成本地 Web UI 监听端口绑定': 'Local Web listener bound',
}
chinese = {
    'Failed to ingest Codex tracer file': '导入 Codex 跟踪文件失败', 'Starting codex-tracer': '正在启动 codex-tracer',
    'Rebuild requested, removing existing database': '已请求重建，正在移除现有数据库', 'Parsing Codex JSONL files': '正在解析 Codex JSONL 文件',
    'Finished parsing JSONL files': 'JSONL 文件解析完成', 'Database already contains events': '数据库中已有事件',
    'Codex Tracer dashboard listening on {}': 'Codex Tracer 看板正在监听', 'Failed to query calls': '查询调用记录失败',
    'Failed to serialize payload': '序列化数据失败', 'Refresh requested': '已请求刷新', 'Codex rollout directory not found': '未找到 Codex rollout 目录',
    'Failed to refresh Codex tracer events': '刷新 Codex 跟踪事件失败', 'Refresh completed': '刷新完成', 'Failed to count events': '统计事件数量失败',
    'starting focused daily report output': '开始输出单来源日报', 'starting focused report output': '开始输出单来源报表',
    'starting focused session report output': '开始输出单来源会话报表', 'bootstrapping local runtime': '正在初始化本地运行时',
    'local runtime bootstrap completed': '本地运行时初始化完成', 'run started': '命令已开始', 'run finished': '命令已结束', 'run failed': '命令失败',
    'importing remote shards': '正在导入远端分片', 'bootstrap finished': '初始化完成', 'progress render cost': '进度渲染耗时',
    'legacy warning': '历史警告', 'first warning': '首条警告', 'interleaved warning': '交错警告', 'last warning': '最后警告', 'outside warning': '外部警告',
    'CCR provider activation log not found; syncing without provider labels': '未找到 CCR 提供商激活日志；本轮不附加提供商标签',
    'failed to load default CCR provider activation log; syncing without provider labels': '读取默认 CCR 提供商激活日志失败；本轮不附加提供商标签',
    'skipping malformed provider activation line': '跳过格式错误的提供商激活行', 'skipping provider activation for unknown platform': '跳过未知平台的提供商激活记录',
    'skipping provider activation with invalid timestamp': '跳过时间无效的提供商激活记录', 'skipping provider activation with unknown event': '跳过未知类型的提供商激活事件',
    'Antigravity post-read fingerprint unavailable': '无法取得 Antigravity 读取后指纹', 'source parse finished': '来源解析完成',
    'source inventory incomplete; skipping missing sweep': '来源清单不完整；跳过缺失文件清理', 'parse issues': '解析诊断',
    'starting Grok Build passive session sync': '开始被动同步 Grok Build 会话', 'Grok session has a missing tracked sidecar; preserving prior usage': 'Grok 会话缺少已跟踪的辅助文件；保留既有用量',
    'finished Grok Build passive session sync': 'Grok Build 被动会话同步完成', 'failed to enumerate source file inventory': '枚举来源文件清单失败',
    'loaded usage blocks': '用量窗口加载完成', 'skipped non-JSON remote shard lines': '已跳过远端分片中的非 JSON 行', 'production boundary': '生产边界',
    'resetting pre-split pi rows before first omp shard': '首次导入 omp 分片前，重置拆分前的 pi 记录',
    'skipped legacy token accounting sources': '检测到旧版 token 计数来源；当前保留历史，跳过常规导入',
    'Antigravity lossy rebuild failed': 'Antigravity 有损重建失败', 'driver finished': '解析驱动完成', 'skipped unreachable remote hosts': '已跳过无法连接的远端主机',
    'stored_events queries finished': '库存事件查询完成', 'sync pipeline finished': '同步流水线完成', '{warning}': '远端导入警告',
    'pricing source refresh failed; using fallback catalog': '刷新价格来源失败；使用回退价格目录',
    'Dashboard query orphan settled': '看板遗留查询已结束', 'detached Web server stopped unexpectedly': '后台 Web 服务意外停止', 'forget failed': '忽略文件失败',
    'Dashboard query timed out waiting for a permit': '看板查询等待执行许可超时', 'Dashboard query timed out before SQLite became interruptible': '看板查询在 SQLite 可中断前超时',
    'Dashboard query timed out before awaiting blocking work': '看板查询在等待后台工作前超时', 'Dashboard query timed out': '看板查询超时',
    'Dashboard query completed': '看板查询完成', 'Web API response serialized': 'Web API 响应序列化完成',
    'Web API response serialization failed': 'Web API 响应序列化失败', 'Web API handler failed': 'Web API 请求处理失败',
}
for period, zh in [('daily','日报'),('weekly','周报'),('monthly','月报'),('session','会话报表'),('blocks','用量窗口'),('statusline','状态行')]:
    for verb, word in [('starting','开始'),('finished','完成')]:
        chinese[f'{verb} {period} {"output" if period=="statusline" else "report output"}'] = f'{word}输出{zh}'

lines = ['// Stable, paired presentation catalog. Original tracing messages are never rewritten.',
         'use super::Language;', '', '#[derive(Debug)]', 'pub struct EventTemplate { pub code: &\'static str, pub english: &\'static str, pub chinese: &\'static str }', '',
         'pub const EVENTS: &[EventTemplate] = &[']
for event in events:
    message = event['message']
    if message in english: en, zh = english[message], message
    elif message in chinese: en, zh = message, chinese[message]
    else: raise ValueError((event['code'], message))
    if message == '{warning}': en = 'Remote import warning'
    if message == 'Codex Tracer dashboard listening on {}': en = 'Codex Tracer dashboard listening'
    lines.append('    EventTemplate { code: '+json.dumps(event['code'])+', english: '+json.dumps(en,ensure_ascii=False)+', chinese: '+json.dumps(zh,ensure_ascii=False)+' },')
lines += ['];', '', 'pub fn template(code: &str, language: Language) -> Option<&\'static str> {',
          '    EVENTS.iter().find(|entry| entry.code == code).map(|entry| language.select(entry.english, entry.chinese))', '}', '']
(ROOT/'src/common/i18n/logging.rs').write_text('\n'.join(lines), encoding='utf-8')
print(f'{len(events)} event templates written')
