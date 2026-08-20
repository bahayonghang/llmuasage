# C3 技术设计

设计依据：父任务 `design.md` 第 6 节。本文件只记录父设计未覆盖的子任务级细节。

## 过滤层

两条过滤面都要加 `host_id`，不能只改 `QueryFilter`。

### dashboard / explorer

`src/query/filter.rs` 的 `QueryFilter` 增加 `host_id: Option<String>`，`Default` 填 `None`。

host 条件加在共用实现 `sql_filter_with_model_column`（`query/filter.rs:98-104`）。`bucket_filter` / `event_filter` / `tool_filter` 都调用它；`turn_filter` 也直接调用它（`query/filter.rs:66-68`）。只改包装函数 `sql_filter` 时，`turn_filter` 不会带 host，AC8d 失败。

`host_id` 是 TEXT，绑定为 `rusqlite::types::Value::Text`，与既有 `source` / `model` 的绑定方式一致。

### CLI 报表

daily / weekly / monthly / session / blocks / focused 调用 `ReportCommonArgs::to_filter`，得到 `query::reports::ReportFilter`（`query/reports.rs:26-35`），再经 `push_bucket_filter` / `visit_filtered_events` 过滤。这条路径不经过 `QueryFilter`。

`ReportFilter` 增加 `host_id: Option<String>`。`to_filter` 把 `--host <LABEL>` 解析为 `host_id`。`push_bucket_filter` 与 `visit_filtered_events` 追加 `host_id = ?`。

解析失败时报错并列出已注册 label（AC8b）。`--source` 由 clap 的 `value_enum` 在解析期拒绝无效值；`--host` 的合法值来自数据库，只能在运行期校验。

## 每主机行

新增与 `load_daily_reports_by_source`（`query/reports.rs:597`）同构的函数：`load_daily_reports_by_host`、`load_monthly_reports_by_host`、`load_weekly_reports_by_host`。分组键从 `SourceKind` 换成 `host_id` 加 `label`。

`build_daily_reports_by_source`（`query/reports.rs:666`）的分组逻辑可抽出一层按任意分组键的公共实现，避免三套复制。若抽取会牵动过多既有调用点，则按现有形状平行新增，不重构既有函数。判断标准：抽取后既有 per-source 测试不需要改动即可通过；否则平行新增。

## dashboard

- payload：`/api/dashboard` 增加 `hosts` 字段，形状与既有 `sources` 字段平行。遵守 `dashboard-performance-contracts.md` 的查询与负载预算，主机分组用与 sources 相同的聚合查询形状，不新增全表扫描。
- 前端：新增 `src/web/assets/render/hosts.js`，镜像 `render/sources.js` 的结构与导出形式。在 `app.js` 的渲染生命周期中注册，不改动 `render/sources.js`。
- 文案：面板字符串进 `src/web/assets/copy.js`（与 `sources` 分组同形）；`i18n.js` 只做 DOM key 应用，不在那里堆新文案。中英两套。

`--public` 的只读聚合 allowlist（`docs/safety/index.md:126`）：主机 label 是用户自定义字符串，可能含主机名等信息。`hosts` 字段默认不进入 public allowlist，与 projects、本地路径字段的处理一致。

## 单主机场景的展示

只有 `local` 一台主机时，主机分组只有一行，信息量为零。dashboard 在 `hosts.length <= 1` 时隐藏主机分组；CLI 的每主机行是显式可选项，不受影响。这样未注册任何远端的用户看不到新增的空面板。

## 编辑约束

`src/web/assets/` 下的 JS 使用单引号（`render/sources.js:1`），仓库无 prettier 配置文件。本机存在全局 prettier 格式化 hook，会把单引号改为双引号并破坏 CI 中的 JS 检查。因此这些文件用 Bash 写入（`cat > file <<'EOF'`），不用 Edit / Write 工具。
