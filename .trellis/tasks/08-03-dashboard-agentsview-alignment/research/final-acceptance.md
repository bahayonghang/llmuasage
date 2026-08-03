# 最终集成验收

日期：2026-08-03

## 结论

父任务跨子任务验收标准 1–8 全部通过。四个子任务均已先行归档；父任务始终保持
`planning`，未执行 `task.py start`。

## 1. 全量质量门

**PASS**。最终命令：

```text
mise exec node@22 -- just ci
```

退出码 0；`cargo fmt --check`、严格 Clippy、580 个 library tests、全部
integration/doc tests、rustdoc、Dashboard Node tests、英文/中文 VitePress 构建均通过。

## 2. 性能预算

**PASS（含 debug 诊断备注）**。

- `sessions?sort=duration`：81.12ms，2,138B。
- `hour_of_week`：20.84ms，9,339B。
- `logs`：7.35ms/页，42,029B。
- ready-widgets 代表性范围：1d 4.1–4.3ms、7d 30.3–31.7ms、30d
  108.7–111.3ms、all 188.6–222.3ms；均低于 400ms。
- `home_overview` release 冷读三次：36.30ms、31.80ms、32.63ms，连续满足
  80ms。debug profile 在关闭浏览器与 fixture 后为 83.60ms、93.84ms、108.26ms；
  仍通过仓库既有的 150ms 非 CI debug 门限。该差异没有通过放宽本任务预算处理，
  生产/release 数据作为 80ms 验收证据保留。
- `home_overview_under_80ms_with_seeded_10k_events` debug/release 均各运行三次，
  非零测试数且全部通过。

详细子任务数据见归档任务的 `research/performance.md` 与 ready-widgets 性能研究。

## 3. 四组合 UI 与空态

**PASS**。用脱敏 fixture：

```text
cargo run --features testing --example docs_dashboard_serve -- --port 37421
```

在 1440x1100 下逐一检查 light/dark x zh/en：

- 六卡、活动日历、7x24 小时热力图、Top Sessions、每日趋势及后续面板无重叠；
- `document.body.scrollWidth == 1440`，没有页面级横向溢出；
- 中英文长标签未遮挡相邻控件；两套主题的 level 0/绿色热力标尺可辨；
- 近 1 天无数据时，新部件显示明确空态；切换 all 后显示 12 sessions、12 requests、
  268 tokens 与真实热力格；
- 390x844 抽查保持摘要卡两列、内容单列，页面宽度等于视口宽度。

浏览器 console 无异常，只有预期的信息级启动/渲染日志。

## 4. 快照兼容

**PASS**。

- `local_flow_bootstraps_and_syncs_without_installing_integrations` 真实生成
  `index.html`、`snapshot.json` 与嵌套资产，1/1 通过；
- Node 22 `dashboard-fetch.test.mjs` 9/9 通过；旧 snapshot 缺少
  `home_overview`、`heatmap`、`trends_daily`、`top_sessions`、`hour_of_week`
  时返回空态且不发 live fetch；
- Logs 在 snapshot 模式使用 live-only 提示，不尝试裸请求。

## 5. Public 404

**PASS**。真实 TCP 测试
`web::tests::public_sensitive_read_routes_are_absent_over_real_tcp` 1/1 通过。
`/api/sessions` 与 `/api/hour_of_week` 均在 loopback inventory 中，在 public
router 返回 404/405；public allowlist 仍只有 `/`、assets、dashboard、health。

## 6. Latest-wins

**PASS**。

- Node 生命周期测试覆盖 10 个 `SECONDARY_SECTIONS`、generation 丢弃、Top 排序
  reload 与 Logs filter-signature/reset 竞态；
- 浏览器内快速连续触发 7d -> all，最终 URL 保持 `range=all`，六卡仍为 all
  数据（12/12/268），未出现旧响应覆盖或残留 loading；
- 新面板均经共享 fetch/load-state 路径，无独立裸 fetch。

## 7. 视觉规格

**PASS**。对照 `agentsview-inventory.md` 第 3–4 节检查：

- 13px 紧凑排版、4/6/8px 半径、中性灰蓝主色、分类色与源身份色生效；
- Dashboard 主体为 12px gap 双栏/通栏网格，窄屏退化为单列；
- 六卡保持稳定等宽；热力图使用 GitHub light/dark 标尺；源徽章为低调文字标记，
  不是填充胶囊；
- `ASSET_MANIFEST` 精确 33 项，新模块在 live/export 使用同一资产图。

## 8. 文档与截图

**PASS**。

- `docs/dashboard/index.md` 与 `docs/zh/dashboard/index.md` 已同步新部件、CSV、
  快照与时区行为；
- `docs/prd/llmusage-integration-prd-v1.1.md` 已登记新端点；
- 权威截图已用脱敏 fixture、中文浅色、all 范围重拍：
  `docs/public/screenshots/web-dashboard-overview.png`，1440x1100，137,713B，
  SHA-256 `D9EA48507398D5A9DC4821F565483871D8A80AA1A9FE8F320B51FD9359219ED4`。

## Spec 评估

已执行 `trellis-update-spec` 评估并落地：

- `dashboard-performance-contracts.md`：会话排序、DST 小时折叠、Logs detail、CSV
  与 latest-wins 契约；
- `web-server-contracts.md`：sessions/hour-of-week public 边界，以及嵌入资产 manifest
  的登记、精确计数、live/export 同图和测试要求。
