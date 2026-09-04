# Desktop ops and quota — Implement

依赖 `09-04-desktop-core-ui`。可与 secondary-ui 并行。

## Checklist

1. logs 页 + session 跳转 + raw。
   - 文件：`desktop/src/features/logs/LogsPage.tsx`
   - 符号：`LogsDto.page_size=20`、`cursor`、`event_key`
   - 验证：单测跳转带 session；分页 cursor

2. CSV 生成 + 系统保存对话框。
   - 文件：`desktop/src/features/export/csv.ts`、`save.ts`
   - 符号：`buildAnalyticsCsv`、dialog `save`
   - 验证：单测 BOM/`escapeCsvCell`；人工保存一份文件抽检六块

3. `desktop.json` 与自动刷新。
   - 文件：`desktop/src/app/prefs.ts`
   - 符号：`PrefsDto`、`auto_refresh_ms`
   - 验证：改间隔后下一次 tick 按新值（假时钟单测）

4. 额度导航接 `fetch_quota`。
   - 文件：`desktop/src/features/quota/QuotaPage.tsx`
   - 符号：`cache_hit`、邮箱隐藏
   - 验证：AC5 fixture

5. 额度 Fixture/本地 HTTP 测试。
   - 文件：`desktop/src-tauri/tests/quota.rs`（或现有 tests 模块）
   - 符号：注入 `UsageEndpoints`、`user_home`
   - 验证：`cargo test --manifest-path desktop/src-tauri/Cargo.toml quota -- --test-threads=1`；无公网

## Validate

```
cargo test --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1
npm --prefix desktop test
```

额度测试不得访问公网主机。

## Rollback

删除 logs/额度 feature 与 prefs 写入；保留核心看板。
