# 执行计划：Pi 事件的 provider 与项目维度

## 前置

- 依赖 `08-23-omp-source-split` 已归档。
- 读 `.trellis/spec/llmusage/backend/source-sync-contracts.md`、
  `token-accounting-contracts.md`、`write-fencing-contracts.md`。
- 读 `docs/adr/0010-provider-label-dimension.md` 与父任务 `design.md` D2（回填口径）。
- 记录基线：`deepseek_harness` 836 条事件 `provider_label` 全空；
  `source='omp'` 的 `provider_label` 与 `project_hash` 全空。
- 跑一次真源扫描存档：
  `python .trellis/tasks/08-23-pi-omp-usage-accounting/research/scan_pi_source.py`

## 步骤

1. [x] 写入端仅填空：改 `src/store/sync_writer.rs:585` 的填充条件。
       单测覆盖「非空标签不被覆盖」「空标签仍被填充」（AC2.3）。
2. [x] 回归确认 `deepseek_harness`：临时 home 覆盖 CCR 时间线存在时 dsh 标签保留、
       `codex` / `claude` 仍吃 CCR 标签（AC2.4 tempfile）。本机 live-home rebuild
       留给 supervisor。
3. [x] `src/parsers/pi.rs` 事件构造点写入 `provider_label`：读 `message.provider`、
       `trim`、缺失或空则空串。单测覆盖三种取值（AC2.2）。
4. [x] 新增编码目录名解码函数并按本机样本
       `--D--Documents-Code-CLI-llmusage--` 写单测；确认它与 percent 编码不同，
       不复用 `grok.rs` 的解码器。
5. [x] 项目回落按 **root 相对路径第一段** 推导，不用文件父目录名。
       单测：顶层文件与嵌套子会话文件得到相同 `project_hash`，且该哈希不由
       `<run-dir>` 派生（AC2.6）。
6. [x] 新增会话头有界扫描：只读文件头部若干行寻找 `type == "session"`，
       跳过 `title` 等前置元数据记录，命中即停；不影响游标推进。
       单测覆盖「`title` 在 `session` 之前」「无会话头」两种文件。
7. [x] 接入 `ProjectResolver`：`cwd` 优先（AC2.5），回落见步骤 5，都不可用则 `None`
       （AC2.7）。复用解析器内的 resolver 缓存。
8. [x] `session_id` 优先取会话头 `id`，文件名派生作回落；`session_label` 保留文件名
       派生值。确认 `event_key` 组成未变（R2.5）。
9. [x] 集成测试：临时 home 下构造 `.omp` 会话（git 仓库 `cwd`、非 git `cwd`、
       无会话头、嵌套子会话四种），断言 `provider_label`、`project_label`、
       `project_hash`、`project_ref` 落库。
10. [x] 负向隐私断言单测：项目字段与 `session_label` 不含 `agent/sessions` 片段（AC2.9）。
11. [x] 文档（AC2.10）：`docs/adr/0010-provider-label-dimension.md` 增补
        「源自带 provider 优先，CCR 时间线只填空」并记录 dsh 修复；
        源清单页未错误描述 Pi 维度覆盖，未改。

## 验证命令

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features -- --test-threads=1
cargo run -- sync --rebuild --source omp     # 历史回填（父任务 R8）
just ci
```

本机数据校验（只读查询，对照当次扫描输出）：

```sql
SELECT provider_label, COUNT(*) FROM usage_event WHERE source='omp' GROUP BY 1;
-- 期望：无空串行；去重值与扫描输出的 providers 集合一致
SELECT project_label, COUNT(*) FROM usage_event WHERE source='omp' GROUP BY 1 ORDER BY 2 DESC;
-- 期望：无空值；包含 llmusage、ccr 等仓库名
SELECT COUNT(DISTINCT project_hash) FROM usage_event WHERE source='omp';
-- 期望：等于真源顶层项目目录数，不因 <run-dir> 膨胀
SELECT COUNT(*) FROM usage_event WHERE source='deepseek_harness' AND provider_label='';
-- 期望：重建后为 0
```

## 评审门

- 步骤 1、2 是跨源改动，先单独验证再继续 pi 侧改动。
- 步骤 5 通过后先跑 `SELECT COUNT(DISTINCT project_hash)`，确认没有伪项目膨胀。
- 步骤 6 涉及游标契约，实现后先跑 `partial_tail_does_not_advance_cursor` 类用例。

## 回滚点

- 每一步都是纯代码改动，`git restore` 可回退。
- 步骤 2 已重建 `deepseek_harness` 数据：回退代码后需再跑一次
  `sync --rebuild --source deepseek_harness` 才能与旧行为一致。
- 已回填的 `omp` 维度：回退代码后再跑 `sync --rebuild --source omp`。
