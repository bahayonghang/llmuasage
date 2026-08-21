# Implement：Grok 用量口径与趋势来源表

前置：`prd.md` 与 `design.md` 已定语义。每步完成后跑对应验证。

## 执行清单（有序）

1. [ ] **契约与版本**
   - `src/store/schema.rs`：`SourceKind::Grok => 3`。
   - `token-accounting-contracts.md`：Grok 改为 `turn_completed.usage` 主路径 + 无 usage 回退；质量 precise；仍 unpriced。
   - `source-sync-contracts.md`：发现/两级枚举/会话重放不变；补充 usage 主路径。
   - 验证：文档改动无代码行为；下一步解析器测试会锁版本。

2. [ ] **解析器主路径**（`src/parsers/grok.rs`）
   - 抽取 `parse_turn_usage`：扫描 `turn_completed.usage` → `UsageEvent`。
   - 会话级：有 usage 则只用 usage；否则调用现有 `parse_updates_file` + signals。
   - 描述符与 monitor 改为 `UsageQuality::Precise`。
   - 单测覆盖 design §6 解析器项（AC1–AC5、AC8 的 unpriced 由集成测）。
   - 验证：`cargo test --lib parsers::grok -- --test-threads=1`。

3. [ ] **集成回归**（`tests/sync_regression.rs`）
   - 扩展 grok fixture，写入脱敏 `turn_completed.usage` 行。
   - 用例：precise 通道、多段求和、usage 抑制 signals、无 usage 回退、sync-twice、追加 usage 重放、GROK_HOME、missing sidecar 仍保护。
   - 如有现成 accounting-version 测试，为 grok 增加 marker 3。
   - 验证：`cargo test --test sync_regression grok -- --test-threads=1`。

4. [ ] **趋势来源表**
   - `src/web/assets/render/trends.js`：按 `PANEL_LIMITS.sources` 截断并追加「其他」。
   - `copy.js` 增加 zh/en 文案。
   - `src/web/mod.rs`：禁止 `.slice(0, 2)`，锁定 PANEL_LIMITS 与其他行。
   - 验证：`node --check src/web/assets/render/trends.js` 与 `cargo test --lib web -- --test-threads=1` 中相关测试。

5. [ ] **文档**
   - `docs/agents/passive-source-candidates.md`
   - `README.md` / `README.zh-CN.md`
   - `docs/guide/first-sync.md` 与 `docs/zh/guide/first-sync.md`
   - 如 CLI/dashboard 页仍写 total_only，一并改正。

6. [ ] **质量闸门**
   - `python scripts/ci-rust.py` 或聚焦测试通过后 `just ci`。

## Review gates

- 步骤 2：对照 `docs/agents/passive-parser-onboarding.md` 的 token 语义与隐私边界。
- 步骤 3：Required tests（fixture、sync-twice、rebuild guard、status）。
- 步骤 4：浏览器或 `just serve` 打开用量趋势 `7d`，确认来源表不再只有两行；在已重放的本地库上 grok 应高于 codex。若本机会话仍全是旧 marker 且尚未 sync，记录待用户执行/自动 repair，不以未重放库否定 AC。

## 回滚点

- 步骤 1–2 可单独 revert 解析器与 version。
- 若已对用户库写出 precise grok 行，回滚代码后需要 `--rebuild --source grok` 才能回到旧口径。
- 步骤 4 只影响前端截断，可独立 revert。

## 验证命令

```bash
cargo test --lib parsers::grok -- --test-threads=1
cargo test --test sync_regression grok -- --test-threads=1
cargo test --lib web trend -- --test-threads=1
node --check src/web/assets/render/trends.js
just ci
```
