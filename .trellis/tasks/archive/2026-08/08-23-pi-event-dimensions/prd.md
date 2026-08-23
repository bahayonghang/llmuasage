# 补全 Pi 事件的 provider 与项目维度

## Goal

Pi / Oh My Pi 的每条事件带上 `provider_label` 与项目维度，并让解析器写入的
`provider_label` 不再被 sync 写入端清空。

## 背景与证据

- `src/parsers/pi.rs:435` 硬写 `provider_label: String::new()` 与 `project: None`。
  本机 423 条 `source='pi'` 事件：`provider_label` 空 423/423，`project_hash` 空 423/423。
- 真源每条 usage 记录都带 `message.provider`（2026-08-23 扫描：1198/1198，
  openrouter 989、deepseek 115、xai-oauth 86、openai-codex 8）。
- 会话头带 `cwd`（28/28，含 9 个嵌套子会话文件）。目录布局有两层形态：
  - `<root>/<encoded-cwd>/agent_<session>.jsonl`
  - `<root>/<encoded-cwd>/<run-dir>/<Name>.jsonl`（Oh My Pi 命名子会话）
  编码目录名例：`--D--Documents-Code-CLI-llmusage--`。
- 写入端冲突：`src/store/sync_writer.rs:585` 在加载了 provider 索引时，对每条事件
  无条件执行 `event.provider_label = index.label_for(event.source, &event.event_at)`，
  而 `ProviderIndex::label_for`（`src/domain/provider_map.rs:125`）对没有时间线的源
  返回空串。本机存在 `~/.ccr/analytics/provider_activation.jsonl`（39 KB），
  因此每次 sync 都会把解析器写入的标签覆盖成空串。
  现存证据：`src/parsers/dsh.rs:603` 写入了 provider，但库中
  `source='deepseek_harness'` 的 836 条事件 `provider_label` 全为空。
- ADR 0010 的「拒绝 parser-level stamping」针对的是 CCR 外部激活状态，
  不是源记录自带的 provider 字段；`dsh.rs` 已经是源自带 provider 的先例。

## Requirements

- **R2.1** `provider_label` 取自 `message.provider`，去空白；为空或缺失时保持空串，
  不做模型名推断（tokscale 的推断表属于额外维护面，本任务不引入）。
- **R2.2** `SyncRunWriter::commit_shard` 只在 `provider_label` 为空时用 provider 索引填充，
  不覆盖解析器已写入的值。该修复同时恢复 `deepseek_harness` 的 provider 标签。
- **R2.3** 项目维度优先取会话头 `cwd`，走 `ProjectResolver`（与 `src/parsers/claude.rs` 一致）。
- **R2.4** `cwd` 不可用时的回落必须取「`root` 之下第一段目录名」解码，
  **不是**文件父目录名：嵌套子会话的父目录是 `<run-dir>`，取父目录会把同一项目
  拆成多个伪项目。同一项目的顶层文件与嵌套文件必须得到同一个 `project_hash`。
- **R2.5** 不改变现有 `event_key` 组成（避免再次触发全量重建）。
- **R2.6** 隐私沿用父任务 R7：项目维度只存标签与哈希，不落库原始路径字符串。

## 非目标

- 不给缺失 provider 的记录做模型名推断。
- 不改 ADR 0010 的 CCR 发现与 `--provider-map` 语义。
- 不引入 provider 到定价目录的映射（成本在 `08-23-pi-source-cost` 处理）。
- 不改 `usage_tool_call.safe_preview` 的脱敏策略（父任务 R7，且本子任务不写行为事实）。

## 依赖

依赖 `08-23-omp-source-split` 完成（事件构造点已参数化为两个源）。
历史回填按父任务 R8 用 `sync --rebuild --source omp`，不依赖普通增量 sync。

## Acceptance Criteria

- [ ] **AC2.1**（R2.1）本机 `sync --rebuild --source omp` 后 `source='omp'` 行的
      `provider_label` 非空比例 100%，去重值与当次真源扫描的 provider 集合一致。
- [ ] **AC2.2**（R2.1）单测：`provider` 为正常值、空串、缺失三种形态，
      分别得到原值、空串、空串，且事件都入库。
- [ ] **AC2.3**（R2.2）单测：`commit_shard` 在 provider 索引存在时不覆盖非空
      `provider_label`，仍填充空 `provider_label`。
- [ ] **AC2.4**（R2.2）本机 `sync --rebuild --source deepseek_harness` 后
      `source='deepseek_harness'` 的 `provider_label` 空值数为 0；
      `codex` / `claude` 的 CCR 标签行为不变（重建前后去重值集合一致）。
- [ ] **AC2.5**（R2.3）单测：会话头有 `cwd` 且指向 git 仓库时，`project_ref` 与
      `repo_root_hash` 由 `ProjectResolver` 填充。
- [ ] **AC2.6**（R2.4）单测：对 `<root>/<encoded>/<run-dir>/<Name>.jsonl` 与
      `<root>/<encoded>/agent_x.jsonl` 两个文件，在 `cwd` 不可用时得到**相同**的
      `project_hash`；断言该哈希不是由 `<run-dir>` 派生。
- [ ] **AC2.7**（R2.3/R2.4）单测：`cwd` 与编码目录名都不可用时 `project` 为 `None`，
      事件仍入库。
- [ ] **AC2.8**（R2.4）本机验证：某个嵌套子会话文件产生的行与同项目顶层文件产生的行
      `project_hash` 相同，且 `project_label` 为仓库名（例如 `llmusage`）。
- [ ] **AC2.9**（R2.6）负向断言：`usage_event` 的 `session_label` 与项目字段里不出现
      `agent/sessions` 这类原始路径片段。
- [ ] **AC2.10**（R2.2）`docs/adr/0010-provider-label-dimension.md` 增补一节说明
      「源自带 provider 优先，CCR 时间线只填空」，并记录 dsh 的修复。
