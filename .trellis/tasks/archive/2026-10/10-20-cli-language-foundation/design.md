> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# F 技术设计

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：无。Trellis父子关系不自动调度这些依赖。

## 架构与数据流

1. Language/en-zh成对目录及调用隔离scope由common/i18n提供，async task-local与同步RAII范围相互独立且可嵌套；缺scope默认en，不用可变进程全局locale。
2. preferences从调用方已解析root读取cli-settings.json；缺失en、坏/不可读en+一次诊断。显式写在独立cli-settings.lock OS锁内复用write_file_atomic无audit接口，保留JSON未知字段；读取不锁/不建目录/不触DB。
3. schema驱动只读启动选择器读取args_os的home/真实路由/help；按metadata跳过0/1值、短参数组合/附值及--。正式Clap try-parse独占有效性判断；错误通道/退出码保留。精确行为与fallback见父design第3节及research/cli-startup-errors.md。
4. 全命令path+ArgId帮助投影翻译章节、说明、默认/合法值标签及validator约束；--help/help PATH跟随偏好，help --zh单次强制中文。root表格结构保留，不分析英文帮助字符串。
5. binary process adapter集中打印一次最终错误并返回ExitCode；保留run()->anyhow::Result与旧publicstructliteral。typed domain errors/code投影，CLI自有错误附typedcontext，不改共享Error Display。
6. lang TTY数字选单始终展示English/中文，空/取消/EOF不写，错误选择可重试退出；非TTY只显示当前值与显式设置方法，直接lang en/zh/zh-CN保存并以目标语言确认。

## 兼容与接口

父design是共享接口真源；新公开类型/必填字段、schema/wire/accounting变更不在范围。人读文本从语言目录投影，数据/代码/用户值保留；原错误source、machine分支与既有退出码不变。共享修改需遵循上述文件责任，不能回退其他child的工作。

## 关键依据

- 父目录 `research/surface-coverage.md` 是完整覆盖与归属清单。
- 精确源码事实与相关合同在本任务implement/check清单的research/spec中，执行前核对未漂移。
- 所有功能验证当前NOT_RUN；不把规划或context校验当成实现证据。

## 回滚与限制

最危险是先Cli::parse/初始化日志再选语言、影子argv清单漂移、public字段破坏、settings writer恢复串扰。回滚不得删usage/旧配置；语言偏好可正常切回en。

不做数据/凭据/历史日志语言迁移。native验收独立于headless；不建立尺寸/DPI/断点矩阵。
