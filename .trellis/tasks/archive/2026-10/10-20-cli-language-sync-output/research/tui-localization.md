# TUI 本地化的真实数据边界

日期：2026-10-20。基于用户新增 TUI 范围的只读分析。功能未实现/未测试。

## 1. 界面与交互

- `src/tui/app.rs:78-104` 的 Panel.label/short_label、TimeWindow 和公共状态都需按展示作用域投影，不能翻译 enum/key 本身。
- `src/tui/nav_bar.rs:55-80` 的点击区域用 `.chars().count()`，中文双宽后与 ratatui 画面不一致。render 与 hit-test 同用 display width 是核心功能验收。
- `src/tui/draw.rs:25-110` 分发九面板、SourcePicker/Help/SyncStatus；`help_dialog.rs:10-78` 与 footer 多个分支独立写死英文，不能只翻译 Panel.label。
- `src/tui/mod.rs:106-145` 的主事件循环与 draw 同线程，后台 loader/quota/sync 通过 channel 回主线程。语言在启动只读快照并在主线程作用域应用，不需要轮询设置或改 dirty tick。
- `src/tui/sync_control.rs:12-17`、`:38-61` 先把事件转成英文 String；改为中立 private feedback 或在已绑定 locale 的主线程转换。JobRegistry/事件 wire 不变，TUI 不依赖 commands::sync。

## 2. 额度数据并没有足够的 message code

不能假设已有字段就能实现完整翻译：

- `src/subscription/types.rs:43-47` 的 UsageFetchDiagnostic 只有 provider/message/severity，没有 code。
- `src/subscription/mod.rs:125-137` 将 anyhow Error 转成 String，随后丢失错误类型。
- `src/subscription/types.rs:4-10` 的 UsageMetric 只有 label、百分比、remaining_label、resets_at；`src/subscription/kimi.rs:161-174` 和 `grok.rs:187-205` 把绝对剩余额度和英文 left 拼进 String。直接从百分比重算绝对剩余会造假。
- `src/tui/panels/usage.rs:86-112`、`:422-424` 原样显示 diagnostic.message。只翻译外框会留下应用自有英文错误。
- `src/subscription/cache.rs:7-10` 的私有 CacheDocument 持有 fetched_at 与公开 report；新鲜 cache 路径在 mod.rs:73-86 直接返回 report，不能只在 live fetch 增加本地化。

## 3. 选定机制：中立展示提示，公开 report 不改

在 T 子任务增加 crate-private `QuotaPresentationHints`：记录 app-owned diagnostic kind/安全参数、metric 固定身份与绝对 remaining/limit/unit。不能存 locale 或翻译后的完整文本。providers 在现有解析/错误创建点产出 hints，不再次请求或解析凭据。

共享内部 fetch core 返回公开原 report/outcome 加 hints；现有 public fetch_all 继续原签名和结果，TUI用crate-private入口取得hints。对于缓存，私有 CacheDocument 可追加可选 `presentation` 成员（serde default/omit-empty），保存中立 hints 并与同份 report fingerprint 绑定；旧 reader 忽略未知成员，旧 cache仍可读取，TTL/cache_hit沿用原 fetched_at。公开 UsageFetchReport/Metric/Diagnostic 和 desktop DTO 不加字段，report中的原值不变。

这个可选缓存成员是明确的私有格式兼容扩展；它修复 live/cache 两条本地化路径，不能冒充「完全没有缓存格式变化」。需更新 subscription 规范并用新旧 reader/fixture 验证。不得保存凭据、token、HTTP响应正文或多余账户私密字段。

TUI private session 保存当前 hints，在同一 frame 的只读 presentation scope 中提供给 Usage renderer；不要向公开 AppState 加必填字段。只在 generation/report指纹匹配时应用hints，避免旧请求结果污染新账户/新缓存。缺 hints 的旧 cache/外部 unknown message 可显示本地化「原始详情」与原始文本，不根据英文自由文本猜 code，也不宣称旧历史已翻译。

通过 typed readiness/credential 来源显示正常标签，metric 的绝对 left 使用 hints 中原数值翻译；没有 hints 时保留已保存的原 metric detail，不用百分比推算。provider/model/plan/账户名等外部值保留。

## 4. 验证要求

- live 和 fresh-cache 同fixture、同report/数值/hints分别在en/zh呈现；cache_hit、TTL、请求计数不变。
- 旧cache没有presentation仍可加载；坏hint/不匹配指纹不影响原report，但不使用错误hint。
- 新app-owned额度诊断和left说明均有hints，unknown fallback不能掩盖新文案遗漏。
- 读取凭据前后bytes一致，r/R/m与隐藏邮箱语义不变，mock endpoints不接公网。
- 直接 public fetch_all 与旧publicstructliteral编译/序列化保持兼容；Web/desktop不新增语言设置。
- TestBackend分别覆盖九面板、所有dialog、loader/error/empty、鼠标命中与NoColor；正常窗口native证据单独记录。

相关规范：tui-presentation、tui-runtime、tui-subscription、source-sync；English-only条款依据用户新范围更新，其他契约继续保留。
