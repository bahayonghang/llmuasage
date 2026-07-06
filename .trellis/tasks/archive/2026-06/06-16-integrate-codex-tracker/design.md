# 技术设计：Rust 完全重写 codex-tracer

## 架构决策

### 设计原则

**完全 Rust 实现，零 Python 依赖**
- 参考 codex-usage-tracker (MIT 许可) 的设计思路
- 用 Rust 重写核心逻辑：parser、store、dashboard 生成、Web 服务器
- 复用 dashboard 前端资产（HTML/JS/CSS ~8280 行，MIT 许可）

---

## 模块划分

### 1. 增强 Codex Parser（基于现有 `src/parsers/codex.rs`）

**现状分析**：
- llmusage 已有 `CodexParser`，解析 rollout JSONL → `UsageEvent`
- 现有模型：`UsageEvent` 包含基础字段（event_key, source, model, tokens, session）
- **缺失字段**：thread_key, call_index, call_initiator, cached vs uncached 拆分

**增强方案**：
```rust
// src/commands/codex_tracer/models.rs （新建）

/// Codex-specific extended usage event for codex-tracer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexTracerEvent {
    // 基础字段
    pub record_id: String,
    pub session_id: String,
    pub thread_name: Option<String>,
    pub event_timestamp: String,
    pub source_file: String,
    pub line_number: i32,
    pub model: Option<String>,
    
    // 线程追踪
    pub thread_key: Option<String>,
    pub thread_call_index: Option<i32>,
    pub previous_record_id: Option<String>,
    pub next_record_id: Option<String>,
    
    // Token 精细拆分
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub uncached_input_tokens: i64,  // computed: input - cached
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    
    // 累积字段
    pub cumulative_input_tokens: i64,
    pub cumulative_cached_input_tokens: i64,
    pub cumulative_output_tokens: i64,
    pub cumulative_reasoning_output_tokens: i64,
    pub cumulative_total_tokens: i64,
    
    // 元数据
    pub cwd: Option<String>,
    pub turn_id: Option<String>,
    pub call_initiator: Option<String>,
    pub is_archived: bool,
    
    // 计算字段
    pub cache_ratio: f64,  // cached / input
    pub reasoning_output_ratio: f64,  // reasoning / output
}
```

---

### 2. 独立 SQLite 数据库（`~/.llmusage/codex-tracer.db`）

**Schema 设计**：

```sql
-- codex_tracer_events 表
CREATE TABLE codex_tracer_events (
    record_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    thread_name TEXT,
    event_timestamp TEXT NOT NULL,
    source_file TEXT NOT NULL,
    line_number INTEGER NOT NULL,
    turn_id TEXT,
    cwd TEXT,
    model TEXT,
    call_initiator TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0,
    thread_key TEXT,
    thread_call_index INTEGER,
    previous_record_id TEXT,
    next_record_id TEXT,
    
    -- Token 字段
    input_tokens INTEGER NOT NULL,
    cached_input_tokens INTEGER NOT NULL,
    uncached_input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    reasoning_output_tokens INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,
    
    -- 累积字段
    cumulative_input_tokens INTEGER NOT NULL,
    cumulative_cached_input_tokens INTEGER NOT NULL,
    cumulative_output_tokens INTEGER NOT NULL,
    cumulative_reasoning_output_tokens INTEGER NOT NULL,
    cumulative_total_tokens INTEGER NOT NULL,
    
    -- 计算字段
    cache_ratio REAL NOT NULL,
    reasoning_output_ratio REAL NOT NULL
);

CREATE INDEX idx_session_id ON codex_tracer_events(session_id);
CREATE INDEX idx_thread_key ON codex_tracer_events(thread_key);
CREATE INDEX idx_event_timestamp ON codex_tracer_events(event_timestamp);
```

**数据层实现**：
```rust
// src/commands/codex_tracer/store.rs （新建）

pub struct CodexTracerStore {
    conn: rusqlite::Connection,
}

impl CodexTracerStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }
    
    pub fn upsert_events(&mut self, events: &[CodexTracerEvent]) -> Result<usize> {
        // INSERT OR REPLACE
    }
    
    pub fn query_calls(&self, filters: &CallFilters) -> Result<Vec<CodexTracerEvent>> {
        // 过滤查询
    }
}
```

