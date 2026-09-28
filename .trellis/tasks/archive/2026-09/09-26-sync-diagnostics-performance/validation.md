# 规划验证记录

日期：2026-09-26。验证范围为本轮研究与任务规划，不是产品实现验收。

## 已执行检查

| 检查 | 结果 |
| --- | --- |
| git -C ref/repo/ccusage pull --ff-only | 退出0，更新至732c7a6362f3d86a4992d2ad7071b6532161a396 |
| git -C ref/repo/tokscale pull --ff-only | 退出0，更新至1d9a9395418efc6952944b794097935d7d6fa1e8 |
| llmusage --version / 命令解析 | 1.3.0，用户Cargo bin下二进制；工作区1.4.0单独记录 |
| research/local-state-probe.py | 退出0，SQLite只读immutable，无WAL，数据库读前后size/mtime一致 |
| research/parse-probe.py | 退出0，Codex/Grok共16个保存样本；仅输出白名单元数据 |
| 四个task.json | 均planning；父子双向关联正确；当前任务仍为父任务 |
| 12份prd/design/implement文档 | 均存在、非空，无TBD/TODO占位 |
| 八份JSONL上下文 | 真实spec/research路径均存在，非空且无示例占位，所有单文件小于32768字节 |
| 父任务 task.py validate | 退出0，implement/check各8条，无警告 |
| diagnostics-contracts task.py validate | 退出0，各7条，无警告 |
| antigravity-replay-preflight task.py validate | 退出0，各6条，无警告 |
| sync-write-profiling task.py validate | 退出0，各6条，无警告 |
| 研究Python语法与JSON解析 | ast.parse / json.loads通过，不生成pycache |
| git diff --exit-code -- src tests Cargo.toml Cargo.lock README.md README.zh-CN.md docs | 无差异 |
| 两个参考仓库git status --short | 均无未提交变化 |
| 主仓库git status --short | 仅四个新任务目录未跟踪 |
| 最终报告链接、父子双向关联与占位检查 | 退出0；初次链接检查脚本的正则表达式错误已改正，复查通过 |

首轮task.py validate提示source-sync-contracts.md为39003字节，超过32768字节注入限制。已替换为有源文件SHA-256及原行号的两份相关摘录（23153/9220字节），然后对四个任务重新验证并通过。摘录是研究快照，实施时以当前权威规范为准。没有修改全局注入配置。

## 已收敛的规划边界

- 不把Serde JSON兼容等同于Rust公开API兼容。私有持久化DTO与内部结果保留公开struct和函数签名；实施增加semver门禁。
- reason与offset同时显示明确修订现有formatter规范；人类输出继续禁止path_hash。
- ParseIssues::total()继续仅计malformed+oversized，informational计数保留；省略提示另计四类总数。
- 采用仓库已有10%控制场景回归预算，至少7组交替A/B；20%目标改善尚待实测。
- 每个材料性问题均有所有者、证据、后续动作与子任务验收。父任务R1–R8映射到AC1–AC7，子任务D1–D5/A1–A5/P1–P5。

## 未执行

没有产品源码修改、真实sync/rebuild/reset、安装、Git提交或发布。没有运行Cargo测试、just ci、semver-checks或性能基准；这些是后续实施门禁。文件缺失原因、Grok incomplete深层原因、精确二进制提交、stderr混行竞态与writer分段耗时仍按未知项记录。

没有把既有测试定义、源码检查或只读探针成功表述为产品回归测试通过。

## 实施检查点（2026-09-26）

用户已明确要求按规划顺序实施。父任务进入 `in_progress`；当前子任务为 `09-26-sync-diagnostics-contracts`。后续两个子任务仍待前置验收。上述规划记录保留其当时范围，不能用作实施完成证明。

- 双语 README/CLI 文档及诊断规范已修改。更新后的两份 source-sync 摘录为 23167/15649 字节，包含当前规范哈希。
- 四个任务再次运行 `task.py validate`，均退出 0；父任务各 8 条、诊断子任务各 7 条、其余子任务各 6 条上下文。
- 实施代理用真实 indicatif 终端适配复现永久锁提示与原始 warning 混行：失败测试 `permanent_progress_lines_end_before_raw_warnings`，退出 1。修复与后续通过结果记录在子任务 `implementation-evidence.md`。
- 最终 Rust、文档、跨表面门禁和性能对照尚未完成。真实用户数据库未执行同步或修复。

### 兼容性验证

环境未提供 `cargo-semver-checks`。从该工具官方 GitHub release 获取 0.50.0 Windows x64 归档，SHA-256 与 release 元数据一致（`58854aaeaf84fee3266d1b4d9c45df585fe0478c7cabb4db4fac8f060a636864`），解压到系统临时目录。只为验证命令临时设置 PATH，未改全局 PATH 或产品安装。

- 正式命令 `cargo semver-checks --baseline-rev v1.2.0` 完成：196 项中 184 通过、11 失败、1 警告，另 58 跳过；要求 major version。该门禁未通过。见诊断子任务 `semver-check.log`。
- 补充命令 `cargo semver-checks --baseline-rev HEAD` 对比实施起点 `29bde59e84e1149b1d9215e020352586fb0a0ed7`：退出 0，223 项通过，31 跳过，无新增 API 破坏。见 `semver-head-comparison.log` 与 `.exit`。补充检查不替代 v1.2.0 正式门禁，不自动满足 D4。
- 首次补充命令使用完整提交 SHA，baseline rustdoc 构建遇到 LNK1104，无法打开 `libversion_check`；改用指向同一提交的短名称 HEAD 后完成。记录原失败，不将其表述为 API 比较失败。

