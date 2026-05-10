# 改造计划：OpenCode optional-absent 改 typed signal

> 来源：`ccr` 仓 v6.1.0 review，`.tmp/review-2026-05-10/task_plan.md` P2.1
> 目标读者：llmusage 维护者
> 状态：待开工，本仓内尚未动代码

---

## 背景

llmusage `src/parsers/opencode.rs:126` 把"DB 缺失"硬编码成字符串塞进 `stats.last_error`：

```rust
// src/parsers/opencode.rs（HEAD = 16932c0 + dirty）
if !db_path.exists() {
    cursor.sqlite_status = "missing-db".to_string();
    cursor.updated_at = now_utc();
    stats.last_error = Some("OpenCode SQLite DB 缺失".to_string());
    store.cursors().save_opencode_cursor(&cursor)?;
    return Ok(stats);
}
```

下游 `ccr-ui` 当前用 `error.contains("OpenCode SQLite DB 缺失")` 反向判断这个 absent
分支，理由是 OpenCode 是 optional source（用户没装也算正常），不该当成"导入失败"。

字符串 sentinel 三处复制：

| 位置 | 引用 |
|---|---|
| llmusage 写入方 | `src/parsers/opencode.rs:126` |
| ccr-ui 后端嗅探 | `ccr-ui/src-tauri/src/commands/usage.rs:204-206` `is_optional_source_absent` |
| ccr-ui 前端嗅探 | `ccr-ui/src/stores/usage.ts:65-79` `OPTIONAL_ABSENT_USAGE_SOURCE_MESSAGES` |

问题：

1. 中文字符串改一改下游全炸
2. i18n 不友好（生产环境若改成英文 message，下游嗅探就失效）
3. 类型不安全，依赖 substring 匹配

---

## 目标

让 llmusage 暴露 typed 信号，下游不再字符串匹配。

---

## 修法对比

### 修法 A：`SourceSyncStats` 加 `absent: bool` 字段（推荐）

```rust
// src/parsers/mod.rs（或 SourceSyncStats 定义所在文件）
pub struct SourceSyncStats {
    pub source: SourceKind,
    // …existing fields…
    pub absent: bool,                  // 新增，默认 false
    pub last_error: Option<String>,    // 保留，作 user-facing message
}
```

`src/parsers/opencode.rs:122-128` 改：

```rust
if !db_path.exists() {
    cursor.sqlite_status = "missing-db".to_string();
    cursor.updated_at = now_utc();
    stats.absent = true;
    stats.last_error = Some("OpenCode SQLite DB 缺失".to_string());
    store.cursors().save_opencode_cursor(&cursor)?;
    return Ok(stats);
}
```

下游消费：`stats.absent` 即 typed flag，`last_error` 仍可拿来给 UI 显示。

**Pros**

- 非 breaking：新加字段，旧字段保留，序列化兼容（serde 反序列化 missing field 用 default）
- 实现 minimal，2 个文件改动
- 与 SyncEvent / SyncSummary 现有流转兼容

**Cons**

- `absent` 字段是 SourceSyncStats 通用字段，但语义只对 OpenCode 这种"optional source"
  有意义，其他 source 永远 `false`
- 未来如果出现"absent 原因有多种"（DB 缺失 / DB 损坏 / 用户关闭）需扩成 enum

**Cons 缓解**：当前唯一一个 optional source 是 OpenCode，且 absent 原因只有"DB 缺失"
一种。先用 bool，未来扩成 `pub absent_reason: Option<AbsentReason>` 时是 additive 改动。

### 修法 B：`LlmusageError::OptionalSourceAbsent` typed variant

```rust
// src/error.rs
pub enum LlmusageError {
    // …
    #[error("optional source {source} absent: {reason}")]
    OptionalSourceAbsent { source: SourceKind, reason: String },
}
```

`src/parsers/opencode.rs:122-128` 改用 Result 路径：

```rust
if !db_path.exists() {
    return Err(LlmusageError::OptionalSourceAbsent {
        source: SourceKind::Opencode,
        reason: "SQLite DB 缺失".into(),
    });
}
```

