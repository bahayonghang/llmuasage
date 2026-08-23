# Verification and rollback evidence

## Migration

- fresh v24、v23 -> v24、同名 drifted v23、同名 exact pre-existing index、forced failure
  rollback 五类测试通过。
- forced failure 保持 schema v23，并恢复 transaction 前的同名 drifted index。
- 最终副本：pre schema v23 / post schema v24，pre/post event count 均为 256,654，
  `PRAGMA integrity_check=ok`。
- v24 主文件复用 freelist，文件字节数不变；freelist 由 46,493 降到 29,972，精确分配
  16,521 × 4,096 = 67,670,016 bytes。
- preserved v23 binary 对 v24 副本返回非零退出并识别 `SchemaTooNew`。发布后回滚边界是恢复
  verified pre-v24 backup 或继续使用支持 v24 的 binary；不得只删除索引或手调 version。

## Focused validation

- Top Sessions lib tests：12 passed，1 ignored；其中完整 pre-v24 serialized oracle matrix
  和 exact query-plan/parity tests 通过。
- v24 专项 migration tests：5 passed；全部 migration tests：30 passed。
- Sync writer focused：21 passed，2 ignored；显式 D2 ignored write benchmark 通过。
- `/api/sessions` integration：4 passed。
- `/api/sessions` `Server-Timing`：1 passed；专用 Node benchmark harness：4 passed，覆盖
  24-case 矩阵、逐样本 query timing、必需 metadata 与 privacy allowlist。
- Dashboard interrupt/hard-timeout：各 1 passed。
- Dashboard fetch/render lifecycle：37 passed，覆盖 AbortSignal、cache invalidation、sort
  stale-result rejection 与 latest-wins。

## Operational boundary

- 活动 DB 始终保持 1,160,073,216 bytes、
  `mtime_ns=1787476925136392100`、schema v23、256,654 events、integrity ok、无 v24 index。
- 所有 server 均为 loopback；lifecycle 端口 39061、harness 端口 39062 均已释放。
- 代表性 DB/WAL/SHM 与临时服务产物共 36 个文件已从经过绝对路径校验的任务专属 target
  目录移入 Windows 回收站；补采逐样本 query timing 的副本及日志另有 6 个文件采用同一
  安全流程移入回收站，均可恢复。去标识化原始矩阵与索引实验 JSON 已保留在 evidence。
- cold/first-touch 未取得真实重启证据，状态保持 **UNVERIFIED**。

## Independent review disposition

- 初次 `trellis-check` 对产品代码给出 GO，对严格任务验收给出 NO-GO：缺少 PRD R4/AC11
  要求的可重复 Top Sessions harness；该项已由专用脚本、Node harness tests、CI 接线和
  `/api/sessions` `Server-Timing` 修复。
- 初审指出的逐样本 `query_ms` 缺口已通过新代表副本补采为 120/120 条；privacy scan
  证明 filter 值、URL 和 rows 未进入 retained JSON。
- baseline 只覆盖 12 个 range shapes；implement checklist 已更正为真实边界。D1/final
  filter matrix 均完整，因此不把未采集的 baseline filter 数据表述为已验证。
- 初审机械修正两列候选索引证据：DB 文件因 freelist 复用增量为 0，但索引实际分配
  4,946 pages / 20,258,816 bytes / 1.7463%。

## Full gates

- `rtk python scripts/ci-rust.py`：通过；`cargo fmt --check`、all-target/all-feature
  clippy `-D warnings`、全特性测试与 `cargo doc --no-deps` 全部成功。Rust 汇总为
  991 passed / 8 ignored。
- `rtk just ci`：通过；CI contract self-test、Rust gate、Dashboard JS syntax/tests 与
  VitePress docs build 全部成功；其中 Top Sessions harness 4/4、Dashboard lifecycle
  28/28。完整 RTK tee log：
  `C:/Users/lyh/AppData/Local/rtk/tee/1787491335_just_ci.log`。
- `git diff --check` 与 Trellis task validate 通过；validate 仅报告既有 performance spec
  超过 context injection size 的非阻断 warning。
- 独立 `trellis-check` 最终 verdict：**GO**。复核确认 24 summaries / 120 samples、0
  non-200/degraded/非法 query timing、最大 wall/query p95 244.11/242.43 ms、最大 payload
  21,202 bytes，且 performance 文档、D2 freelist 算术、privacy allowlist 和 CI 接线一致；
  无剩余 finding。
