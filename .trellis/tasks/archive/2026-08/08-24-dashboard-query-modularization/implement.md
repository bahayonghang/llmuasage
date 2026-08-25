# Implementation Plan

1. Inventory
   - [x] 生成 production symbol、public re-export、serde snapshot、test leaf 与 statement count baseline。
   - [x] 为 architecture forbidden edges 添加先红后绿 fixture。
2. Extract vertical modules
   - [x] overview/timeseries。
   - [x] ranked breakdowns/context。
   - [x] behavior；必要时拆 activity/tools/optimize。
   - [x] comparison。
   - [x] diagnostics/sync center。
   - [x] snapshot composition。
3. Converge root
   - [x] 删除 root 中重复实现，收窄 visibility，保留 public re-exports。
   - [x] 检查 `mod.rs`/feature production size 与 canonical ownership。
4. Validate each move
   - [x] Focused tests、serde/statement parity、test discovery、architecture gate。
5. Final validation
   - [x] Representative dashboard range benchmark 与 10% non-regression gate。
   - [x] fmt、clippy、serial tests、rustdoc、Node checks、docs、`just ci`。

本 child 不修改 SQL 算法；若移动暴露性能缺陷，记录为后续 task，不在本任务扩大范围。