**Pros**

- 错误语义清晰
- 强类型枚举，扩展时直接 match

**Cons**

- Breaking：当前是 `Ok(stats)` with `last_error`，改成 `Err` 后下游全部要重写
- `run_with_progress` / `SyncEvent` 接口需要决定：absent 是中断 sync 还是跳过这个
  source 继续后面的？当前事实语义是"跳过"，用 Err 表达"跳过"语义不直观
- 上游 `sync_opencode -> Result<SourceSyncStats>` 调用方都要在 absent 路径下提取
  Err 信息反向构造一个空 stats，逻辑反而绕

---

## 决议

**采用修法 A**。

理由：

1. 当前 sync 的事实行为是"absent 时跳过这个 source 继续后面的"，更接近 Ok-with-flag
   语义，用 Err 反而要在调用栈里写"absent 不是真错误，要降级处理"的额外逻辑
2. 不影响 `SyncEvent` / `SyncSummary` / `SourceSyncStats` 现有流转
3. 下游迁移成本最小（ccr-ui 只需把字段消费方式从 substring 切到 typed bool）
4. 非 breaking，patch 升版即可发布

---

## 落地步骤

### 1. llmusage 内修改

文件：

- `src/parsers/mod.rs`（或 `SourceSyncStats` 实际定义位置）：
  - 加 `pub absent: bool`，加 `#[serde(default)]` 保证旧 JSON 反序列化兼容
- `src/parsers/opencode.rs:122-128`：
  - 进入 absent 分支时 `stats.absent = true`
  - 保留 `stats.last_error = Some("OpenCode SQLite DB 缺失".to_string())` 作为
    user-facing message（前端可选择渲染）
- 其他 parser（claude / codex / gemini）：默认 `absent = false`，无需改动

测试：

- `tests/sync_regression.rs` 加用例：DB 路径不存在时 `stats.absent == true`，
  `stats.last_error.is_some()`，`SyncSummary` 不当成 failure

### 2. 文档与版本

- `CHANGELOG.md` 新增条目：
  ```
  - feat(parsers): 给 SourceSyncStats 加 typed `absent: bool`，让下游无需字符串嗅探
    "DB 缺失"判断 optional source 是否实际安装。OpenCode 缺 DB 时填 true，
    其他 source 默认 false。
  ```
- 版本：`0.5.x → 0.5.(x+1)`，patch 升级。
- 发版流程：commit → push → tag → cargo publish（如果发布 crates.io）；
  不发 crates.io 也可以由下游用 git rev pin 消费

### 3. ccr-ui 同步（在 ccr 仓的另一个 PR）

- `commands/usage.rs::is_optional_source_absent` 改读 `stats.absent`，
  保留函数名作为 adapter 边界
- 前端契约里 `ImportResult.is_optional_absent: boolean` 字段已在 ccr v6.1.0 PR
  里加好，前端无感知切换
- ccr 仓 grep `"OpenCode SQLite DB 缺失"` 应 0 命中

---

## 验收

- `cargo test -p llmusage` 全过
- `tests/sync_regression.rs` 含 absent 路径用例
- llmusage 0.5.(x+1) 发布
- ccr 仓 grep `"OpenCode SQLite DB 缺失"` 在 ccr-ui 部分 0 命中
- ccr-ui 用新版 llmusage 编译通过

---

## 风险与回滚

- 风险：dirty 工作（pricing / store 重构）尚未 commit，叠加本次改动需注意
  commit 拆分。建议本工作单独 commit，与 pricing 重构分开
- 回滚：`SourceSyncStats.absent` 字段保留 default false 即可，无 schema 变更，
  无数据迁移负担

---

## 关联

- 上游决策：`docs/llmusage-integration-prd.md` §F2.3 / §9c（在 ccr 仓）
- ccr v6.1.0 review plan：`.tmp/review-2026-05-10/task_plan.md` P2.1（在 ccr 仓）
- ccr follow-up issue：<https://github.com/bahayonghang/ccr/issues/35>（session_archive 迁移；与本工作独立）
