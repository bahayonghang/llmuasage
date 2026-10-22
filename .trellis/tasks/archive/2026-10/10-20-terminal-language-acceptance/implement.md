> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# A 执行与验证计划

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：10-20-cli-language-foundation, 10-20-cli-language-sync-console, 10-20-cli-language-command-output, 10-20-tui-language-consistency。Trellis父子关系不自动调度这些依赖。

## 启动条件

- 用户后续明确解除先不实现限制并批准最新规划。
- 前置child通过独立验收，接口可用；task.py validate通过，当前源码/工具链可用。
- 只启动本child，不把父任务当实现target；按Trellis implement/check协议加载本任务上下文。

## 顺序

- [ ] 1. 核对所有前置child独立AC与证据，补跨命令/CLI-TUI一致性测试，不重复已经足够的局部测试。
- [ ] 2. 按实际CLI/TUI行为更新中英文档、例子和规范，TUI说明使用现有CLI reference章节。
- [ ] 3. 跑精准检查和完整just ci，保持原命令与lockfile；失败回归owner修复后再复测必要部分。
- [ ] 4. 做隔离fixture真实终端smoke并逐项记录PASS/FAIL/NOT_RUN。
- [ ] 5. 对照实际diff/父AC做最终整合审阅，更新parent evidence与任务状态；继续遵守当时用户权限。

## 验证命令

先运行新增/受影响过滤器，再运行以下领域gate；新测试必须注册在既有main.rs，autotests=false。

```text
cargo test --locked --all-features --test cli -- --test-threads=1
cargo test --locked --all-features --test tui -- --test-threads=1
npm --prefix docs run docs:build
just ci
```

最终跨Rust/docs门禁由A整合执行just ci；禁止cargo update/重写锁文件。

## 完成标准

本child PRD全部AC有真实证据，owning文件diff符合父design，影响范围测试通过，未跑native项如实NOT_RUN；将交付/限制回写本任务证据并由父任务映射AC。子任务完成不代表父任务整体完成。

## 回滚与权限

最危险是把自动检查/编译当native证据，或为了让CI绿改协议/lockfile。native不可用要如实保留NOT_RUN；发现产品缺陷回owner，不能打补丁绕过契约。

本轮只生成规划文件；未执行任何上面的产品步骤或测试。没有commit/push/install/rebuild授权。
