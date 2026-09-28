# 2026-09-26 sync 分析与优化任务

## 结论

本轮运行完成，运行记录耗时约 127 秒。Antigravity CLI 和 IDE 的历史均被保留，但两个来源都没有新增写入。Codex 的已核查 skipped 样本属于超长非用量工具输出；Grok 的 accounting 来自上游明确的不完整标记。已确认的优化问题集中于源级错误误分类、诊断定位缺失、样本省略提示、无法提交的重复解码，以及需要分段测量的写入成本。

安装命令自报 1.3.0，当前源码为 29bde59 / 1.4.0。分析分别使用本轮持久诊断、当前只读文件观察及当前源码，没有声称安装二进制对应某个精确提交。

## 逐项说明

| 项目 | 本轮事实与核查 | 处理方向 |
| --- | --- | --- |
| CLI legacy accounting | 标记为2，当前契约为3；本轮跳过写入并保留3477条事件 | 保留保护。显式 rebuild 前核实可重建输入；不自动改标记 |
| IDE tracked database missing | 保留9591条事件，新增0；稍后只读核查501个跟踪成员中147个路径缺失 | 报告缺失数量、观察时间与恢复条件；保留完整组事务 |
| IDE malformed=1 | 源级 record_failure 被写入记录级 Malformed；缺文件不能证明语法损坏 | 分离来源故障和记录解析计数，同时保持持久化和健康检查 |
| IDE 500 CHANGED / 394.0 MB / 6.9s | 缺成员先被识别，随后仍解码；BYTES为主DB大小之和，非物理读取量 | 全部选中产品已阻断时提前返回；未知归属和部分阻断保持安全路径 |
| Codex skipped=13 | 保存8条，全部为4.8–12.2 MB合法custom_tool_call_output，超过4 MiB边界 | 保留内存限制，增加明确原因和省略5条的提示；另外5条无逐条证据 |
| Grok accounting=13 | 8条保存样本均usageIsIncomplete=true；对应当前文件13条incomplete | 保留原生已报告数值和完整性警告；补足真实文件定位和offset，不补造tokens |
| WRITE=105.2s | Codex39.6s、Claude63.6s，占WRITE约98.1%；具体阶段原因未查明 | 同输入、同初态profile，然后优化实测热点 |
| 58ms锁等待与首行混行 | 锁已取得；提示与WARN拼在一行 | 锁无需作为性能根因；用TTY/non-TTY日志组合定位混行原因 |

SOURCE表中的FILES/CHANGED/SKIPPED是文件或来源工件统计，与parse issues的记录级skipped不同。SEEN、COMMITTED与STORED也不是同一个计数阶段，其差额不能直接解释为漏记。Claude的项目级重放服务于streaming/sidechain逻辑去重；Codex已具备cursor增量读取。

OpenCode、Kimi Code、ZCode、DeepSeek Harness本轮无变更，符合增量跳过。Pi本轮发现0个工件；该输出本身不证明故障。

IDE文件消失原因、Grok设置incomplete的更深原因、安装二进制提交、writer各阶段耗时及stderr混行竞态仍未确定。没有把这些未知项改写为已确认根因。

## 参考仓库更新与取舍

两仓库更新前均干净，已运行git pull --ff-only并成功快进：

| 仓库 | 更新前 | 更新后 |
| --- | --- | --- |
| ccusage | 0663eb9c7aca2c168364eff3949322e4a0f1205c | 732c7a6362f3d86a4992d2ad7071b6532161a396 |
| tokscale | d8fd670a46857e5290e71b10245dc522a344fc17 | 1d9a9395418efc6952944b794097935d7d6fa1e8 |

参考坏记录继续读取、usage结构验证、完整行cursor、缓存状态一致性和确定性测试。拒绝直接移植缺字段默认0、Grok usage/fallback混合、缺表当空数据、mtime时间回退和与本项目native descriptor不一致的Antigravity通道映射。tokscale的Codex append仍可能哈希整个旧前缀；没有可比基准支持“参考项目更快”的结论。具体行号与取舍见[上游比较](research/upstream-comparison.md)。

## 已创建的任务

父任务：[需求](prd.md)、[设计](design.md)、[实施顺序](implement.md)。三个子任务均为planning：

1. [Sync诊断分类与定位契约](../09-26-sync-diagnostics-contracts/prd.md)，P1。修正源级分类、持久化、样本省略与文件定位，验证输出边界及公开API兼容。
2. [Antigravity重放预检与历史保护](../09-26-antigravity-replay-preflight/prd.md)，P2。依赖诊断契约；全选中产品阻断时usage decode调用数为0，保留历史、cursor和marker。
3. [Sync写入测量与定向优化](../09-26-sync-write-profiling/prd.md)，P2。至少7组配对A/B，目标WRITE中位数下降20%，控制场景退化不超过10%，输出语义完全一致。百分比是待验证验收预算。

每个任务均包含prd.md、design.md、implement.md和真实的implement/check.jsonl。保留记录上限与8样本预算；默认人类输出不打印path_hash。私有诊断DTO复用现有JSON列，避免直接向公开struct增加必填字段；API相关改动需通过项目semver门禁。

## 验证与未执行项

已执行两个参考仓库更新、版本核查、两组只读脱敏探针及规划结构/上下文检查，结果见[规划验证记录](validation.md)。只读观察核对前后数据库元数据一致、无WAL。动态文件状态有各自观察时点，未作为冻结历史快照处理。

本轮未修改产品代码、原始日志或使用历史，未运行真实sync/rebuild、Cargo测试或性能基准，未安装新版本、提交或发布。后续实现按三个子任务验收，不能把本轮规划检查等同于产品回归通过。
