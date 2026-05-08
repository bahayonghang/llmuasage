# ADR 0006 — source_file 三态状态机

- 状态：拟稿（0.5.0 sprint M2）
- 落地阶段：M2
- 相关代码：`src/store/source_file.rs`、`src/parsers/driver.rs`、`src/commands/diagnostics.rs`
- 相关术语：SourceFile / FileState / Cursor
- 关联 PRD：v1.1 §F5（D15）

## 背景

ccr-ui `UsageArchiveDiagnostics` 期待三态：live / missing / deleted_by_user。0.4.x `source_cursor` 仅记 cursor 推进位置，不区分前两者，第三态完全缺失。

## 决策

### 1. 新表 source_file

```sql
CREATE TABLE source_file (
    source       TEXT NOT NULL,
    file_path    TEXT NOT NULL,
    file_size    INTEGER,
    file_state   TEXT NOT NULL CHECK(file_state IN ('live','missing','deleted_by_user')),
    last_seen_at TEXT NOT NULL,
    PRIMARY KEY (source, file_path)
);
CREATE INDEX idx_source_file_state ON source_file(source, file_state);
```

### 2. 状态转换矩阵

```text
扫描看见  + 任何旧状态           → live
扫描没看见 + 上次 live            → missing
扫描没看见 + 上次 missing/deleted → 不变
mark_source_file_deleted          → deleted_by_user (覆盖任意前态)
```

deleted_by_user → live 视为"用户改主意"，不二次确认。

### 3. 三入口

```text
Rust:  Store::mark_source_file_deleted(SourceKind, &Path)
CLI:   llmusage diagnostics --forget-file <path> [--source <k>]
HTTP:  POST /api/diagnostics/forget  body={source, path}
```

CLI 不传 --source 时按路径前缀推断 source；推断失败报错列候选。

### 4. 写时机

- `commit_shard` 末尾同事务 upsert(source, file_path, 'live', now)
- 每 source 扫描结束跑 update_missing_states(source)：本轮没扫到 + 上次 live → missing
- mark_source_file_deleted 单事务 upsert deleted_by_user + DELETE source_cursor 同 file_path

### 5. Diagnostics 输出

```rust
SourceDiagnostics {
    live_files:    COUNT(*) WHERE state='live',
    missing_files: COUNT(*) WHERE state='missing',
    deleted_files: COUNT(*) WHERE state='deleted_by_user',
}
```

## 备选与否决

- 扩 source_cursor 加 file_state 列：cursor 与 file_path 是 1:N，列在多行不一致，否决。
- 仅二态：grilling D15 已要求三态 + 入口，否决。
- 文件系统层重命名：违反不动外部数据的边界，否决。

## Deletion-test

砍掉 source_file 表与三入口 → diagnostics 无法区分 missing/deleted → ccr-ui hardcode deleted_sources=0 → 砍 UX。违反 D15 决议。

## 后果

正面：

- 三字段语义稳定。
- 用户可忽略文件。
- 5 条 transition 单测覆盖。

负面：

- mark_source_file_deleted 删 cursor → 重新看见时全量重读。可接受。
- 历史 usage_event 行不删；如需彻底清，用 sync --rebuild --source。

## 验证

- 5 条 transition 单测
- mark_source_file_deleted_drops_cursor
- deleted_then_seen_again_resurrects_to_live
- 三入口对同一 path 调一次后状态一致集测
- diagnostics 与 source_file group by 计数一致集测
