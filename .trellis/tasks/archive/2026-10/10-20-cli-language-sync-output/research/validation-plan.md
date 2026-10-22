# CLI/TUI 语言验收矩阵

规划日期：2026-10-20。作用域已确认为完整CLI + TUI；Web独立。父PRD的AC1-AC16是唯一需求验收真源，下表定义验证机制。所有产品测试/真实终端当前NOT_RUN，结构校验另列。

## 1. 从需求到证据

| 父AC | 测试机制 | 主要测试落点 | Owner |
| --- | --- | --- | --- |
| AC1 | temp root A/B，无设置en→lang zh→新CLI/dash/tui→lang en；优先级/clean例外分别断言 | 新tests/cli/language.rs，tests/tui/shell.rs，preferences单测 | F/T/A |
| AC2 | 注入menu reader/writer；三TTY真/任一假、空/取消/EOF、无init/占用DBworker lock；并发设置writer/坏文件/替换失败 | preferences/lang单测、CLI subprocess | F |
| AC3 | 从实际Clap metadata遍历命令与ArgId；英中目录覆盖；所有当前ErrorKind、各validator、root/leaf/help/hidden和未知kind安全fallback | commands help/cli_errors/entrypoint单测、tests/cli/language.rs | F |
| AC4 | C01-C22逐项success/empty/reject/failure；自动展开5source×4period；结果匹配稳定数据而非笼统无汉字扫描 | tests/cli/{local_flow,operations,reports,language}.rs、tests/remote | C/S/A |
| AC5 | 两语言k/K/r/R、空/EOF/readfailure、两次非法输入；missing/out_of_scope/unreadable/discovery与三流TTY/flags组合 | sync_recovery单测、tests/sync既有recovery/coverage测试 | S |
| AC6 | 同一typed事件经两语言console投影；各level/filter；新id日志重放；同一file event原message/字段不变；oldunknown保留标记raw | runtime/logging单测、tests/sync/runtime/progress_io.rs、logs单测 | S/C |
| AC7 | 纯summary渲染fixture按cell与display width断言十列/边框/数值/合计、缺失源/大整数；去ANSI后仍成表 | sync_summary单测、tests/sync/progress_io.rs | S |
| AC8 | source标题顺序、fault/info分组、样本8/14与6 omitted、隐私消毒/offset分支；同一记录多提示不假称去重记录数 | sync_summary/domain现有单测、source-status与TUI覆盖层 | S/C/T |
| AC9 | en/zh固定fixture深比较report/diagnostics/doctor/logs/catalog、sync/shards/handshake；每行NDJSON可parse | cli/sync/remote targets | S/C/A |
| AC10 | 显式注入sink+visible progress reproducer，early return/cancel/error后恢复，stdout/stderr无串行；NoColor/nonTTY/forced-line | sync_progress单测与subprocess | S |
| AC11 | 缺失root情况下help/version/lang读/parseerror/clean零目录创建；--home位置/附值/--/误路由值与OsString；async/sync scope交错/嵌套/unwind | language/preferences/entrypoint/architecture测试 | F/C |
| AC12 | bilingual docs build；实际命令和旧English-only/locale/机器边界逐项核对 | docs:build、source-spec审阅 | A/T |
| AC13 | T01-T10全表：两语言九面板各data/loading/empty/error、footer、dialogs、details、sync/quota状态 | tests/tui/及panel单测，正常窗口native另记 | T |
| AC14 | en/zh nav cell区间点击与同一Panel一致；稳定sort/selection/keys；既有idle-tick/visible-row/generation/JobRegistry/NoColor回归 | nav_bar/mod/data_loader/sync_control单测，tests/tui | T |
| AC15 | 单行statusline en/zh隔离cache及旧latest不串；update fake provider/executor验证默认Y/EOF/二次ref；remote/clean/uninstall原删除集 | tests/cli operations、update/clean单测、tests/remote | C |
| AC16 | 精准target→sharedRust→just ci→独立native smoke，组合证据不夸大未跑项 | 各child evidence与parent索引 | A |

## 2. 额度显示元数据的专项验证

UsageFetchDiagnostic现状没有code，remaining_label可能嵌有绝对余额/英文left，不能假设只换UI表头即可。T的privatehints/cache可选metadata必须验证：

- 新鲜fetch与freshcache返回原publicreport的相同数字/字段，TUI英中显示相同含义；不从百分比重建绝对值。
- 新CacheDocument的可选presentation只含中立安全事实；无凭据/token/locale/HTTP正文。旧cache无字段仍加载，旧reader忽略新字段，TTL/cache_hit/mtime语义与请求计数不变。
- hints绑定report/generation指纹；错配/缺失走本地化raw-detail说明，不能污染另一账户或回放旧请求。
- publicfetch_all、UsageFetchOutcome/Report/Metric/Diagnostic字段/序列化兼容，desktop不新增语言输入；当前用户批准的是TUI展示，不是配额抓取重写。
- mock endpoint和tempcredential；凭据前后bytes一致，r/R/m、邮箱默认隐藏与取消行为不变。

## 3. 证据判定规则

- 不用「英语stdout没有汉字」做完整性判断：用户label、路径/文件名、模型可能合法包含中文。验证app-owned目录/模板与输出结构。
- 不用「所有文本完全相同」保护机器兼容：日志timestamp/run-id、同轮时间/诊断观察时间本来动态；规范化必须逐字段列出。optional event_code与privatecachepresentation是明确允许的新增metadata，原report/协议值不变。
- 字典条目成对是结构条件，不能取代行为测试；header快照也不能取代count/TTY/stdin/数据保护断言。
- TestBackend与编译通过只证明各自范围；真实普通窗口CLI/TUI smoke单独留证。不可用则NOT_RUN，不能标PASS或默默豁免。
- 不主动增加尺寸/DPI/断点矩阵；中文导航命中与表格cell width是语言功能本身，应测试。

## 4. 计划命令（本轮未执行）

新集成文件必须mod接入既有8target，autotests=false。先跑受影响过滤器，再跑：

```text
cargo test --locked --all-features --test cli -- --test-threads=1
cargo test --locked --all-features --test sync -- --test-threads=1
cargo test --locked --all-features --test tui -- --test-threads=1
cargo test --locked --all-features --test remote -- --test-threads=1
cargo test --locked --all-features --test architecture_dependencies -- --test-threads=1
cargo test --locked --all-features --lib subscription -- --test-threads=1
python scripts/ci-rust.py
npm --prefix docs run docs:build
just ci
```

runtime logging/store/atomic定向单测按修改补充。最终跨Rust+docs用原just ci，不cargo update、不改锁文件。dedicated semver如需运行是cargo semver-checks --baseline-rev v1.2.0，不加--locked。

## 5. 当前已知限制与本轮校验

- 默认shell无法启动后改用显式WindowsPowerShell；文件操作均在六个task目录内。
- 研究子代理启动失败（父thread-store无rollout），没有独立代理研究/审阅证据；主会话完成源码和官方API核实。
- Cargo metadata只读尝试失败：mbx找不到真实Cargo；未修工具链、未下载依赖。未来实现前先恢复正常工具链，本轮不宣称API编译验证。
- 本轮校验目标：父子链接/依赖无环、六任务均planning/未授权实现、每任务prd/design/implement齐全、context路径与大小有效、源码锚点和规划引用有效、无占位符、过期范围问题或未归属AC、只改任务文件。
- 功能、Cargo测试、just ci、nativeCLI/TUI全部NOT_RUN；具体planning校验结果记录在planning-audit.md。
