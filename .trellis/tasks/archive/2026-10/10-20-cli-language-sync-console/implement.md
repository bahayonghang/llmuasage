> 2026-10-20 执行授权更新：用户已明确要求开始实施本父任务和子任务。下文历史性的“先不要实现 / planning only”限制已被本次授权取代；既有范围、依赖、验收与禁止提交/推送/安装/真实重建/发布的边界继续有效。

# S 执行与验证计划

父任务：`.trellis/tasks/archive/2026-10/10-20-cli-language-sync-output`。需求/全局决策/兼容边界以父PRD和design为准。

**状态：已实施并通过完整 Rust 门禁；独立验收见 `research/check-evidence.md`。** 前置：10-20-cli-language-foundation。Trellis父子关系不自动调度这些依赖。

## 启动条件

- 用户后续明确解除先不实现限制并批准最新规划。
- 前置child通过独立验收，接口可用；task.py validate通过，当前源码/工具链可用。
- 只启动本child，不把父任务当实现target；按Trellis implement/check协议加载本任务上下文。

## 顺序

- [x] 1. 读取F接口和父notice表，先写日志/重建/表格回归并覆盖曾经串行的stderr。
- [x] 2. 在中立engine边界提供private notices，CLI获取同轮事实并迁移prompt I/O。
- [x] 3. 加CLI console formatter/消息目录与发射点id，不动file writer或level。
- [x] 4. 迁移line/bar和summary到目录/Unicode边框/来源诊断。
- [x] 5. 跑sync/remote/architecture及logging/store定向门禁，记录数据等价证据后交C消费。

## 验证命令

先运行新增/受影响过滤器，再运行以下领域gate；新测试必须注册在既有main.rs，autotests=false。

```text
cargo test --locked --all-features --test sync -- --test-threads=1
cargo test --locked --all-features --test remote -- --test-threads=1
cargo test --locked --all-features --test architecture_dependencies -- --test-threads=1
python scripts/ci-rust.py
```

最终跨Rust/docs门禁由A整合执行just ci；禁止cargo update/重写锁文件。

## 完成标准

本child PRD全部AC有真实证据，owning文件diff符合父design，影响范围测试通过，未跑native项如实NOT_RUN；将交付/限制回写本任务证据并由父任务映射AC。子任务完成不代表父任务整体完成。

## 回滚与权限

最危险是把翻译写回持久化状态/协议、回调改变事务、缩窄数值或误改恢复确认。回滚只回展示与私有feedback，不真实重建或降级数据版本。

本轮已按新授权实施并运行上述 Rust 检查；未提交、推送、安装、发布或操作真实用量。历史 planning-only 边界已被文件顶部的实施授权更新取代。

## 实施进度（2026-10-20）

产品接口与定向测试已完成；173 项领域测试、74 项定向单元测试通过。完整 Rust gate 首次运行在两项 Clippy 机械告警处停止，交独立 checker 修复后继续验证。详见 `research/evidence.md`；未把 pending full gate 标为完成。

## 最终 Rust 验证（2026-10-20）

`research/ci-rust-after-review.log` exit 0：fmt、Clippy -D warnings、1253 项测试通过 / 19 项既有 ignored、rustdoc -D warnings 全部通过。最终源码哈希与 `research/final-source-snapshot.json` 一致。独立 verdict 由 checker 维护；native 与跨面 just ci 由 A 继续。原 Web Local/UTC 日期策略差异仍为 OUT_OF_SCOPE，见证据。未归档或切换 active task。
