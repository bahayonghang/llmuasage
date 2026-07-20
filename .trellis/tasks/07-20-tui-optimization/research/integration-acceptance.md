# TUI 综合优化集成验收矩阵

日期：2026-07-20。

| Criterion | Status | Evidence |
| --- | --- | --- |
| X1 sync non-blocking/cancellable | pass | Isolated Windows Terminal smoke: `x`, Tab/9/j/o, then `q`; process stayed alive during input and exited in 176ms. Sync controller tests cover repeat/cancel and progress mapping. |
| X2 loading/generation safety | pass | Async loader TestBackend loading assertions, stale generation/filter tests, and serial suite. |
| X3 time-window bounds | pass | Time-window child tests and release 30d query/scan equivalence evidence. |
| X4 theme/no-color coverage | pass | Theme source guards, all-theme shells, no-color buffers, and serial suite. |
| X5 token/cost semantics | pass | Async/time-window payload equivalence tests and full serial suite; default window remains `All`. |
| X6 repository quality gate | pass | `just ci` exited 0, including fmt, strict clippy, Rust tests, Node checks, and docs build. |
| X7(a) render-thread blocking median | missing evidence | No three-sample before/after render-thread blocking measurement exists in `perf-baseline.md`. |
| X7(b) async payload wall time | pass | Release three-sample medians: Stats 35.8% and Behavior 52.3% improvement. |
| X7(c) idle draw count | pass | Deterministic redraw evidence: 40 idle ticks -> 0 requests; active ticks remain dirty. |

The parent remains open/planning because X7(a) is a factual acceptance gap. The
missing measurement is not converted into a pass by the isolated smoke or by
query wall-time data.
