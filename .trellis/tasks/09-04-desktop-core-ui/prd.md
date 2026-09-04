# Desktop core UI

父任务：`.trellis/tasks/09-04-llmusage-desktop-mvp`。依赖 `09-04-desktop-shell-ipc`。DTO/错误码只引用 shell/父 design，不另造载荷。

## Goal

用 React 19 + Vite 做出 Desktop 壳和 live 核心看板：筛选、interactive snapshot 各块、运行状态、sync center。视觉跟 `src/web/assets/base.css`。

## Requirements

- 继承父任务核心 UI 范围。
- R4: 继承父任务，只经 `runtime` invoke shell DTO。
- R7: 继承父任务，视觉来源 `src/web/assets/base.css`。
- R8: 继承父任务核心块、筛选、取消、sync 控件映射。
- R10: 继承父任务 LockBusy/LockLost 可见。
- R12: 继承父任务侧栏 `root_dir` 与锁持有者、运行状态。
- U1. `desktop/src/runtime` 是唯一 `invoke` 点。
- U2. 侧栏导航骨架含父 R8 十块 + 额度占位（额度页可空，链到 ops-quota）。侧栏展示 `runtime_info.root_dir` 与锁持有者。
- U3. 核心绘制：hero、overview/trends/models/sources/hosts/projects/costs、sync command center、运行状态（health/diagnostics）。**不**在核心请求里等待 `home_overview`；六卡由 secondary-ui 在核心绘制后独立加载。
- U4. generation + `cancel_queries`；核心 2s 标慢、6s 失败。
- U5. zh/en 与 light/dark 可切换（持久化可留给 ops-quota 的 desktop.json）。
- U6. 筛选控件 → `FilterDto`（含 1d/7d/30d/all/custom 的 since/until 与 window 映射）。sync 按钮 → `SyncStartDto`（R8.9 表）。
- U7. hosts 行数 ≤ 1 时不渲染 hosts 面板（对齐 `src/web/assets/render/hosts.js`）。

## Out of scope

- heatmap / hour-of-week / top-sessions / trends-daily / behavior / explorer / `home_overview` 六卡
- logs、CSV、额度数据、NSIS
- 自造 IPC 字段名

## Acceptance Criteria

- [ ] AC1（R8, R4）：`tauri dev` 能看到核心块，数据来自 `dashboard_interactive` IPC，路径不含 `/api/`。
- [ ] AC2（R8）：改筛选会 `generation++`、调用 `cancel_queries`（旧 `request_id`）、发出新 `dashboard_interactive`；旧响应不覆盖新筛选。
- [ ] AC3（R7, R8）：Fixture：hosts 行数 0 或 1，视口 1440×900 与 720×800。hosts 面板不出现（隐藏或未挂载，高度 0），sources 面板仍在。hosts 行数 ≥ 2 时可见并可点。
- [ ] AC4（R8, R10）：sync 按钮调用 `start_sync(SyncStartDto)` / `cancel_job`。1d/7d/30d/all/custom 与当前 source 按父 R8.9 表发载荷（可用 runtime 层单测断言 invoke 参数）。展示 running 与 `lock_busy`（含 holder）。
- [ ] AC5（R10, R12）：侧栏显示 `root_dir`。运行状态块可展示 idle / running / failed / lock_busy / lock_lost；`lock_lost` 有告警文案。diagnostics 有数据则渲染，无则明确空态。
- [ ] AC6（R8）：点击项目行写入 `project_hash` 并重拉；点击 hosts 行写入 `host_id` 并重拉。
- [ ] AC7（R7）：亮/暗主题切换后 `--bg-primary` 与 `--accent` 匹配 `base.css` 对应主题。zh/en 切换后侧栏文案切换。1440px 侧栏 248px；720px 侧栏横排且无页面横向溢出。
- [ ] AC8（R8）：`dashboard_interactive` 成功后，即使尚未收到任何次级结果，核心块已可见。
