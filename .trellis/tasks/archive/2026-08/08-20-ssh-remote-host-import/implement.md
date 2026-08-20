# 执行计划（父任务）

父任务本身不承载实现工作，只负责子任务排序、跨子任务集成门与最终审查。每个子任务的详细清单在各自的 `implement.md`。

## 子任务顺序与依赖

```
C1 host 维度 schema 与 event_key 前缀   （无前置）
        │
        ├──► C2 SSH 传输与远端 shard 导入   （依赖 C1）
        │            │
        │            └──► C4 远端生命周期语义与文档  （依赖 C2）
        │
        └──► C3 读取层 host 维度            （依赖 C1）
```

C2 与 C3 都只依赖 C1，可以并行推进；C3 不依赖 C2，因为 C1 完成后本地事件已带 `host_id`，读取层可以先只对 `local` 生效。

## 集成门

每个子任务完成后必须通过自己的验收标准才能开始下一个。跨子任务的门：

| 门 | 触发时机 | 判据 |
|---|---|---|
| G1 | C1 完成 | AC1、AC1b、AC1c、AC1d、AC2、AC3、AC6a 全部通过；`cargo test --all-features -- --test-threads=1` 通过 |
| G2 | C2 完成 | AC4、AC5、AC5b、AC6、AC10、AC11、AC13 通过；C1 的 AC1–AC3 复测仍通过 |
| G3 | C3 完成 | AC8、AC9 通过 |
| G4 | C4 完成 | AC7、AC7d 通过；`just ci` 通过（AC12） |

G1 是最关键的门。C1 的迁移一旦发布就不可逆，必须在进入 C2 之前确认存量数据零损失。

## 验证命令

```bash
# 单元与集成测试（匹配 CI 排序）
cargo test --all-features -- --test-threads=1

# 定向测试
cargo test --test sync_regression -- --test-threads=1
cargo test --test source_file_state -- --test-threads=1
cargo test --test token_accounting_parity -- --test-threads=1
cargo test --test report_commands -- --test-threads=1
cargo test --test architecture_dependencies

# 完整门（子任务全部完成后）
just ci
```

## 风险文件与回滚点

| 文件 | 风险 | 回滚点 |
|---|---|---|
| `src/store/migrations.rs` | 迁移不可逆；重写四张表的主键与引用列 | `bootstrap` 在 v22→v23 前备份 `backups/llmusage.db.pre-0.23-host`（design.md §7） |
| `src/store/schema.rs` | `reset_for_source` 删除边界；v23 备份入口 | AC3、AC1d |
| `src/store/sync_writer.rs` | 唯一写入协议；前缀集中施加点；bucket `ON CONFLICT` | 提交前跑 `sync_regression` 与 `token_accounting_parity` |
| `src/store/cursor.rs`、`src/store/sync_status.rs`、`src/store/source_file.rs` | 主键变更后的 `ON CONFLICT` 与 forget-file | AC6、AC3 |
| `src/commands/sync.rs` | 远端阶段编排与 lossy 守卫 | AC7 |
| `src/query/filter.rs`、`src/query/reports.rs` | `QueryFilter` 与 `ReportFilter` 两条过滤面 | AC8 |
| `src/web/mod.rs` | dashboard payload | AC9 |

## 提交前检查

- `.trellis/spec/llmusage/backend/` 下五份契约按 design.md §9 更新。
- 新 ADR 写入 `docs/adr/` 并登记 `docs/adr/index.md`。
- `README.md`、`README.zh-CN.md` 与 `docs/`（含 `docs/zh/`）对应页同步。
- 迁移测试覆盖"从 v22 升级"与"全新建库"两条路径，与既有 `run_migrations_for_test(&MIGRATIONS[..N])` 模式一致（`store/migrations.rs:1626-1796`）。
- 编辑 `.rs` 文件后运行 `cargo fmt`，再提交（本机 formatter hook 会重排 import）。
