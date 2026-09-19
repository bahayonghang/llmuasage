# 参考仓库检查与 Antigravity CLI / IDE Token 统计

## Goal
固定本次 ccusage/tokscale 检查点，并按用户后续“请开始实施”完成 Antigravity CLI 计数语义校正与 IDE 原生 SQLite 接入。

## Confirmed Facts
- 2026-09-19 核对 ccusage HEAD `0663eb9c7aca2c168364eff3949322e4a0f1205c`、tokscale HEAD `d8fd670a46857e5290e71b10245dc522a344fc17` 均等于当时 origin HEAD；无 fetch/pull。llmusage 起始 `d1108b0` / dev / clean。
- 起始版本 `d1108b0:src/parsers/antigravity.rs:466` 的 #1/#9/#10 解释与 ccusage 新 adapter 冲突；现已按独立安装版 descriptor 校正，旧 fixture 相等不作为正确性证据。
- ccusage 在 8 月 31 日恢复 Antigravity SQLite adapter，支持 gen_metadata、steps/retry 和跨库 identity 去重。本机 CLI 有 110 个 .db、IDE 有 519 个 .db；部分只读样本确认两者具有 gen_metadata/steps，存在 WAL。
- IDE 原生 SQLite 优先；RPC、.pb 解密不进入本轮。完整原始证据边界见 research/native-sqlite-findings.md。

## Requirements
- R1 检查点：保存 hash、日期、关键提交、当前实现对照、候选优先级，供下次精确增量比较。
- R2 语义：接受真实脱敏 CLI/IDE 样本，裁决模型编号、input/cache、visible output/reasoning、retry、total 与 identity；据证据校正 parser，未知格式明确降级/阻断。
- R3 覆盖：补齐现代逐轮时间、steps/retry、IDE 原生根和 WAL 变化，避免镜像/跨源重复计数。
- R4 历史：保留 hook-era 和不可重建记录；新旧不兼容语义不能静默混入。accounting version、显式修复、host/remote 与 bounded sync 必须一致。
- R5 展示：CLI/API/TUI/Web/Desktop 能区分 CLI/IDE 来源与不完整覆盖；token 未知与 cost unpriced 分开。

## Task Map
| Child | Owner | Scope |
| --- | --- | --- |
| 09-19-antigravity-cli-accounting-refresh | R2, CLI R3/R4 | 校正字段语义、现代时间/steps/retry、重放和历史策略 |
| 09-19-antigravity-ide-token-collection | IDE R2/R3/R4, R5 | 原生 SQLite 接入、产品归属、独立来源展示 |
父任务拥有 R1 和跨子任务验收。Pi/Claude 去重、时间相关价格仅列后续候选，不自动实施。

## Acceptance Criteria
- [x] AC1（R1）：两个快照与远端核对，历史基线不可解析明确说明，已存在与新增机会分开。
- [x] AC2（R2）：真实正常/空/失败脱敏 fixture 和独立 oracle 证明六通道，#1 模型编号不误作 token；没有仅凭 #3=#9+#10 推断通道名称。
- [x] AC3（R3）：跨日、steps-only/retry、idx 缺口、重复 identity、跨库备份、CLI/IDE 共存和 WAL-only 已提交更改均有验证，第二次 sync 幂等。
- [x] AC4（R4）：版本切换/显式修复不删 hook history，旧未变化 parser 行不悄悄变为新语义；失败/取消/损坏保留；remote host/marker/bounded 验证通过。
- [x] AC5（R5）：各消费者按既有合同显示总量/来源/诊断；JSON/数据库/看板使用 authoritative total，人类 CLI 表保留 visible-channel 投影；Windows 原生读取实测和跨面门禁通过，其他平台未测不标 PASS。

## Out of Scope
RPC/订阅配额/云账号/.pb 解密/读取正文列/第三方 hooks/tokscale 运行时依赖/自动重建回填/其他平台同步实施/commit/push/archive。

## Decisions / Blockers
P1 已关闭：用户“请开始实施”采纳独立 CLI/IDE 推荐，保留 antigravity、新增 antigravity_ide。
E1 已关闭：安装版 CLI/IDE descriptor、13 个脱敏原生 fixture 和独立全量整数 oracle，详见 research/native-semantic-validation.md。正 cache-write 未在原生语料观察到，使用明确标注的 schema-based 构造用例。
E2 已关闭：分产品完整 staging、原子来源组替换；显式 rebuild 仅替换可归属 parser 行，保留 hook 历史并提示；v21 固定2，现代两来源为3；旧 remote 历史继续严格拒绝，远程恢复不是本轮新增功能。
E3 已关闭：Windows CLI110DB/IDE530DB 隔离导入逐通道对账和幂等、失败原子性、跨面各门禁及独立审查通过。初次全门禁被未改动的 Desktop 异步测试波动中断，desktop-check 重跑完整通过，其余结果见验证报告；不声称单次 uninterrupted just ci 成功。其他平台只报告自动化或静态证据，不标 live PASS。

## Artifact Status
研究、实施和验收完成；证据与 AC 映射见 research/implementation-validation.md，历史规划审查保留在 research/validation.md。父子任务保留 in_progress，等待另行授权 commit/archive。