---

### 3. Dashboard 前端资产复用

**策略**：直接复用 codex-usage-tracker 的前端代码（MIT 许可）

**实现**：
```rust
// src/commands/codex_tracer/dashboard.rs （新建）

pub fn generate_dashboard(
    store: &CodexTracerStore,
    output_dir: &Path,
) -> Result<PathBuf> {
    // 1. 查询数据
    let calls = store.query_calls(&CallFilters::default())?;
    
    // 2. 生成 JSON payload
    let payload = serde_json::json!({
        "calls": calls,
        "metadata": {
            "generated_at": chrono::Utc::now().to_rfc3339(),
        }
    });
    
    // 3. 生成 HTML（使用 include_str! 嵌入模板）
    let template = include_str!("dashboard/dashboard_template.html");
    let html = template
        .replace("__TITLE__", "Codex Tracer Dashboard")
        .replace("__DATA__", &serde_json::to_string(&payload)?);
    
    // 4. 写入文件
    let dashboard_path = output_dir.join("dashboard.html");
    fs::write(&dashboard_path, html)?;
    
    // 5. 复制 JS/CSS（使用 include_str!）
    copy_dashboard_assets(output_dir)?;
    
    Ok(dashboard_path)
}
```

---

### 4. Web 服务器（基于 `axum`）

```rust
// src/commands/codex_tracer/server.rs （新建）

use axum::{Router, routing::get};

pub async fn serve_dashboard(
    db_path: PathBuf,
    port: u16,
    open_browser: bool,
) -> Result<()> {
    let store = Arc::new(Mutex::new(CodexTracerStore::open(&db_path)?));
    
    let app = Router::new()
        .route("/api/refresh", get(handle_refresh))
        .route("/api/calls", get(handle_calls_query))
        .with_state(store);
    
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("🚀 Dashboard: http://{}", addr);
    
    if open_browser {
        open::that(format!("http://{}", addr))?;
    }
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    
    Ok(())
}
```

---

## CLI 接口

```rust
pub enum Commands {
    #[command(name = "codex-tracer")]
    CodexTracer {
        #[arg(long, default_value_t = 8765)]
        port: u16,
        
        #[arg(long)]
        no_open: bool,
        
        #[arg(long)]
        rebuild: bool,
    },
}
```

---

## 实施路径

### MVP（Phase 1-5，~9-14 天）
1. **数据层**（2-3 天）：CodexTracerEvent、SQLite schema、Store
2. **增强 Parser**（3-4 天）：从 JSONL 提取精细 token 字段
3. **Dashboard 前端**（1-2 天）：复用 HTML/JS/CSS
4. **Web 服务器**（2-3 天）：axum + API 端点
5. **CLI 集成**（1-2 天）：命令注册、端到端测试

### 完整版（+Phase 6-7，~12-19 天）
6. **高级功能**（2-3 天）：线程追踪、Call investigator
7. **文档优化**（1-2 天）：README、测试、性能优化

---

## 功能范围

### ✅ 实现（MVP）
- 基础解析（cached/uncached token、累积字段）
- SQLite 存储
- 静态 dashboard（Calls 列表）
- Web 服务器 + 实时刷新
- 基础过滤（model、date range）

### ❌ 不实现（Out of Scope）
- Insights 卡片（复杂规则）
- MCP 服务器
- 多语言 UI
- Pricing/Allowance 管理
- Context API（原始 JSONL 上下文加载）

---

## 风险与缓解

### 风险 1：前端资产版权
**缓解**：保留版权声明、README 致谢

### 风险 2：Parser 兼容性
**缓解**：参考原项目实现、详细诊断日志

### 风险 3：性能问题
**缓解**：增量解析、rayon 并行、索引优化

---

## 总工作量：12-19 天（2.5-4 周）

**用户确认点**：
1. 开发周期 12-19 天是否可接受？
2. MVP 功能范围是否满足需求？
3. 是否同意复用 codex-usage-tracker 前端资产（MIT 许可）？