### 独立审查中的修正

- 运行时中断使首轮检查代理未完成；重新派发 `diagnostics_verify` 接续审查，不把未运行的检查记为通过。
- 定位值单位已核对：JSONL 字节、ZCode 毫秒时间戳、OpenCode 私有 rowid。显示保留原始值，分别使用 `@`、`timestamp_ms=`、抑制 rowid。
- 首次完整 CLI 组出现一次错误日志为空的失败，定向重跑通过；原因未查明，保留首次失败记录，后续完整 CLI 组仍需验证。
- 审查发现新增 legacy fixture 漏填 NOT NULL 列及 ZCode 旧输出断言，已要求修正。
- 实际行级 protobuf / timestamp 错误不能全部转换为 source-only 故障；正在补私有 typed 错误边界和回归，D1–D5 尚未全部验收。
- `npm --prefix docs run docs:build` 退出 0，VitePress 报告 5.13 秒。

## 门禁扩展实施检查点

- 用户已授权先修复既有门禁，再继续预检与写入性能任务。范围与顺序写回父子 `implement.md`。
- 三个时区失败来自失效的固定偏移假设及 UTC fixture 遗漏显式时区。实现保留 Local/IANA 产品行为，更新注释/测试并增加 Chicago 23/25 小时边界；30 项定向测试通过，独立代码审查通过。八份相关用户文档与 dashboard-performance 契约已同步。
- 更新后 docs 构建退出 0，VitePress 报告 4.80 秒；证据为诊断子任务 `docs-expanded-gate.log/.exit`。
- 诊断审查增加 typed Antigravity 行错误及八类故障的历史保留矩阵。`thiserror` 的 source 字段推导导致构建失败，改名 source_kind 后 `cargo check --locked --all-features` 退出 0。
- 包装器 Cargo 构建的 sync 测试程序两次在执行测试前以 `0xc0000005` 退出，`--list` 也失败。使用现有原生 Cargo、相同参数重建后的八类矩阵通过。内部原因未查明；后续门禁仅在命令进程内优先使用原生 Cargo，不改全局配置。
- SemVer 研究记录 11 类失败及工具未报告的 fetch_all 返回类型变化。保留所有现有功能时，未找到小范围 1.x 兼容修复。已提交具体 2.0.0 / 1.x 兼容层选择，版本动作等待该选择；正式门禁尚未通过。
- source-sync 摘录已按当前规范重建，23167/17539 字节；诊断子任务 implement/check 各 9 条上下文，`task.py validate` 退出 0。
- 仍未执行真实用户 sync/rebuild/reset、安装、提交或发布。后续两个子任务尚未启动。

### 诊断独立审查交付

- 修复 native 产品与历史 owner 不同时的诊断重复归属。行级 malformed 仅归属 typed native source，受影响旧 owner 继续保留源级阻断。新增第九个矩阵场景先失败后通过。
- 最后归属修正前完整 sync 集成 143/143 通过；修正后九场景矩阵 1/1 及相关 lib 150/150 通过。格式、类型检查通过；最终完整门禁另行执行，不混用先后状态。
- 诊断子任务 `check-report.md` 与 `implementation-evidence.md` 已完成独立审查记录。根代理启动 `just ci`，日志及锁文件校验结果保存到 `ci-expanded*`。最初 PATH 前缀的 Windows 反斜杠被 JavaScript 字符串转义，前缀无效；环境日志证实本次实际仍调用 MBX Cargo。门禁运行继续按实际入口记录。使用 `C:/Users/lyh/.cargo/bin` 的独立探针已确认可以在命令级解析到原生 Cargo，不修改全局环境。

### 首次完整门禁结果

`ci-expanded.log`：`just ci` 退出 1。格式与 Clippy 通过；lib 904 通过、0 失败、12 忽略；API target 3/3 通过；architecture target 11 通过、1 失败。新增 store 诊断单测引用 `query::Dashboard`，触发 ARCH-007。四份受监控锁文件均未改变。后续 CLI、其余 integration、JS、desktop 和 docs 未被该命令执行。

检查代理已将 Dashboard 状态/隐私断言移至 query 层测试，store 层保留持久化及清除契约。ARCH-007 规则未改变；修正后 architecture target 12/12 通过。最终完整门禁将以单独日志记录，保留首次失败。

### 最终本地门禁（2026-09-28）

命令进程内将 `C:/Users/lyh/.cargo/bin` 置于 PATH 首位，并在运行前断言 Cargo 解析路径，再执行原始 `just ci`。结果退出 0。日志为诊断子任务 `ci-native.log`；结果、环境和初始锁文件哈希分别见 `ci-native-result.json`、`ci-native-environment.log`、`ci-native-locks-before.json`。

- lib：905 通过，0 失败，12 忽略。三个原时区失败均通过。
- 八个 integration target 共 246 通过：api 3、architecture 12、cli 34、query 9、remote 8、store 2、sync 143、tui 35。
- 格式、Clippy、rustdoc、CI gate self-test/contract、dashboard JS 66 项、desktop 前端测试/构建/Rust 测试、docs 构建全部通过。
- 根 Cargo、desktop Cargo、desktop npm、docs npm 四份锁文件均未改变。
- 源规范摘录最终为 23167/17924 字节。此前的失败和中间检查日志全部保留。

本地门禁不替代正式 SemVer。G1/G3 完成；G2 等待已发送的具体版本策略选择。版本仍为 1.4.0，v1.2.0 基线及失败策略保持。diagnostics 任务保留 in_progress；后续 preflight、profiling 尚未启动。没有提交、安装、发布或真实 usage 数据操作。
