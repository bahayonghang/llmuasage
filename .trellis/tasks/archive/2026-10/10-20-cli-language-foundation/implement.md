> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# F 执行与验证计划

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：无。Trellis父子关系不自动调度这些依赖。

## 启动条件

- 用户后续明确解除先不实现限制并批准最新规划。
- 前置child通过独立验收，接口可用；task.py validate通过，当前源码/工具链可用。
- 只启动本child，不把父任务当实现target；按Trellis implement/check协议加载本任务上下文。

## 顺序

- [x] 1. 先复核工具链与schema，写F-AC1-AC7行为红测并注册language.rs。
- [x] 2. 实现最小目录、scope、preferences/OS锁与lang。
- [x] 3. 实现schema启动选择、metadata help和structured parser/final error；保留兼容帮助。
- [x] 4. 接入无副作用入口及clean keep分类，跑定向/cli/architecture gates。
- [x] 5. 记录共享API与测试证据供S/C/T消费；更新基础语言合同，不启动其他任务。

## 验证命令

先运行新增/受影响过滤器，再运行以下领域gate；新测试必须注册在既有main.rs，autotests=false。

```text
cargo test --locked --all-features --test cli -- --test-threads=1
cargo test --locked --all-features --test architecture_dependencies -- --test-threads=1
python scripts/ci-rust.py
```

最终跨Rust/docs门禁由A整合执行just ci；禁止cargo update/重写锁文件。

## 完成标准

本child PRD全部AC有真实证据，owning文件diff符合父design，影响范围测试通过，未跑native项如实NOT_RUN；将交付/限制回写本任务证据并由父任务映射AC。子任务完成不代表父任务整体完成。

## 回滚与权限

最危险是先Cli::parse/初始化日志再选语言、影子argv清单漂移、public字段破坏、settings writer恢复串扰。回滚不得删usage/旧配置；语言偏好可正常切回en。

本轮只生成规划文件；未执行任何上面的产品步骤或测试。没有commit/push/install/rebuild授权。
