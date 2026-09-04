# Desktop ops and quota — Design

沿用父 `design.md` 的 logs、CSV、prefs、`fetch_quota`。

## 边界

- logs：`LogsDto.page_size=20`，游标，raw 展开，session 过滤。
- CSV：前端由已加载 payload 生成，系统保存对话框，规则同 `csv-export.js`。
- prefs：`AppPaths.root_dir.join("desktop.json")`。
- 额度：`fetch_quota`；`user_home` 与 `root_dir` 分离；失败不重载 R8。

## Change list

| 文件 | 动作 | 关键符号 |
|---|---|---|
| `desktop/src/features/logs/LogsPage.tsx` | 新建 | `LogsDto`、cursor、raw、session |
| `desktop/src/features/export/csv.ts` | 新建 | `buildAnalyticsCsv`（移植 csv-export.js） |
| `desktop/src/features/export/save.ts` | 新建 | Tauri dialog save |
| `desktop/src/app/prefs.ts` | 新建 | `load_prefs` / `save_prefs`、auto-refresh timer |
| `desktop/src/features/quota/QuotaPage.tsx` | 新建 | `cache_hit`、outputs、diagnostics、邮箱隐藏 |
| `desktop/src-tauri` 已有 `fetch_quota`/`logs`/`prefs` | 不改形状 | 若缺 dialog 插件则加 `tauri-plugin-dialog` |

## Contract

```text
LogsDto { request_id, filter, page_size: 20, cursor, include_raw_json, session, event_key }
expand row → logs({ event_key, include_raw_json: true, page_size: 20 })

buildAnalyticsCsv(data, locale) == csv-export.js 六块 + BOM
save: @tauri-apps/plugin-dialog save({ filters: [{ extensions: ["csv"] }] }) 然后写文件

auto_refresh_ms ∈ {0, 30000, 60000}
on change: clearTimeout; if !=0 startInterval(new value) immediately

fetch_quota(bypass_cache) -> { cache_hit, report }
Quota UI shows report.outputs, report.diagnostics, cache_hit
```

## Verification boundary

- Desktop cargo test：额度本地监听、凭证字节、`cache_hit`。
- 前端单测：CSV BOM + 六块标题 + 公式中和。
- 保存对话框：`tauri dev` 人工选路径（AC3）。
- 禁止公网主机。

## 已考虑不做

- 新额度提供者。
- 把额度失败重载看板。
- 运行状态主 UI（core 已做）。
