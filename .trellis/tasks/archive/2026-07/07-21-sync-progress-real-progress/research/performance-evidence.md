# A7 性能对照

日期：2026-07-21；平台：Windows；基线：`a9bf4ac`；构建：release。

## 测量协议

- 一次性生成并冻结 Codex replay 快照：同一日期 shard 下 400 个
  `rollout-*.jsonl`，共 209,762,400 bytes（200.0 MiB）。每个文件包含
  `session_meta` 和一个 512 KiB 的可解析、非 usage payload；首文件 SHA-256
  为 `E02EECA4FA9084B81F4DE14CDA902DC8F876B0206A3EC29C5D91F14E662815EF`。
- 基线和当前实现读取同一快照；每次使用独立空 `LLMUSAGE_HOME`，并固定
  `CODEX_HOME`、`HOME`、`USERPROFILE`。命令为
  `llmusage sync --source codex`，默认并行度相同，且全部文件位于一个 shard。
- 输出模式固定为 `LLMUSAGE_PROGRESS=line`，日志级别为
  `LLMUSAGE_LOG=debug`。两边各预热一次，再交替执行 9 次正式样本。
- wall time 是进程 elapsed；parse/write、`progress_dropped` 取结构化日志；
  Progress event count 取 stderr 的 Codex progress 行。
- 为记录未截断的 `RenderStats.nanos`，基线测量副本和当前实现均加入相同的
  `render_nanos = stats.nanos` debug 字段。该字段不改变 sync 事件、存储或用户输出。

## 原始样本

| 实现 | wall ms | pipeline ms | parse ms | write ms | Progress | Render calls | Render nanos | dropped |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| baseline 1 | 1467 | 588 | 541 | 2 | 1 | 34 | 41,506,300 | 0 |
| baseline 2 | 702 | 510 | 468 | 1 | 1 | 34 | 50,247,100 | 0 |
| baseline 3 | 751 | 554 | 512 | 1 | 1 | 34 | 51,702,500 | 0 |
| baseline 4 | 678 | 484 | 446 | 1 | 1 | 34 | 49,885,900 | 0 |
| baseline 5 | 750 | 549 | 505 | 1 | 1 | 34 | 60,746,000 | 0 |
| baseline 6 | 734 | 541 | 501 | 1 | 1 | 34 | 51,564,200 | 0 |
| baseline 7 | 748 | 552 | 496 | 1 | 1 | 34 | 62,747,900 | 0 |
| baseline 8 | 745 | 555 | 515 | 1 | 1 | 34 | 49,481,500 | 0 |
| baseline 9 | 788 | 598 | 556 | 1 | 1 | 34 | 49,284,900 | 0 |
| current 1 | 1445 | 530 | 485 | 1 | 4 | 37 | 58,181,900 | 0 |
| current 2 | 765 | 571 | 530 | 2 | 4 | 37 | 52,173,400 | 0 |
| current 3 | 703 | 513 | 473 | 1 | 4 | 37 | 61,074,600 | 0 |
| current 4 | 627 | 438 | 393 | 2 | 3 | 36 | 57,513,500 | 0 |
| current 5 | 715 | 510 | 468 | 1 | 4 | 37 | 61,900,800 | 0 |
| current 6 | 742 | 546 | 505 | 1 | 4 | 37 | 52,284,400 | 0 |
| current 7 | 753 | 554 | 514 | 1 | 4 | 37 | 55,335,100 | 0 |
| current 8 | 662 | 469 | 425 | 2 | 3 | 36 | 56,474,200 | 0 |
| current 9 | 662 | 463 | 418 | 2 | 3 | 36 | 51,448,800 | 0 |

## 汇总与结论

| 指标 | baseline 中位数（范围） | current 中位数（范围） |
|---|---:|---:|
| wall | 748 ms（678-1467） | 715 ms（627-1445） |
| pipeline | 552 ms（484-598） | 513 ms（438-571） |
| parse | 505 ms（446-556） | 473 ms（393-530） |
| write | 1 ms（1-2） | 1 ms（1-2） |
| Progress events | 1（1-1） | 4（3-4） |
| RenderStats.calls | 34（34-34） | 37（36-37） |
| RenderStats.nanos | 50,247,100（41,506,300-62,747,900） | 56,474,200（51,448,800-61,900,800） |
| progress_dropped | 0（0-0） | 0（0-0） |

wall 中位数变化为 `(715 / 748 - 1) * 100 = -4.41%`，满足 A7 的
“回归不超过 5%”门槛。首个正式样本在两边同时出现进程级 wall 抖动，但
pipeline 并未出现同等放大；因此结论只依据预先约定的 9 次中位数，不把
负值解释为性能提升。当前实现稳定增加了采样期/提交边界 Progress，且 9 次
均无 channel drop。
