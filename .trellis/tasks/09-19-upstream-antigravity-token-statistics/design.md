# Native SQLite design — accepted implementation

## Implementation authorization — 2026-09-19
用户明确要求“请开始实施”，采纳独立来源：`antigravity` 为 CLI，`antigravity_ide` 为 IDE。安装版 CLI1.2.5 和 IDE2.5.5 的独立 descriptor 与原生脱敏样本拥有字段语义证据。产品合同见 ADR0017；验收见 research/implementation-validation.md。

## Ownership
父任务负责检查点和跨 child 验收。CLI child 先裁决共享 wire/steps/token/identity；IDE child 在其合同收敛后接入原生数据库。普通 sync 保持 passive-only，无 collector/cache/RPC 新框架。

CLI conversation DB → 已校正 parser → antigravity
IDE conversation DB → 同一解码内核、IDE root adapter → antigravity_ide
两条路径经统一 identity/ownership → 产品组 staging → fenced snapshot transaction → buckets/reports。

## Contracts
R2：descriptor 证明 #1=model enum、#2=input、#4=cache write、#5=cache read、#9=reasoning、#10=visible；#3 是输出合计。总 token 为五个互斥通道之和。13 个真实脱敏样本与独立整数 oracle 支撑正常/空/失败/retry；正 cache-write 仅有 descriptor 与构造回归证据。未知 model 保留原值并可为 unpriced，不复制推测别名。
R3：只读 usage/model/time/identity/workspace/product metadata，不读正文列。typed generation/step 时间拥有事件时间，mtime/当前时钟/context-window bytes 不作替代。优先 retry attempt usage，避免与直接累计值相加；多身份去除 gen/step/复制镜像，保留独立强身份。无身份使用 file+location fallback 并显示 anomaly，不承诺跨副本去重。两根在筛选前统一归属，trajectory source17CLI/1IDE 优先目录，冲突阻断。DB+WAL 指纹在每文件读前后采样，一致只读事务 staging。
R4：每产品一个 replay group；cursor/source_file 保留全部物理成员，包括忘记记录与首次 bounded 导入。完整 snapshot 在 fenced transaction 一次替换归属 parser 行、成员与 marker3，v21 固定2；普通 sync 跳过旧语义。NULL/empty-path hook 行保留并计入历史总量，明确披露未转换。缺失/不可读/冲突/取消保留原组；缺文件仅显式 allow-lossy 可放行，不可读不豁免。bounded 不 reset/推进 full cursor，身份变更需 full sync，产品转移需完整双来源原子提交。旧 remote v2 历史仍拒绝，不添加远程恢复。
R5：新增 IDE source 与 registry、status、filters、focused command、API/query/TUI/Web/Desktop、pricing source matching 同步。CLI ready 不代表 IDE ready。JSON/数据库/看板使用 authoritative total；人类 CLI 表保留既有 visible-channel 投影。普通 sync/status 不读 credentials/调用网络。

## Trade-offs / rollout
原生读取符合 passive-only 合同；仅 .pb 的版本保持 unsupported。独立 source 显示分别覆盖，代价是新增消费者注册。没有新增依赖或迁移框架，也不自动 backfill。Windows 隔离验收已完整对账 CLI110DB 与 IDE530DB，第二次同步零新增；其他平台保持 live-UNVERIFIED。
