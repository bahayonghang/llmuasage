# Implement：zcode 被动解析器

前置：父任务 `research/zcode-artifacts.md` 已定 token 语义；本文件是执行顺序清单。每步完成后跑对应验证，全部完成后跑全量 gate。

## 执行清单（有序）

1. [ ] **来源注册骨架**
   - `src/domain/models.rs`：`SourceKind::Zcode` + `as_str()="zcode"`。
   - `src/domain/source_descriptor.rs`：descriptor（Precise / LocalDatabase / parser+probe）。
   - `src/domain/platform_monitor.rs`：monitor（root `~/.zcode`，env `ZCODE_HOME`，pattern `cli/db/db.sqlite`，Registered）。
   - `src/store/schema.rs`：`expected_token_accounting_version`（对齐其他非 codex 来源 =2）。
   - 验证：`cargo test --lib registry` 与 `cargo test --lib platform_monitor`（分两次跑，cargo 测试过滤只收一个词）。

2. [ ] **DB 发现层**
   - `src/integrations/zcode.rs`：`resolve_db_path()`（`ZCODE_HOME` → `~/.zcode`；只读定位，不创建文件）。
   - 单测：默认路径、env 覆盖、缺失返回 None。

3. [ ] **游标层**
   - `src/store/cursor.rs`：`load_zcode_cursor/save_zcode_cursor`（`last_completed_at` + `last_processed_ids` 锚点，见 design §2；跟随 opencode cursor 的表用法）。
   - 验证：cursor 读写单测。

4. [ ] **解析器主体**（`src/parsers/zcode.rs`）
   - `sync_zcode`：缺失 DB → no_data 空跑；只读连接；schema 探测（动态投影变体）；分页 SELECT（`WHERE status='completed' AND (completed_at > ? OR (…))`，括号/水位见 design §2/2b）；行→`UsageEvent`（design §3 映射表，含 `input = in - cr - cc` 饱和减与 output 保留 reasoning）；**join `session` 取 `directory`/`path` 哈希做 project 归属（不落 `title`，join 不到 → project 留空）**；error/cancelled 计数走独立聚合查询；按页 `commit_shard`；页后存 cursor（仅全量模式，bounded 不推进）；锚点缺失重置重放。
   - `registry.rs` 注册 `ZcodeParser`。
   - 单测（design §6 清单）：status 过滤、cache-inclusive 数值（含 deepseek 例证 316/256/391/380/707 → input=60, output=391, reasoning=380, total=707）、旧 schema 降级（投影变体）、饱和、全零跳过。
   - 验证：`cargo test --lib parsers::zcode`。

5. [ ] **集成测试**（`tests/sync_regression.rs`）
   - `Fixture` 增 `ZCODE_HOME` env save/restore + `seed_zcode()`（rusqlite 建表插行）。
   - 用例（design §6）：sync-twice / append / **late-completing 不漏** / **error+cancelled 跳过且计数** / db-rebuild / missing-root / env-override / token-accounting 标记 / **bounded run 不推水位**。
   - 每源状态计数断言（文件头部 per-source status counts）更新。
   - 验证：`cargo test --test sync_regression zcode -- --test-threads=1`。

6. [ ] **状态与文档**
   - `source-status` 覆盖断言（`passive_no_data`/`passive_ready` + `precise`）。
   - `docs/agents/passive-source-candidates.md`：新增 zcode 行（Approved as parser-backed `zcode`；artifact family、token 语义、cursor、privacy 按实现填写）。
   - `README.md` / `README.zh-CN.md` / docs 对应页：来源列表与 source-status 说明。
   - 验证：`cargo run -- source-status`。

7. [ ] **全量 gate**
   - `cargo fmt --check` → `cargo clippy --all-targets --all-features -- -D warnings` → `cargo test --all-features -- --test-threads=1` → `just ci`。

## Review gates

- 步骤 4 完成后：对照 `docs/agents/passive-parser-onboarding.md` Required evidence 逐项核对（样本✓ 语义✓ cursor✓ quality✓ privacy✓）。
- 步骤 5 完成后：Required tests 逐项核对（fixture✓ sync-twice✓ cursor 回归✓ rebuild guard✓ status✓）。

## 回滚点

- 每步一个 commit；回滚 = revert 对应 commit。步骤 1-3 无行为影响；步骤 4 起可能产生 `source='zcode'` 行，回滚后用 `sync --rebuild --source zcode` 清理（确认 rebuild 支持后写入 README）。

## 验证命令速查

```bash
cargo test --lib parsers::zcode
cargo test --test sync_regression zcode -- --test-threads=1
cargo run -- sync && cargo run -- source-status
just ci
```
