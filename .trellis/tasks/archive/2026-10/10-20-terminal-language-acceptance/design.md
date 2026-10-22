> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# A 技术设计

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：10-20-cli-language-foundation, 10-20-cli-language-sync-console, 10-20-cli-language-command-output, 10-20-tui-language-consistency。Trellis父子关系不自动调度这些依赖。

## 架构与数据流

1. 对照父PRD AC1-AC16和surface C01-C22/T01-T10建立一份组合证据索引，每项指向owningchild测试/输出；不能把context validate标成产品PASS。
2. 双语文档说明默认en、lang、home优先级/clean例外、TUI启动快照、statusline语言cache、旧日志原文、JSON/SSH/Web边界，以及真实十列表格/风险提示示例。
3. 更新runtime-log/source-sync/report-cli/TUI规范：撤销已由用户覆盖的English-only，补privatequota presentation可选metadata的新旧兼容与不存secret约束；不要复制第二套AGENTS规则。
4. 先精准测试，再原样just ci。比较protocol固定fixture，明确timestamp/runid与optionalmetadata的允许差异，不能随意删除字段来制造通过。
5. temp home在真实普通终端分别跑英文/中文sync与dash/tui，核对菜单/警告/表格、九面板/弹窗、鼠标导航、排序/选择与退出恢复；使用fixture与mock，不拿真实usage做rebuild/不接公网额度。
6. 每项标PASS/FAIL/NOT_RUN并附命令/截图或terminal记录；native不可用保留NOT_RUN。完成后回父任务做需求/设计/实际diff一致性审查，未授权不commit/install/push。

## 兼容与接口

父design是共享接口真源；新公开类型/必填字段、schema/wire/accounting变更不在范围。人读文本从语言目录投影，数据/代码/用户值保留；原错误source、machine分支与既有退出码不变。共享修改需遵循上述文件责任，不能回退其他child的工作。

## 关键依据

- 父目录 `research/surface-coverage.md` 是完整覆盖与归属清单。
- 精确源码事实与相关合同在本任务implement/check清单的research/spec中，执行前核对未漂移。
- 所有功能验证当前NOT_RUN；不把规划或context校验当成实现证据。

## 回滚与限制

最危险是把自动检查/编译当native证据，或为了让CI绿改协议/lockfile。native不可用要如实保留NOT_RUN；发现产品缺陷回owner，不能打补丁绕过契约。

不做数据/凭据/历史日志语言迁移。native验收独立于headless；不建立尺寸/DPI/断点矩阵。
