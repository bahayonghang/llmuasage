# Desktop core UI — Implement

依赖 `09-04-desktop-shell-ipc`。

## Checklist

1. Vite + React 19 壳。
   - 文件：`desktop/src/main.tsx`、`desktop/src/app/shell.tsx`、`desktop/src/styles/tokens.css`、`layout.css`
   - 符号：十块+额度占位导航、248px 侧栏
   - 验证：`npm --prefix desktop test` 或 `tauri dev` 打开壳

2. `runtime` invoke 封装。
   - 文件：`desktop/src/runtime/invoke.ts`
   - 符号：`invokeCommand`、request_id、`cancel_queries`
   - 验证：单测 mock invoke 记录参数

3. 筛选与 sync 映射。
   - 文件：`desktop/src/app/filters.ts`
   - 符号：`rangeToFilterDto`、`syncOptionsFromState`
   - 验证：单测 1d/7d/30d/all/custom + source；载荷无 `rebuild`

4. 核心加载时序。
   - 文件：`desktop/src/app/load-state.ts`
   - 符号：`loadDashboardProgressive`、generation、2s/6s
   - 验证：单测旧 generation 丢弃；成功后不调用 `home_overview`

5. 核心面板 + hosts 规则 + 项目/host 钻取。
   - 文件：`desktop/src/features/**`
   - 符号：`renderHosts` 条件 `length > 1`
   - 验证：fixture 单测 AC3、AC6

6. 运行状态与侧栏 `root_dir`/锁。
   - 文件：`desktop/src/features/status/StatusPanel.tsx`、`shell.tsx`
   - 符号：状态枚举
   - 验证：fixture 各状态文案

7. 主题/语言/视口。
   - 文件：`tokens.css`、i18n 字典
   - 验证：AC7（`tauri dev` 人工 + token 值单测）

## Validate

`tauri dev` 打开核心块；改筛选旧响应不覆盖。`npm --prefix desktop test` 覆盖映射与 hosts 规则。未改根 `src/` 时不跑满 `just ci`。

## Rollback

还原 `desktop/src` 中壳与核心 feature，保留 src-tauri command。
