# CLI 启动、帮助与错误的研究结论

- 范围：本地源码与锁定版本官方 API；仅规划。
- 日期：2026-10-20。
- 决策：全 CLI 与 TUI 共享英语/简体中文设置，Web 不变；用户明确禁止本轮实现。

## 代码证据

- `src/lib.rs:99-117` 的先 help、后 parse、再 AppContext/logging 顺序，会让只放在 dispatch 的语言设置错过帮助和参数错误。
- `src/commands/mod.rs:45-47` 的 `home` 为 global；值可以出现在根或子命令后，PathBuf 不能先强制转 UTF-8。
- `src/commands/help.rs:112-129` 的现有匹配只处理几个精确 argv 形式；不等于完整命令树帮助。
- `src/commands/report_args.rs:20-35`、`:354-357` 的 custom validator 返回英文字符串；只翻译 Clap 的 error 标题不够。
- `src/main.rs:1-5` 返回 anyhow::Result，错误由 Rust Termination 输出；只改 tracing 仍会泄露英文错误上下文或重复打印。
- `src/api/error.rs:10-137` 为共享公共错误类型；不能使其 Display 依赖 CLI 语言，否则嵌入消费者、持久化错误和 API 会被连带改变。
- 当前命令定义的具名选项只有 0/1 个原始 argv 值；`src/commands/report_args.rs:128` 的 sections 是一个 argv 内的逗号分隔。这个约束必须由遍历 Clap schema 的测试持续核实，不能用手写参数清单维持。

## 已核实的官方接口

锁文件 `Cargo.lock:423-446` 使用 clap/clap_builder 4.6.6。官方 `Error` 支持读取种类、结构化上下文和输出通道/退出码，可据此自行格式化，避免分析英文错误句子。[Error API](https://docs.rs/clap_builder/4.6.6/clap_builder/error/struct.Error.html)

上下文可携带无效参数/值、合法值、建议与 usage 等分类信息；它和错误种类都是 non-exhaustive，因此未来版本必须有保留诊断能力的默认分支。[ContextKind API](https://docs.rs/clap_builder/4.6.6/clap_builder/error/enum.ContextKind.html)、[ErrorKind API](https://docs.rs/clap_builder/4.6.6/clap_builder/error/enum.ErrorKind.html)

Command 提供命令/参数元数据遍历和帮助定制能力。方案选用元数据驱动的只读启动选择器及帮助投影；正式验证仍由原 Clap schema 完成。`ignore_errors` 仅用于宽容解析，不假定其能恢复所有错误之后的 `--home`。[Command API](https://docs.rs/clap_builder/4.6.6/clap_builder/builder/struct.Command.html)

以上是 API 可用性与设计推导，未进行编译试验。在线文档未下载到仓库，也不引用第三方博客。

## 冻结的方案

1. 读取 `args_os`，从同一 `Cli::command()` 元数据构建小型只读选择器，仅选出 home、真实命令路径和 help 目标。它不接受/拒绝命令、不执行 value validator、不产生产品副作用。
2. 根据每个 Arg 的名称/别名、短参数、arity 和当前子命令跳过参数值；支持 `--home PATH`、`--home=PATH`、根/叶命令位置、短参数附值/组合和 `--`。不得把其他参数值中的 `clean`、`help`、`--home=...` 当成路由。
3. 当前 grammar 的 0/1 值约束由 schema 测试检查；新增其他 arity 时必须更新选择器测试与实现。不是另建一套命令选项定义。重复/缺值 home 的非法输入只使用第一个无歧义完整值或正常默认 root 来决定错误语言，正式 Clap 仍返回原错误；不会据此写任何文件。
4. 正式 Clap 使用 `try_get_matches_from` 与 `FromArgMatches`，不在语言确定前使用会自动打印退出的 parse/exit 方法。错误按 kind + context 投影；保留 use_stderr/exit_code。元数据/上下文不足时显示本地化通用说明、稳定 kind 与已有安全参数，不拼回英文框架段落。
5. 对 command path/ArgId 的帮助说明与 validator 约束建立英中目录；显式生成人读章节、默认值/可选值说明，避免原生 help 的固定英文标题漏译。参数拼写、枚举值、占位技术标记保持稳定。保留原根帮助的表格组织与 `help --zh` 单次显式中文兼容入口。
6. 共享领域错误继续语言中立；已知 LlmusageError/SyncRequestError 使用其 typed variant/code 渲染。命令内部自有错误附可 downcast 的诊断上下文，保留 source chain；不通过 contains/regex 抽英文语义。
7. 二进制入口负责一次人读最终错误和 ExitCode；复用的 `run()->anyhow::Result<()>` 不改签名。不要用进程 exit 绕过 renderer/scope guard 析构；不要同时打印新错误和旧 anyhow Debug。

## 审核重点

- `--home` 在非法参数前后、参数值恰为子命令、`--` 后的伪 flag、含空格/中文的 PathBuf 都必须测试。
- help/version/lang/参数错误在创建 AppContext 日志层前短路；clean 保留忽略 env home 的特例。
- 这是启动选择，不扩大 CLI 接受语法；Clap parser 的错误种类/退出码仍是唯一权威。
- 若首个实现切片证明 schema 的当前 arity/字段不足，先修订设计并保留红测，不能静默退回仅扫描 `argv.windows(2)` 或解析英文错误。

## 本轮限制

- 研究子代理启动失败：父 chat 的 thread-store 无 rollout；没有子代理实际工作或写文件，研究由主会话完成。
- 默认 shell 启动失败后使用显式 Windows PowerShell 完成只读工作。
- `cargo metadata --locked --offline` 被 mbx 拒绝，原因是其无法找到实际 Cargo。未修工具链、未下载依赖、未编译；实现前需恢复 Cargo 可用性。本轮规划文件校验不依赖 Cargo。

## Implementation-time grammar verification (2026-10-20)

F regression tests established that current Clap accepts `--home A daily --home B` and resolves B, while repeated home at the same depth is rejected. The read-only selector must mirror valid global propagation by command depth; it uses the first complete home only for ambiguous invalid same-depth repetitions. This refines the earlier planning assumption without changing the formal CLI grammar.
