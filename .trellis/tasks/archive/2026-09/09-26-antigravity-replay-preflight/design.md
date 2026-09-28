# Antigravity 预检设计

## 流程

现有入口 sync_antigravity_family 保持唯一。顺序为：选中来源与旧 membership → 同时发现两个 native root → 有界 fingerprint/覆盖检查 → 结构化阻断决定 → 必要 usage 解码/跨 root 归属 → 完整快照 → fenced commit。

在覆盖检查完成且所有选中产品均失败时返回，不打开 native usage 表、不运行 observations 解码。保留发现阶段必要的文件 metadata 与 fingerprint 成本；不宣称零 I/O。不要将未选中 legacy 来源的失败单独当作其他选中产品必然失败。

## 不完整发现与旧 membership

discovered 不包含某路径，不总等于路径不存在。预检报告已知的物理缺失、当前 root/扩展名不覆盖和发现失败，保守保留历史。旧 JSON membership 只作为兼容性诊断事实；不能仅因为新 reader 不支持就删除。没有无损覆盖证据时显式 rebuild 仍不能静默认证。

本次覆盖结果进入子任务 1 的源级诊断。source_file 的最后成功库存与 usage cursor 不因故障而推进；需要实时状态的展示读取有时间界限的观察事实。远端主机路径不在本地做 exists 检查。

## 部分阻断

先保留现有解码路径。目录不能证明产品归属，复制数据库及更强身份可能改变候选组。若后续优化部分阻断，必须先给出稳定元数据覆盖和反例验证，不能忽略未知或未选中 root。没有证据就不加分支。

## 保留写入机制

仍使用 commit_antigravity_snapshot。产品组 reset、cursors、accounting marker 和跨来源转移在既有 fenced 事务中完成。bounded 不 reset/不推进完整 cursor；取消/不完整快照保留旧组。没有 schema 或 accounting version 变更。

## 验证方法

使用现有 native fixture factory，增加 decode 次数的测试 seam，证明全阻断时 0 调用；生产统计只计实际执行工作，不用文件总大小伪造节省字节。对同一输入比较完整事件 key/tokens/cost、bucket、cursor、source membership 与 marker。回滚为移除提前返回与新诊断生产逻辑，无须改数据。
