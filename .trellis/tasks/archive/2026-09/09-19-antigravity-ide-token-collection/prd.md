# Antigravity IDE 原生 SQLite Token 统计

## Goal / Background
本机 IDE conversations 存在519个.db，支持先研究原生只读接入；不增加RPC。负责父任务 IDE R2/R3/R4 和 R5。结构探针未替代语义验收，见 research/native-sqlite-findings.md。

## Requirements
- R1 真源：验证 IDE gen_metadata/steps/retry 真实脱敏正常/空/失败样本，复用 CLI child 已收敛的现代解码合同。
- R2 去重/归属：IDE/native/旧根/backup 别名与复制 DB 不重复，CLI/IDE 同请求有唯一归属；不以新 source 自动假定无重叠。
- R3 生命周期：WAL、只读一致快照、失败取消/history/bounded/remote/rebuild 与现有同步一致，.pb-only 不冒充支持。
- R4 展示（P1）：建议 source antigravity_ide，保留 CLI antigravity；各消费者可筛选、区分覆盖与 unpriced。

## Acceptance Criteria
- [x] AC1（R1）：真实 normal/empty/error fixture+独立整数 oracle，step/retry/时间/model/identity 可核验；未解未知形态显示诊断。
- [x] AC2（R2）：canonical aliases、跨库备份、CLI与IDE相同/不同生成、response-only/message-only/缺 identity 有正确计数；二次 sync 零新增重复。
- [x] AC3（R3）：Windows WAL-only 新 usage 可读，busy/损坏/缺表/缺文件/取消和bounded不丢历史；remote marker/host及重建真源保护通过；其他平台未实测标UNVERIFIED。
- [x] AC4（R4）：CLI/API/TUI/Web/Desktop source/filter/status与总量按既有合同一致（JSON/数据库/看板保留 authoritative total，人类 CLI 表保留 visible-channel 投影），CLI成功不掩盖IDE失败；README/docs/spec/ADR同步且跨面 just ci 各门禁通过。

## Out of Scope
RPC/运行时服务发现/credentials/.pb解密/配额/正文归档/第三方hooks/新通用配置框架/自动回填。

## Readiness
独立 antigravity_ide 实施验收完成。E1 原生样本与 descriptor/oracle、E2 source17CLI/1IDE归属和完整双来源原子转移已验证。Windows530DB/10063events/577564225tokens逐通道对账、二次零新增与跨面各门禁通过；证据见父 research/implementation-validation.md。任务保留in_progress，未commit/archive。
