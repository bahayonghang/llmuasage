# Antigravity 重放预检与历史保护

## 目标

在已经知道选中产品不能提交时减少无效 usage 解码，并准确说明缺失、发现范围与修复条件。父任务：09-26-sync-diagnostics-performance；覆盖 R1/R4/R5/R8。

## 事实

本轮 IDE 解码计数 500、PARSE=6866 ms、WRITE=0。缺成员在解码前已判定；当前实现仍解码后再跳过失败组。BYTES 是数据库 file_size 累计，不能视为真实磁盘读量。当前已有 147 个跟踪路径缺失，但 501 条库存状态仍为 live。CLI 旧成员同时含 native .db 与旧 .json。

## 需求

- 全部选中产品已阻断时，在 usage 解码前返回保留历史的状态。
- 只报告确定的观察事实；区分物理缺失、当前发现范围不包含和发现失败。
- CLI/IDE 的 native 产品元数据、跨 root copy、整组 replay、WAL、bounded 与取消契约不变。
- 普通同步不得恢复为部分组覆盖；旧库存状态不被解释为实时存在性。

## 验收

- [x] A1（R4）：选中产品全阻断用例的 usage decode 调用数为 0，新增事件/重放/写入计数为 0；状态明确保留历史，events/buckets/cursors/markers 不变。
- [x] A2（R1/R5）：存在旧成员但当前 reader 不发现的用例不谎报物理删除；缺失、不可读、根目录变化与权限失败分别保守处理，报告数量与观察时点。
- [x] A3（R4/R8）：部分阻断、跨产品 copy、重复请求、unselected copy 更强身份、WAL-only 变化、取消和 bounded 同步保持既有正确性。
- [x] A4（R1/R4）：恢复成员后完整组能同步；显式 rebuild/no-lossy/allow-lossy 的历史保护仍通过，hook-era 行保持。
- [x] A5（R5）：诊断表示本次覆盖检查，不推进 usage cursor，不伪造 source_file 的成功扫描时点。保护期间旧 live 与本次 missing 的差异可解释。

## 范围外与依赖

依赖 diagnostics-contracts 的源级 code 和持久化契约。首版仅实现已证明安全的全选中产品阻断路径；部分阻断的更细粒度跳过推迟，除非本轮实施证据足以证明完整身份和归属边界。逐文件历史保留加部分新导入、自动接受有损、扩大到 backup/.pb/RPC 均不在范围内。

## 未知项

文件消失原因、旧 CLI .json 是否可由现有 native 文件无损重建仍未确定；这些事实不影响保守快速路径设计。不得通过改变恢复策略填补未知项。
