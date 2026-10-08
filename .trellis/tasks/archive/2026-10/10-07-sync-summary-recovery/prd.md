# Sync 结果表与来源恢复提示

## 目标

普通 `llmusage sync` 遇到旧 token 记账或 Antigravity 跟踪输入不完整时，保留现有历史，并用一次可执行的提示说明后果。人读结果只保留一张连续的来源表；警告和错误放在表后，表述短、能对应到来源。

## 已确认事实

证据：`research/current-sync-output.md`。只读查看本机 `~/.llmusage/llmusage.db` 与 Antigravity 目录，未改数据。

- 2026-10-08 的那次 `llmusage sync` 使用已安装的 1.3.0。当前仓库版本是 2.0.0。
- `antigravity` 记账版本是 2，当前契约是 3。普通 sync 整源跳过，保留 3477 条。事件最新到 2026-08-31。
- CLI 发现范围内有 114 个 conversation `.db`，其中 10 个尚未进入上次清单。另外 112 个已跟踪的 `~/.gemini/tmp/**/chats/*.json` 仍在磁盘上，但不在当前发现范围。只加 `--rebuild` 仍会被这些范围外路径挡住。
- `antigravity_ide` 已是版本 3。上次成功同步记下 501 个库，现存 354 个，缺失 147 个，另有 146 个新库未导入。9591 条被整组保留，本轮提交 0。
- 1.3.0 在整组已阻断后仍解码约 394MB，并把保护说明记成 `malformed=1`。展示时去掉空格和标点，样本不可读。2.0.0 已在全部选中产品阻断时跳过 usage 解码，也不再把该说明记成 malformed。
- 人读警告目前出现在表前，来源错误和 parse issue 插在表行之间。`CHANGED 500` 表示已解码但未提交的文件。

## 需求

| ID | 可观察需求 |
| --- | --- |
| R1 | 人读成功结果是一张连续来源表，行内不再插入 `↳`、parse issue 或长警告。`TOTAL` 之后才出现 warning 或 error。 |
| R2 | 表后说明按来源分组，只保留原因、保留条数、未导入数量和一条可执行动作。不重复 tracing 原文，不输出私有路径、记录正文或 path hash。 |
| R3 | 被阻断或整源跳过时，表内 `COMMITTED` 为 0，`STORED` 仍是保留总量。已读但丢弃的文件不得显示成已变更导入。 |
| R4 | 旧记账和跟踪输入不完整继续默认保留历史。不得因为检测到问题就静默删除、重建或推进记账版本。 |
| R5 | 只有 Antigravity CLI（`antigravity`）和 IDE（`antigravity_ide`）可以在普通 sync 里接受丢失。前提是本轮覆盖发现已跟踪文件不在发现范围，或已跟踪文件已经不在磁盘。询问按来源进行；回车或选择保留时，本轮与现在的安全跳过或阻断一致。选择接受丢失时，同一次命令只重建被确认的产品，并在同一张结果表里显示结果。其他旧记账来源不询问。文件仍在、可以无损重建的旧记账也不询问。 |
| R6 | `--json-events`、stdin 管道、stdout 重定向、stderr 重定向和其他非终端运行都不询问、不读取 stdin、不阻塞。stdout 的 NDJSON 保持纯事件；人读说明不进入 JSON。 |
| R7 | 提示和表后说明要区分「范围外但仍存在的旧输入」与「磁盘上已不存在的跟踪库」，并写明各自会失去什么。 |
| R8 | 真实记录级 malformed、oversized、skipped、accounting 仍按现有分类保留，但样本改到表后；源级保护不得再显示成乱码 malformed。 |
| R9 | 带 `--recent-days` 的普通 sync 即使发现可恢复缺口，也不询问、不重建、不取消原来的时间窗口。交互终端的表后动作是去掉窗口后再运行 `llmusage sync`。非交互的表后动作仍是不带 `--recent-days` 的显式有损重建命令。 |

## 验收标准

- [ ] AC1：覆盖 R1/R2/R8。成功 sync 的人读 stdout 中，表头到 `TOTAL` 之间只有对齐的数据行。警告和错误都在 `TOTAL` 之后，TTY 有颜色，重定向无 ANSI。窄终端仍只压缩来源名，不截断数字。
- [ ] AC2：覆盖 R3。Antigravity 整组阻断时，人读表的 changed、committed、write 都是 0，stored 等于阻断前数量；bytes 不把丢弃读取显示成已导入数据。
- [ ] AC3：覆盖 R4/R7。无选择、拒绝、非交互或带时间窗口时，被保护来源的 events、buckets、cursors、inventory 和 accounting marker 不变。没有旧 `source_issues` 时，提示数量仍来自本轮磁盘覆盖。已保存诊断和磁盘不一致时，提示使用本轮数量，不用旧 JSON。旧 JSON 与缺失 `.db` 的说明不同，且都不含私有路径。
- [ ] AC4：覆盖 R5/R6/R9。交互测试可以只对 Antigravity CLI/IDE 选择保留或接受丢失，直接回车等于全部保留。接受丢失只重建被确认的产品；重建失败时该产品的 events、cursor、inventory 和 marker 一起保持原样。Codex 等其他旧记账来源没有选择，只跳过，并在表后给出现有显式命令。带 `--recent-days` 不询问。stdout 单独重定向、stderr 单独重定向、stdin 管道和 `--json-events` 都不读取 stdin，退出行为与当前安全跳过兼容。
- [ ] AC5：`--json-events` 的 stdout 每行仍是一个 JSON 事件。TUI、Web 和 doctor 不因摘要改版而改变数据合同；持久化诊断仍可在重启后读到。新文件数量只用于本次提示，不新增 dashboard 字段。

## 已确认

- 2026-10-08：交互方式采用「弹出选择」。每个会删除不可重建历史的来源可选保留或接受丢失；接受后同一次 `sync` 重建。直接回车保留全部。非交互环境不询问，只在表后警告。
- 2026-10-08，审阅 TPR-02 选择 A：当场接受丢失只限 Antigravity CLI 和 IDE。失败时事件、cursor、inventory 和记账版本一起保留。其他旧记账来源只跳过，表后给出现有显式命令。
- 2026-10-08，审阅 TPR-04 选择 A：带 `--recent-days` 的普通 sync 不弹出重建选择，维持安全跳过。表后提示另行运行不带时间窗口的完整 sync。不在这条命令里取消时间窗口。

## 范围外

不改 token 字段含义，不自动升级未确认来源的记账版本，不把范围外 JSON 重新纳入 Antigravity 解析，不新增 RPC 或桌面弹窗。不把交互接受丢失扩展到 Antigravity 以外的 parser，也不为那些来源新做失败回滚。实施验证使用隔离夹具，不对本机真实库执行 sync、rebuild 或删除。安装发布不在本任务内。

## 说明

本任务是复杂任务。`design.md` 和 `implement.md` 已按上面的确认修订。审查通过前不执行 `task.py start`。
