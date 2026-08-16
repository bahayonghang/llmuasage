# Implement：antigravity CLI 被动解析器

前置：PRD R1 的 fixture 证据步骤是硬闸门——不通过则本任务停在登记状态（stop rule）。

## 执行清单（有序）

1. [ ] **fixture 证据采集（硬闸门）**
   - 已完成（2026-08-16）：schema 定稿、时间戳来源、109 行字段出现率、嵌套路径验证（research §2.1/§8 已勾）。
   - 剩余：跨 81 库抽查 `#4(顶层)/#5/#19/#21/#8` 出现率；空/中断会话样本；`seed_antigravity()` 合成 blob builder。
   - 结论写回父任务 `research/antigravity-artifacts.md` §8 勾选清单；quality 标签据此定稿。
   - 验证：research 文件更新 + 合成 blob 可被（即将编写的）解码器读出预期数值。

2. [ ] **P0 历史保护（先于任何 parser 注册代码合入）**
   - migration `m_0XX_preset_antigravity_token_accounting`：预置 `token_accounting_version:antigravity = expected`。
   - `--rebuild --source antigravity` 守卫：未归属历史行（`source_path_hash` 空/NULL）> 0 时拒绝。
   - 测试：真实旧 key 形状存量行 + 无界 sync/serve 不删除 + rebuild 拒绝/放行。
   - 验证：`cargo test --test sync_regression antigravity_upgrade` 与 `antigravity_rebuild_refused`（--test-threads=1）。

3. [ ] **wire 解码器 + 单测**（`src/parsers/antigravity.rs` 内私有模块或独立模块）
   - varint / len-delimited / 未知字段跳过 / 截断容错。
   - 字节级单测先行（TDD）：`#1` chatModel 嵌套（含**顶层 `#4` 干扰字段**）、`#4` usage、`#19` model、`#21` label、`#3` 校验、混合未知字段。
   - 验证：`cargo test --lib parsers::antigravity`。

4. [ ] **发现层 + 来源翻转**
   - `src/parsers/source_files.rs`：`list_antigravity_conversation_files()`（env 覆盖、`*.db` 过滤）。
   - descriptor `parser=true`、monitor `Registered`、roots 更新。
   - `src/commands/sync.rs`：收缩 parserless 特判；rebuild 走 §5a 守卫。
   - 新 ADR：`docs/adr/` 记录解除 blocked 决策 + 历史分代（证据链接 research）。
   - 验证：`cargo test --lib registry` 与 `cargo test --lib platform_monitor`（分两次跑）+ source_status 单测更新。

5. [ ] **解析器主体**
   - per-file 流程（design §4）：只读连接 → gen_metadata 行 → 解码 → model 回填 → UsageEvent（#9/#10 分离、total 含 reasoning）→ shard 提交 → FileCursor（fingerprint 全文件重解析策略）；bounded run 按 design §5b（不 reset、不推 cursor）。
   - registry 注册；`SourceSyncStats` 完整（files/skipped/bytes/issues）。
   - 单测：合成 DB → 事件断言、全零跳过、responseId 缺失兜底。
   - 验证：`cargo test --lib parsers::antigravity`。

6. [ ] **集成测试组**（design §6 全部用例 + Fixture env 扩展）
   - 验证：`cargo test --test sync_regression antigravity -- --test-threads=1`。

7. [ ] **文档与候选表**
   - `docs/agents/passive-source-candidates.md` Antigravity 行 → Approved（注明 CLI 工件族；IDE `.pb` 单列 planned；total 为通道求和含 reasoning）。
   - `README.md` / `README.zh-CN.md` / docs 页；ADR 链接。
   - 验证：`cargo run -- source-status`。

8. [ ] **真实数据抽查 + 全量 gate**
   - 本机 sync 后抽查 2-3 个 conversation 手工解码对账（#9+#10 都计入），结果记入 research。
   - `cargo fmt --check` → `cargo clippy --all-targets --all-features -- -D warnings` → `cargo test --all-features -- --test-threads=1` → `just ci`。

## Review gates

- 步骤 1 完成：对照 onboarding 文档 Required evidence（样本✓ 脱敏说明✓ 发现规则✓ cursor✓ 语义✓ quality✓ 隐私✓），不齐即停。
- 步骤 5 完成：Required tests 逐项核对。

## 回滚点

- 步骤 1-2 无行为影响。步骤 3 起改变 `source-status`/sync 输出（对外可见），回滚 = revert；步骤 4 起新增 `antigravity:` 前缀事件行，回滚后残留行无冲突风险（key 前缀独立），如需清理走 `sync --rebuild --source antigravity`。

## 验证命令速查

```bash
cargo test --lib parsers::antigravity
cargo test --test sync_regression antigravity -- --test-threads=1
cargo run -- sync && cargo run -- source-status
just ci
```
