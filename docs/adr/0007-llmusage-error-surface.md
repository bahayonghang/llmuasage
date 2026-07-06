# ADR 0007 — LlmusageError 公共错误表面

- 状态：拟稿（0.5.0 sprint M0- 骨架 / M3 收尾）
- 落地阶段：M0- 骨架 / M3 收尾
- 相关代码：`src/api/error.rs`、所有 pub fn 签名替换
- 相关术语：LlmusageError
- 关联 PRD：v1.1 §F0.3（D17）

## 背景

0.4.x 公开 API 全用 `anyhow::Result<T>`。anyhow 适合应用层串错误链，不适合做库的对外契约：调用方无法 match 区分错误类型；error chain 跨 crate 边界类型擦除严重；SemVer 友好性差。

ccr-ui Tauri 命令期待按错误类型映射 UI 提示：LockBusy → "另一处正在导入"；NotInitialized → 引导跑 init；MigrationFailed → 严重错误对话框；PricingMissing → cost 字段显示 N/A。

## 决策

### 1. 8 variant 粗粒度 enum

```rust
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum LlmusageError {
    #[error("io: {0}")]            Io(#[from] std::io::Error),
    #[error("db: {0}")]            Db(#[from] rusqlite::Error),
    #[error("parse: {0}")]         Parse(String),
    #[error("worker lock busy: pid {pid} kind {kind} since {since}")]
                                   LockBusy { pid: i32, kind: String, since: String },
    #[error("not initialized — run `llmusage init`")]
                                   NotInitialized,
    #[error("config invalid: {0}")] ConfigInvalid(String),
    #[error("migration {version} failed: {source}")]
                                   MigrationFailed { version: u32, #[source] source: anyhow::Error },
    #[error("pricing missing for source={source} model={model}")]
                                   PricingMissing { source: String, model: String },
}

pub type Result<T, E = LlmusageError> = std::result::Result<T, E>;
```

`#[non_exhaustive]` 让下游 match 必须带 `_ => ...`，0.5.x 追加 variant 不破 SemVer。

### 2. 公开 API 全替换

paths/store/query/sync/integrations 的 pub fn 全部从 anyhow::Result 改为 Result<T, LlmusageError>。

### 3. CLI 内部仍 anyhow

commands/parsers 内部续用 anyhow + context；仅边界 `.map_err(|e| ...)` 转换。

### 4. variant 选择标准

每个 variant 必须有调用方按它分支的合理理由；否则归 Io / Db / Parse / ConfigInvalid 之一。

## 备选方案与否决理由

A. 多层 per-module 错误 + 顶层包装：4 个 enum 互相 from 转换样板爆炸，否决。
B. LlmusageError::Other(anyhow) 兜底：把 anyhow 逃生通道开在公共契约，下游永远 match 不全，否决。
C. 保留 anyhow + trait downcast：rust trait downcast 不直观，否决。

## Deletion-test 论证

删除 LlmusageError → 公开 API 退回 anyhow → ccr-ui 适配层做不了精细 UI 映射 → 用户体验退化。这是面向 stable surface 的设计，符合 0.5.0 SemVer 切线初衷。

## 后果

正面：ccr-ui match 直观；公开 API 类型稳定；thiserror 自动 Display。
负面：内部 fn 仍 anyhow，边界要 map_err；0.5.0 改动面广（每个 pub fn）；新 variant 是 breaking，`#[non_exhaustive]` 缓解。

## 验证

- 编译：所有公开 fn 签名 Result<T, LlmusageError>，零 anyhow 漏出
- 集测：每个 variant 至少 1 条触发用例
- 文档：每个 variant docstring 明示触发条件 + ccr-ui 推荐分支
- changelog：列出 0.4.x → 0.5.0 错误类型迁移指南
