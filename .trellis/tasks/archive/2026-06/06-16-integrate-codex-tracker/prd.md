# 集成 codex-usage-tracker 到 llmusage

## Goal

为 llmusage 添加 `codex-tracer` 子命令，集成 `codex-usage-tracker` 的功能，让用户通过 `llmusage codex-tracer setup` 配置，通过 `llmusage codex-tracer` 打开 Web 界面查询 Codex 专属的详细使用数据。

同时评估与现有 llmusage 功能的合并与优化可能性。

## Background

### codex-usage-tracker (Python, ~12.5k LOC)
- **定位**：专门为 OpenAI Codex 设计的本地使用追踪工具
- **核心功能**：
  - 解析 Codex JSONL 日志 → SQLite 聚合索引
  - Web dashboard (静态 HTML + localhost 实时 API)
  - CLI 报表、CSV 导出、MCP 工具
  - 详细的 token 会计（cached/uncached input、output、reasoning、context 估算）
  - 线程关系追踪、子 agent 推断、pricing/credit 估算
  - 隐私模式（redacted/strict）
- **架构**：
  - `parser.py`：JSONL → UsageEvent（聚合指标，不存储 prompt/response）
  - `store.py`：SQLite 读写、增量刷新、线程摘要
  - `dashboard.py` + `server.py`：静态 HTML 生成 + localhost 服务
  - `context.py`：按需从 JSONL 加载上下文（带 redaction）

### llmusage (Rust, 当前 v0.7.1)
- **定位**：多平台 AI CLI 使用分析（Codex/Claude/OpenCode/Antigravity）
- **核心功能**：
  - 多源解析 → SQLite（usage_events, 30-min buckets, diagnostics）
  - TUI dashboard (`dash`) + Web dashboard (`serve`)
  - CLI 报表（daily/monthly/session/blocks）
  - Hook 集成（Codex notify、Claude Stop/SessionEnd 等）
- **架构**：
  - `src/parsers/`：各平台 parser trait
  - `src/integrations/`：hook 安装/探测
  - `src/registry.rs`：集中注册 parser/integration/source descriptor
  - `src/commands/`：子命令实现

## Requirements

### 必须实现
1. **完全 Rust 实现**：
   - 不依赖 Python 或 codex-usage-tracker PyPI 包
   - 纯 Rust 重写核心功能，保持二进制独立性

2. **CLI 接口**：
   - `llmusage codex-tracer`：启动 Codex 专属 dashboard 并在浏览器打开
   - `llmusage codex-tracer --help`：显示使用帮助
   - 不需要 `setup` 子命令（无外部依赖）

3. **核心功能复现**：
   - 解析 Codex JSONL 日志（增强现有 CodexParser）
   - 生成聚合 SQLite 索引（专用于 codex-tracer）
   - Web dashboard（静态 HTML + Rust Web 服务器）
   - 详细的 token 会计（cached/uncached input、output、reasoning）
   - 线程追踪（thread_key、call_index、parent/child 关系）
   - Call investigator（单次调用的详细分析）

4. **功能优先级**：
   - **P0（必须）**：基础解析、聚合索引、简化版 dashboard
   - **P1（重要）**：线程追踪、详细 token 会计、Calls 视图
   - **P2（次要）**：Threads 视图、Insights 视图、高级过滤器

### 可选实现
5. **数据整合**：
   - 评估是否与 llmusage 主数据库整合
   - 或使用独立的 `~/.llmusage/codex-tracer.db`

6. **Dashboard 资产复用**：
   - 评估是否可以复用 ref/codex-usage-tracker 的静态 HTML/JS/CSS
   - 或从头实现简化版 dashboard

## Acceptance Criteria

- [ ] `llmusage codex-tracer` 成功启动 dashboard 并在浏览器打开（纯 Rust，无 Python 依赖）
- [ ] Dashboard 显示 Codex 使用数据：Calls 列表、token 统计、基础过滤
- [ ] 支持详细 token 会计：cached/uncached input、output、reasoning output
- [ ] 支持线程追踪：thread_key、调用顺序、累积 token
- [ ] 技术方案文档：`design.md` 包含 Rust 重写策略、模块划分、实现路径
- [ ] 实施计划文档：`implement.md` 包含按优先级排序的实现检查清单

## Out of Scope

- 完整复现 codex-usage-tracker 的所有高级特性（Insights 卡片、MCP 服务器、复杂推荐系统）
- 修改 codex-usage-tracker 上游代码（ref/ 为只读参考）
- 支持多语言 dashboard UI（首版仅英文）
- 与 llmusage 主数据库深度整合（可作为未来优化）

## Open Questions

None. 进入技术设计阶段。

## Notes

- `ref/codex-usage-tracker` 为只读参考仓库，不应修改
- llmusage 已有 Codex 支持（parser + integration），需评估与 codex-usage-tracker 的差异
- 用户可能同时使用两个工具，需考虑兼容性
