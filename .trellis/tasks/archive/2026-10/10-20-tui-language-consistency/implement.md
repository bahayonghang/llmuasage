> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# T 执行与验证计划

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：planning；用户明确先不要实现。** 前置：10-20-cli-language-foundation。Trellis父子关系不自动调度这些依赖。

## 启动条件

- 用户后续明确解除先不实现限制并批准最新规划。
- 前置child通过独立验收，接口可用；task.py validate通过，当前源码/工具链可用。
- 只启动本child，不把父任务当实现target；按Trellis implement/check协议加载本任务上下文。

## 顺序

- [ ] 1. 读F scope与T01-T10，先写双语TestBackend/中文hit-test/async状态回归。
- [ ] 2. 接入启动locale快照并迁移nav/footer/help/source-picker和全部面板。
- [ ] 3. 补quota producer private hints与cache可选metadata，保持public序列化/网络/凭据规则，验证fresh/cache一致。
- [ ] 4. 迁移同步/加载/错误主线程呈现，验证generation/sort/selection/idle与恢复。
- [ ] 5. 跑tui/subscription/architecture和Rust gates；按用户决定修订English-only条款及subscription私有cache合同，交A做native验收。

## 验证命令

先运行新增/受影响过滤器，再运行以下领域gate；新测试必须注册在既有main.rs，autotests=false。

```text
cargo test --locked --all-features --test tui -- --test-threads=1
cargo test --locked --all-features --lib subscription -- --test-threads=1
cargo test --locked --all-features --test architecture_dependencies -- --test-threads=1
python scripts/ci-rust.py
```

最终跨Rust/docs门禁由A整合执行just ci；禁止cargo update/重写锁文件。

## 完成标准

本child PRD全部AC有真实证据，owning文件diff符合父design，影响范围测试通过，未跑native项如实NOT_RUN；将交付/限制回写本任务证据并由父任务映射AC。子任务完成不代表父任务整体完成。

## 回滚与权限

最危险是只翻译tab漏掉dialogs/worker字符串、中文点错导航、变更quota公开payload或从百分比造绝对余额。可选cache元数据必须兼容缺失/旧reader，不改凭据或触公网。

本轮只生成规划文件；未执行任何上面的产品步骤或测试。没有commit/push/install/rebuild授权。
