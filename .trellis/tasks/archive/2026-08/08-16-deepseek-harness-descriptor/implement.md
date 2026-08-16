# Implement：deepseek-harness (dsh) 被动解析器

## 执行清单（有序）

1. [ ] **zstd 依赖评审（gate，不通过则降级为仅登记阶段）**
   - 对照 `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md` 评审 `zstd` crate（首选，tokscale `sessions/dsh.rs` 同款且其 Windows CI 已验证；流式 `Decoder` 支持撕裂尾帧前缀恢复）；备选 `ruzstd`（需先验证流式撕裂帧行为）。产出结论写入父任务 research §7。
   - **真实变更 manifest 验证**（`--dry-run` 不改依赖图，不算）：`cargo add zstd` → `cargo build` → `cargo clippy --all-targets --all-features -- -D warnings` → `python scripts/ci-rust.py` → **隔离 target 的 MSRV 证明**（`CARGO_TARGET_DIR=<isolated> cargo +<rust-version> check --locked --all-features`，版本以 `Cargo.toml` 为准）。

2. [ ] **interrupted 真实样本采集（onboarding 证据 gate，解析器阶段前置）**
   - 本机 15 文件扫描无撕裂尾帧（2026-08-16）；真实运行一次 dsh 会话并在写入中途中断（kill），采集真实撕裂帧 `session.jsonl.zstd`，验证"完整帧 + 撕裂尾帧"结构，脱敏说明与路径写回父任务 research §4。

3. [ ] **阶段一：登记（独立可交付 commit）**
   - platform monitor `deepseek_harness`（roots `~/.dsh` + 旧根 `~/.deepseek` 探测、`parser_status = Planned`、artifact patterns）。
   - `docs/agents/passive-source-candidates.md` 新增行（Decision: Approved pending zstd dependency review）。
   - 注册表/monitor 不变量测试更新。
   - 验证：`cargo test --lib registry` 与 `cargo test --lib platform_monitor`（分两次跑）；`cargo run -- source-status` 显示根探测与 Planned。

4. [ ] **发现层**
   - `list_dsh_session_files()`：`$DSH_HOME | ~/.dsh`，`sessions/` 下任意深度、文件名精确 `session.jsonl.zstd` 或 `session.jsonl`（tokscale `dsh-session-log` 契约）；env 语义对齐 kimi/pi。
   - 验证：单测（默认路径、env 覆盖、双拼写、任意深度、同名排除其它 jsonl/zstd、缺失根空列表）。

5. [ ] **解析器主体**（`src/parsers/dsh.rs`）
   - 帧魔数分派 + **流式管道**（文件 BufReader → zstd 流式 Decoder → 带 4 MiB 单记录上限/partial-tail 的行分割，不物化整缓冲）；窄 serde 结构体行解析；`UsageEvent` 构造（design §2 表：**复合 event_key + seedLength 守卫 + source 优先模型归属 + 会话家族映射**）；`registry.rs` 注册 `DeepseekHarnessParser` + `SourceKind::DeepseekHarness` 同批。
   - 单测：双带 usage 去重、空会话、全零跳过、time 缺失跳过、version≠0 计数、帧魔数分派双向、撕裂尾帧前缀恢复、超大记录丢弃并计 issue、fork 双防线（seedLength 跳过 + 跨文件键折叠 + 占位符 id 靠 time/token 分离）、归一化数值（本机 7619/19840/171；官方快照 2885/25/23 → total 2910）。
   - 验证：`cargo test --lib parsers::dsh`。

6. [ ] **集成测试**（`tests/sync_regression.rs`）
   - `seed_dsh()`（合成 .jsonl + 压缩多帧 .zstd 含截断尾帧 + fork 父子双文件）+ Fixture env 扩展。
   - 用例：sync-twice / append / rewrite / 删除保历史 / missing root（含旧根探测）/ `DSH_HOME` 覆盖 / token-accounting 标记 / fork 不双计 / **所有权重放**（owner 重写移除共享事件、duplicate 未变化 → 家族重放后事件不消失）/ **bounded run 不推 cursor 不 reset**。
   - 验证：`cargo test --test sync_regression dsh -- --test-threads=1`。

7. [ ] **文档 + 真实数据抽查**
   - 候选表 Decision → Approved as parser-backed；README 双语 + docs；ADR（新来源 + zstd 依赖决策）。
   - 本机 sync 后抽查 2 个会话手工对账（对照 DSH 自带 meter 口径 in+cache+out），结果记入 research。
   - 验证：`cargo run -- sync && cargo run -- source-status`。

8. [ ] **全量 gate**
   - `cargo fmt --check` → `cargo clippy --all-targets --all-features -- -D warnings` → `cargo test --all-features -- --test-threads=1` → `just ci`。

## Review gates

- 步骤 1 完成：依赖结论明确（zstd 或降级），manifest/CI/MSRV 证据落盘。
- 步骤 2 完成：onboarding 三类样本（normal/empty/interrupted）齐备。
- 步骤 5 完成：对照 `docs/agents/passive-parser-onboarding.md` Required evidence 逐项核对。
- 步骤 6 完成：Required tests 逐项核对（含 fork、撕裂帧、所有权、bounded 四类边界）。

## 回滚点

- 阶段一（步骤 2）独立 commit，可单独保留（登记价值不受解析器影响）。
- 步骤 3-4 回滚 = revert；新增 `deepseek_harness:` 前缀行回滚后无冲突，可 `sync --rebuild --source deepseek_harness` 清理。

## 验证命令速查

```bash
cargo test --lib parsers::dsh
cargo test --test sync_regression dsh -- --test-threads=1
cargo run -- sync && cargo run -- source-status
just ci
```
