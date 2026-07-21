# TUI 首访渲染线程阻塞基准结果

日期：2026-07-21。数据库大小：1,160,073,216 bytes。窗口：30d。构建：release。
固定 120x30 TestBackend；每个面板预热一次后采 3 次样本。

命令：

```powershell
cargo test --release tui::tests::measure_local_render_thread_first_visit -- --ignored --nocapture --test-threads=1
```

每次访问只计时四个连续渲染线程区段：request dispatch、loading draw、result
apply、populated draw；等待后台查询结果的时间明确排除。命令退出码为 0，测试体
耗时 7.92s。

```text
panel=Stats sample=1 dispatch_ms=0.013 loading_draw_ms=0.117 result_apply_ms=0.001 populated_draw_ms=0.190 max_render_thread_ms=0.190
panel=Stats sample=2 dispatch_ms=0.014 loading_draw_ms=0.110 result_apply_ms=0.001 populated_draw_ms=0.210 max_render_thread_ms=0.210
panel=Stats sample=3 dispatch_ms=0.012 loading_draw_ms=0.118 result_apply_ms=0.001 populated_draw_ms=0.203 max_render_thread_ms=0.203
panel=Stats baseline_sync_ms=169.3 median_max_render_thread_ms=0.203 improvement_pct=99.9
panel=Behavior sample=1 dispatch_ms=0.015 loading_draw_ms=0.106 result_apply_ms=0.003 populated_draw_ms=0.250 max_render_thread_ms=0.250
panel=Behavior sample=2 dispatch_ms=0.017 loading_draw_ms=0.114 result_apply_ms=0.002 populated_draw_ms=0.235 max_render_thread_ms=0.235
panel=Behavior sample=3 dispatch_ms=0.013 loading_draw_ms=0.113 result_apply_ms=0.002 populated_draw_ms=0.267 max_render_thread_ms=0.267
panel=Behavior baseline_sync_ms=3777.5 median_max_render_thread_ms=0.250 improvement_pct=100.0
panel=Blocks sample=1 dispatch_ms=0.015 loading_draw_ms=0.170 result_apply_ms=0.001 populated_draw_ms=0.186 max_render_thread_ms=0.186
panel=Blocks sample=2 dispatch_ms=0.013 loading_draw_ms=0.115 result_apply_ms=0.001 populated_draw_ms=0.212 max_render_thread_ms=0.212
panel=Blocks sample=3 dispatch_ms=0.013 loading_draw_ms=0.105 result_apply_ms=0.001 populated_draw_ms=0.213 max_render_thread_ms=0.213
panel=Blocks baseline_sync_ms=403.2 median_max_render_thread_ms=0.212 improvement_pct=99.9
```

结论：父任务 X7(a) 通过。该结论仅针对渲染线程最长连续同步区段；后台查询
wall-time 仍由 X7(b) 单独记录。
