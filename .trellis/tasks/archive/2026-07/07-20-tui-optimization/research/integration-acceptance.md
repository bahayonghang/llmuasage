# TUI 综合优化集成验收矩阵

日期：2026-07-21。

| Criterion | Status | Evidence |
| --- | --- | --- |
| X1 sync non-blocking/cancellable | pass | Isolated Windows Terminal smoke: `x`, Tab/9/j/o, then `q`; process stayed alive during input and exited in 176ms. Sync controller tests cover repeat/cancel and progress mapping. |
| X2 loading/generation safety | pass | Async loader TestBackend loading assertions, stale generation/filter tests, and serial suite. |
| X3 time-window bounds | pass | Time-window child tests and release 30d query/scan equivalence evidence. |
| X4 theme/no-color coverage | pass | Theme source guards, all-theme shells, no-color buffers, and serial suite. |
| X5 token/cost semantics | pass | Async/time-window payload equivalence tests and full serial suite; default window remains `All`. |
| X6 repository quality gate | pass | Final `just ci` exited 0 on 2026-07-21, including fmt, strict clippy, 366 library tests (3 local performance tests ignored), integration/doc tests, Node checks, and docs build. |
| X7(a) render-thread blocking median | pass | Release warm-up + three-sample medians on the representative database: Stats 0.203 ms vs 169.3 ms before, Behavior 0.250 ms vs 3777.5 ms before, Blocks 0.212 ms vs 403.2 ms before. Background query wait was excluded. |
| X7(b) async payload wall time | pass | Release three-sample medians: Stats 35.8% and Behavior 52.3% improvement. |
| X7(c) idle draw count | pass | Deterministic redraw evidence: 40 idle ticks -> 0 requests; active ticks remain dirty. |

All X1-X7 criteria have direct evidence. X7(a) measures request dispatch,
loading-frame draw, matching-result application, and populated-frame draw as
separate continuous render-thread sections; background query wait and the quit
smoke are not included in the metric.
