> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# F：提供 CLI/TUI 共用的英语/简体中文语言基础与持久化设置，让帮助、参数错误和首条输出在日志/DB初始化前就选择正确语言。

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：无。Trellis父子关系不自动调度这些依赖。

## 目标与范围

提供 CLI/TUI 共用的英语/简体中文语言基础与持久化设置，让帮助、参数错误和首条输出在日志/DB初始化前就选择正确语言。

R1-R3、R8；父 AC1-AC3、AC11 的基础交付。C01-C02 及所有命令路径的帮助/Clap 错误。

## 文件责任

- `src/main.rs`
- `src/lib.rs`
- `src/common/mod.rs`
- `src/common/i18n/`
- `src/runtime/mod.rs`
- `src/runtime/preferences.rs`
- `src/commands/mod.rs`
- `src/commands/lang.rs`
- `src/commands/entrypoint.rs`
- `src/commands/cli_errors.rs`
- `src/commands/help.rs`
- `src/commands/clean.rs`
- `tests/cli/main.rs`
- `tests/cli/language.rs`
- `tests/architecture/main.rs`

clean仅负责settings/lock/residue的keep分类，剩余clean人读翻译交C；不实现console formatter、sync notices/表格或TUI面板。root/help/lang与shared接口归本子任务，其余子任务只能消费。

## 验收标准

- [x] F-AC1：临时root A/B中缺设置en、保存zh/切回en跨进程一致，zh-CN规范化；不读LANG/LC_*改默认。
- [x] F-AC2：lang在无init/无DB及worker lock占用时可工作；非TTY reader零次读取，取消/空/EOF不写。
- [x] F-AC3：未知code无写入；文件损坏读取不覆盖；注入write/replace失败旧配置完整且退出非零；串行writer不互相回滚设置。
- [x] F-AC4：--home在根/叶前后、附等号、中文空格路径、短flag附值、参数值为clean/help或伪home、--后伪flag等均不误路由；正式解析结果与原schema一致。
- [x] F-AC5：所有Clap路径/ArgId都有英中帮助和错误模板；未知参数/命令、缺值/required、冲突、enum/日期/时区/数字校验、未来kind fallback分别覆盖，help/version 0、parse 2。
- [x] F-AC6：首条bootstrap前语言正确；help/version/parse-error/lang只读不建home/logs/DB；clean继续忽略env-home。最终应用错误只打印一次且保留code/source。
- [x] F-AC7：两个交错async任务、嵌套sync scope和unwind不串语言；原AppContext/AppPaths/错误/renderer调用保持可编译。

## 范围外与风险

最危险是先Cli::parse/初始化日志再选语言、影子argv清单漂移、public字段破坏、settings writer恢复串扰。回滚不得删usage/旧配置；语言偏好可正常切回en。

不新增第三语言/热切换，不修改Web/desktop语言、token/query/sync算法，不操作真实usage或凭据，不安装/提交/推送/发布。没有阻塞产品决策；技术机制见design，执行前仍需用户明确解除不实现限制。
