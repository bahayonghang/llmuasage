> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# 父任务执行计划与整合门禁

状态：只规划。用户明确说「先不要实现」，当前父子任务全部 planning；本文件不是执行授权。后续只有用户明确解除限制并批准最新规划，才启动第一个拥有产品改动的子任务。

## 1. 子任务与顺序

| 顺序 | 子任务目录 | 前置交付 | 独立验收结果 |
| --- | --- | --- | --- |
| F | `../10-20-cli-language-foundation` | 无；先确认 Cargo 可用 | shared locale/settings、lang、早期 root/help/parse error、基础目录与 scope 隔离 |
| S | `../10-20-cli-language-sync-console` | F 的 Language、scope 与诊断接口 | CLI 日志投影、typed sync notices、恢复确认、十列线框表与诊断 |
| C | `../10-20-cli-language-command-output` | F；logs 人读重放依赖 S | 冻结命令清单 C03-C21 的其余运行期人读输出/错误/交互 |
| T | `../10-20-tui-language-consistency` | F；复用 S 的中立诊断而不依赖 commands | T01-T10，九面板/弹窗/同步/导航命中一致 |
| A | `../10-20-terminal-language-acceptance` | F/S/C/T 都完成并通过局部门禁 | 双语文档/规范、完整 CI、CLI/TUI 真实终端证据与跨协议回归 |

依赖写入各 child PRD/implement 与 task meta；Trellis parent/child 自身不强制依赖，不依赖自动调度。即使具备并行代理能力，本计划默认串行，避免文件所有权冲突。

## 2. 启动前检查（未来执行时）

- [x] 2026-10-20 用户已明确要求开始实施已有父任务与子任务。
- [ ] 阅读 parent PRD/design、surface-coverage 和当前 child 三文档；核对相关源文件未漂移，不能把旧 file:line 当实时证明。
- [x] Cargo 预检：本次 cargo/rustc 1.97.0 与 `cargo metadata --locked --offline --no-deps --format-version 1` 已通过；未修改工具链配置或锁文件。
- [x] 六个任务的 implement/check.jsonl 均通过 task.py validate；各清单已有真实 spec/research 条目。
- [ ] 仅启动拥有本阶段文件的 child；parent 不作为实现入口。实现/check 使用当前 Trellis dispatch 协议和 native context 注入，保留模型继承，不重复规划已确认范围。
- [ ] 测试全部 temp home/fixture，禁用真实价表/额度/SSH/git/Cargo install 等外部副作用。

## 3. 文件所有权与重叠处理

- F 创建 common/i18n 核心、runtime preferences、commands lang/entrypoint/cli_errors，修改 main/lib/root Clap/help/clean 的设置分类。实施复核另授权 F 仅修正 report_args.rs 的 parse_timezone_value Unicode 切片 panic 并加参数错误回归；该文件其余报告行为/输出仍归 C。其接口稳定后其他 child 才消费。
- S 独占 runtime/logging、sync_*、sync engine 的 private feedback 与 parser recovery 迁移；可在 entrypoint 原初始化调用处接入 CLI 专用 logger，保持 Web/desktop 默认 logger 不变。为既有 tracing callsites 追加中立 event_code 时仅改必要字段，不能顺手改业务逻辑。
- C 独占其余 commands、CLI report_table 与 command-output 目录；不再修改 F 的启动机制。S 触及过的命令文件由 C 在串行阶段接续，只迁移 remaining human text。
- T 独占 tui 除 report_table 外全部交互文案/同线程 scope；C 已完成的 report_table 不由 T 重构。必要的 shared code/diagnostic helper 先明确边界再修改，不让 TUI import commands。
- A 独占 README/docs 与规范整合、跨表面测试组织。发现 bug 返回 owning child 修正，不混成文档提交。
- 规范可在对应 child 完成时更新其自有合同；A 审查组合一致性，不能覆盖别的并行工作。当前规划不改任何 spec。

## 4. 定向验证

未来先跑新增/受影响过滤器，再跑各 domain target：

```text
cargo test --locked --all-features --test cli -- --test-threads=1
cargo test --locked --all-features --test sync -- --test-threads=1
cargo test --locked --all-features --test tui -- --test-threads=1
cargo test --locked --all-features --test architecture_dependencies -- --test-threads=1
cargo test --locked --all-features --test remote -- --test-threads=1
python scripts/ci-rust.py
```

存在 typed notice callback 或 settings/文件行为变更时，补跑已有 store/engine/atomic 单测；不新增第九个 integration target。新集成文件必须从相应 main.rs 显式 mod 引入。

最终 Rust+docs 跨 surface gate 是：

```text
just ci
```

按 AGENTS.md 保持原 recipe，禁止 cargo update 或写锁文件。dedicated semver 如需检查用 `cargo semver-checks --baseline-rev v1.2.0`（不加 --locked），不擅自升版本或改基线。

## 5. 父级验收矩阵

验收 ID 与需求映射以 parent PRD 为唯一真源；各 child AC 引用，不复制改义。

- [ ] 默认 en → lang zh → 新 CLI/TUI 中文 → lang en → 新 CLI/TUI 英文；temp profile A/B 不串。
- [ ] 从实际 Clap metadata 遍历所有命令/ArgId，帮助/错误模板有英中配对；28 existing variants + lang 与覆盖清单一致；包括 hidden tui/handshake 和20 focused叶路径。
- [ ] C01-C22、T01-T10 各有 owning evidence，原生 OS/第三方内容和历史日志例外明确，不能以未知 fallback 掩盖新自有文案漏译。
- [ ] sync 英中恢复输入/TTY/flags 分支不变；同 fixture 数据、cursor、accounting marker 与无 UI callback 路径等价（只排除原本动态时间/run-id）。
- [ ] 十列 summary 数值/单位、合计、缺失来源与诊断隐私不变；accounting 信息不伪装失败或不同记录数。
- [ ] JSON/NDJSON/remote/diagnostics/导出/订阅缓存保持契约；logs 新 optional id 与 timestamp 等允许差异逐项说明。
- [ ] TUI 九面板/所有dialog/loading/empty/error在两语言 TestBackend 通过，nav render/hit-test 共享 display width，排序/选中/热键一致。
- [ ] idle tick、可见行窗口、quota TTL与r/R/m、JobRegistry取消/退出、主题/NoColor等既有测试不退化。
- [ ] 双语 README/docs、CLI/TUI/log/source-sync 合同一致；Web/desktopassets 无非授权改动。
- [ ] 真实终端普通窗口英中 CLI sync 与 TUI smoke 分开给 PASS/FAIL/NOT_RUN；TestBackend 不能替代 native acceptance。不建立尺寸/DPI/断点矩阵。

## 6. 完成与回滚规则

- 每个 child 的功能、测试、code review、source anchor 漂移与例外写入其 evidence；父任务维护一份组合证据索引，不重复抄 CI 日志。
- 子任务只有独立 AC 和门禁都满足才能完成。父任务只有全部需求/跨子任务 AC 完成、无未归属覆盖项后才能结束；不能因为规划文件齐全而标功能 complete。
- 每阶段回滚只回本 child 的必要产品改动，不删除用户 settings/usage，不做真实 rebuild，不修改第三方凭据。lang en 是正常切换，不是清库。
- 本轮无实现、测试执行、提交/推送、安装/发布；全部功能状态 NOT_RUN。后续 commit/finish 不得超出用户当时授权。

实施接线说明：S/C/T 可在 common/i18n/mod.rs 中仅登记各自拥有的目录模块，locale 核心语义仍归 F；这属于目录接线而非语言上下文重构。
