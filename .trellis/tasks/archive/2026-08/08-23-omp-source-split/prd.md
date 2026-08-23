# 拆出 omp 独立源

## Goal

`~/.omp/agent/sessions` 产出的用量记为独立源 `omp`，`~/.pi/agent/sessions` 保留源 `pi`，
并在默认 sync、限定源 sync、远端导入三条路径上都保证不出现 token 双计。

## 背景

`src/parsers/source_files.rs:167` 的 `list_pi_session_files()` 把 `.pi` 与 `.omp` 两根
合并后交给同一个 `PiParser`，事件统一写 `source = pi`、`event_key` 前缀 `pi:`
（`src/parsers/pi.rs:433`）。本机 423 条 `source='pi'` 事件全部来自 `.omp`。

迁移面的既有行为核对结果见父任务 `design.md` D1 表格，三条路径各需一个机制。

## Requirements

- **R1.1** 新增 `SourceKind::Omp`，稳定 id `omp`，CLI `--source omp` 可用，展示名 `Oh My Pi`。
- **R1.2** `omp` 的发现根为 `~/.omp/agent/sessions`；`pi` 的发现根为 `PI_AGENT_DIR`
  （可逗号分隔）或 `~/.pi/agent/sessions`。两个源各自持有 `source_file` 清单与文件游标。
- **R1.3** 同一个 canonical 路径只能被一个源计入，判定必须**按路径**且**双向**：
  `PI_AGENT_DIR` 既可能是 `.omp` 根的祖先，也可能是它的子目录
  （例如 `PI_AGENT_DIR=~/.omp/agent/sessions/<project>`）。归属冲突时 `pi` 优先，
  `omp` 只跳过冲突的那些路径，不整根丢弃。
- **R1.4** 默认 `sync` 在升级后首次运行时清掉存量 `source='pi'` 事件并按新归属重放。
  保真判据是身份集合覆盖，不是总量比较：以升级前导出的
  `(source_path_hash, event_at, model, total_tokens)` 集合为基线，升级后 `omp` 必须覆盖
  基线每一条。
- **R1.5** `omp` 在 source-status、platform monitor、报表、看板、CLI 帮助与
  `diagnostics` 错误提示里作为独立源出现。
- **R1.6** 解析逻辑不复制：`pi` 与 `omp` 共用同一份记录解析实现。
- **R1.7** `sync --source omp` 在存量 `pi` 行仍是拆分前口径时不得写入 `omp` 行；
  按父任务 D1.2 的决定，带明确信息拒绝。
- **R1.8** 远端主机路径按父任务 D1.3 实现 host 级一次性迁移；若实现中证明不可行，
  必须在文档与命令输出里写明手动步骤，二者必须择一落地并有测试。

## 非目标

- 不改 token 归一算法（权威 total、reasoning 独立不入 total 保持不变）。
- 不给 `omp` 增加定价目录行（成本由 `08-23-pi-source-cost` 处理）。
- 不新增 `provider_label` / project / 行为维度（另两个子任务处理）。
- 不顺手补 `zcode` / `deepseek_harness` 在硬编码源清单里的缺失（见父任务
  `implement.md` 的「已知的既有缺口」）。

## Acceptance Criteria

- [ ] **AC1.1**（R1.1）`llmusage daily --source omp` 与 `llmusage source-status` 认识 `omp`；
      `SourceKind::parse_id("omp")` 有单测。
- [ ] **AC1.2**（R1.2）单测：`PI_AGENT_DIR` 未设置时 `pi` 只枚举 `~/.pi/...`，
      `omp` 只枚举 `~/.omp/...`；两个清单无交集。
- [ ] **AC1.3**（R1.3）单测三种重叠形态各一例：两根相等、pi 根包含 omp 根、
      omp 根包含 pi 根。每种情况下同一文件只出现在一个源的清单里，且未冲突的
      `.omp` 文件仍被 `omp` 枚举。
- [ ] **AC1.4**（R1.4）集成测试：构造拆分前的 `pi` 行与 `.omp` 文件，跑一次完整
      `sync`，断言 `pi` 行被重置、`omp` 行覆盖基线身份集合、差集为空。
- [ ] **AC1.5**（R1.4）单测：同一条记录在 `pi` 与 `omp` 下的 `event_key` 前缀分别为
      `pi:` 与 `omp:` 且不相等；各自重放幂等。
- [ ] **AC1.6**（R1.7）集成测试：存量 `pi` 行未迁移时 `sync --source omp` 返回错误、
      不写入任何 `omp` 行；完成迁移后同一命令正常执行。
- [ ] **AC1.7**（R1.8）测试覆盖所选的远端方案：host 级迁移则断言来自某 host 的
      `omp` shard 会清掉该 host 的 `pi` 行且只清一次（标记幂等）；走文档兜底则断言
      文档与命令输出包含手动步骤。
- [ ] **AC1.8**（R1.5）`README.md`、`README.zh-CN.md`、`docs/index.md`、
      `docs/zh/index.md`、`docs/guide/first-sync.md`、`docs/architecture/index.md`、
      `docs/reference/cli.md`、`docs/dashboard/index.md`、`docs/agents/passive-source-candidates.md`
      与 `docs/zh/` 对应页面列出 `omp`；`src/commands/help.rs` 与
      `src/commands/diagnostics.rs` 的源清单字符串也列出 `omp`。
- [ ] **AC1.9**（R1.6）`pi` 与 `omp` 共用一份解析实现，没有复制的记录解析函数
      （评审确认 + `PiFormatParser` 双实例注册的单测）。
- [ ] **AC1.10**（R1.1）新增 ADR 记录拆分决定、三条迁移路径与回滚步骤。
