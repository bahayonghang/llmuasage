use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::{params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::{Dashboard, QueryFilter};

const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 500;

/// Query object for cursor-paginated usage logs.
///
/// The cursor is an opaque base64url-encoded JSON payload containing the last
/// `(event_at, event_key)` pair returned by the previous page. Pagination sorts
/// by newest event first and uses the event key as a deterministic tie-breaker.
#[derive(Debug, Clone, Default)]
pub struct LogsQuery {
    /// Stable read-side filter shared with dashboard/report queries.
    pub filter: QueryFilter,
    /// Requested page size. `0` means the default page size.
    pub page_size: u32,
    /// Opaque cursor returned by the previous [`LogsPage`].
    pub cursor: Option<String>,
    /// When true, compute the total row count for the current filter.
    pub include_total: bool,
    /// When true, include the opt-in raw archive JSON if present.
    pub include_raw_json: bool,
}

/// One page of usage events for the logs view.
#[derive(Debug, Clone, Serialize)]
pub struct LogsPage {
    /// Normalized log records, ordered newest first.
    pub records: Vec<LogRecord>,
    /// Cursor to pass to the next request, or `None` at the end.
    pub next_cursor: Option<String>,
    /// Optional total row count for the active filter.
    pub total: Option<i64>,
}

/// One normalized usage event row returned by [`Dashboard::logs`].
#[derive(Debug, Clone, Serialize)]
pub struct LogRecord {
    /// Alias used by ccr-ui row identity. Equals [`Self::event_key`].
    pub id: String,
    /// Stable event key used as cursor tie-breaker and row identity.
    pub event_key: String,
    /// Source identifier (`codex` / `claude` / `opencode`).
    pub source: String,
    /// Alias for ccr-ui's platform field.
    pub platform: String,
    /// Normalized model name.
    pub model: String,
    /// RFC 3339 event timestamp.
    pub event_at: String,
    /// RFC 3339 ingestion timestamp from `usage_event.created_at`.
    pub recorded_at: String,
    /// Non-cache prompt tokens.
    pub input_tokens: i64,
    /// Cache-read prompt tokens.
    pub cache_read_tokens: i64,
    /// Cache-creation prompt tokens.
    pub cache_creation_tokens: i64,
    /// Non-reasoning output tokens.
    pub output_tokens: i64,
    /// Reasoning-only output tokens.
    pub reasoning_output_tokens: i64,
    /// Normalized total tokens.
    pub total_tokens: i64,
    /// Alias for the cache-aware event cost.
    pub cost_usd: f64,
    /// Cache-aware event cost.
    pub cost_with_cache_usd: f64,
    /// Event cost if cache reads were billed as regular input.
    pub cost_without_cache_usd: f64,
    /// Pricing status (`static`, `snapshot`, or `unpriced`).
    pub pricing_status: String,
    /// Pricing catalog/source label when matched.
    pub pricing_source: Option<String>,
    /// Pricing rate JSON when matched.
    pub pricing_rate: Option<String>,
    /// Stable project hash if available.
    pub project_hash: Option<String>,
    /// Human-readable project label if available.
    pub project_label: Option<String>,
    /// Repo/project reference if available.
    pub project_ref: Option<String>,
    /// Raw project path when the parser could preserve it.
    pub project_path: Option<String>,
    /// Full token object for ccr-ui UsageRecordV2 adapters.
    pub token: LogTokenBreakdown,
    /// Event path hash from the source file / project.
    pub path_hash: Option<String>,
    /// Stable source path hash used to group sessions by local file.
    pub source_path_hash: Option<String>,
    /// Canonical source id for the upstream row. Currently equals `event_key`.
    pub source_id: String,
    /// Session id if the source provides one.
    pub session_id: Option<String>,
    /// Human-readable session label if available.
    pub session_label: Option<String>,
    /// Optional raw archive JSON. Present only when `include_raw_json=true`
    /// and raw archive was enabled before sync.
    pub raw_json: Option<String>,
}

/// Token breakdown nested on [`LogRecord`] for adapter-friendly JSON.
#[derive(Debug, Clone, Serialize)]
pub struct LogTokenBreakdown {
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CursorPayload {
    event_at: String,
    event_key: String,
}

pub(crate) fn load(dashboard: &Dashboard, query: &LogsQuery) -> Result<LogsPage> {
    let page_size = normalize_page_size(query.page_size);
    let cursor = query
        .cursor
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(decode_cursor)
        .transpose()?;

    let mut filter = query.filter.event_filter(Some("e"));
    if let Some(cursor) = &cursor {
        filter.push_raw("(e.event_at < ? OR (e.event_at = ? AND e.event_key < ?))");
        filter.push_value(SqlValue::Text(cursor.event_at.clone()));
        filter.push_value(SqlValue::Text(cursor.event_at.clone()));
        filter.push_value(SqlValue::Text(cursor.event_key.clone()));
    }

    let total = if query.include_total {
        let total_filter = query.filter.event_filter(Some("e"));
        let sql = format!(
            "SELECT COUNT(*) FROM usage_event e{}",
            total_filter.where_sql()
        );
        Some(dashboard.conn.query_row(
            &sql,
            params_from_iter(total_filter.params().iter()),
            |row| row.get::<_, i64>(0),
        )?)
    } else {
        None
    };

    let raw_column = if query.include_raw_json {
        "r.raw_json"
    } else {
        "NULL"
    };
    let sql = format!(
        r#"
        SELECT
            e.event_key,
            e.source,
            e.model,
            e.event_at,
            e.input_tokens,
            e.cache_read_tokens,
            e.cache_creation_tokens,
            e.output_tokens,
            e.reasoning_output_tokens,
            e.total_tokens,
            e.cost_with_cache_usd,
            e.cost_without_cache_usd,
            e.pricing_status,
            e.pricing_source,
            e.pricing_rate,
            e.project_hash,
            e.project_label,
            e.project_ref,
            e.project_path,
            e.path_hash,
            e.source_path_hash,
            e.session_id,
            e.session_label,
            e.created_at,
            {raw_column}
        FROM usage_event e
        LEFT JOIN usage_event_raw r ON r.event_key = e.event_key
        {}
        ORDER BY e.event_at DESC, e.event_key DESC
        LIMIT ? 
        "#,
        filter.where_sql()
    );

    let mut params = filter.into_params();
    params.push(SqlValue::Integer(page_size as i64 + 1));

    let mut stmt = dashboard.conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
        let event_key: String = row.get(0)?;
        let source: String = row.get(1)?;
        let input_tokens = row.get(4)?;
        let cache_read_tokens = row.get(5)?;
        let cache_creation_tokens = row.get(6)?;
        let output_tokens = row.get(7)?;
        let reasoning_output_tokens = row.get(8)?;
        let total_tokens = row.get(9)?;
        let cost_with_cache_usd = row.get::<_, Option<f64>>(10)?.unwrap_or_default();
        Ok(LogRecord {
            id: event_key.clone(),
            event_key: event_key.clone(),
            source: source.clone(),
            platform: source,
            model: row.get(2)?,
            event_at: row.get(3)?,
            input_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
            cost_usd: cost_with_cache_usd,
            cost_with_cache_usd,
            cost_without_cache_usd: row.get::<_, Option<f64>>(11)?.unwrap_or_default(),
            pricing_status: row
                .get::<_, Option<String>>(12)?
                .unwrap_or_else(|| "unpriced".to_string()),
            pricing_source: row.get(13)?,
            pricing_rate: row.get(14)?,
            project_hash: row.get(15)?,
            project_label: row.get(16)?,
            project_ref: row.get(17)?,
            project_path: row.get(18)?,
            token: LogTokenBreakdown {
                input_tokens,
                cache_read_tokens,
                cache_creation_tokens,
                output_tokens,
                reasoning_output_tokens,
                total_tokens,
            },
            path_hash: row.get(19)?,
            source_path_hash: row.get(20)?,
            source_id: event_key,
            session_id: row.get(21)?,
            session_label: row.get(22)?,
            recorded_at: row.get(23)?,
            raw_json: row.get(24)?,
        })
    })?;
    let mut records = rows.collect::<rusqlite::Result<Vec<_>>>()?;

    let next_cursor = if records.len() > page_size as usize {
        records.truncate(page_size as usize);
        records
            .last()
            .map(|last| encode_cursor(&last.event_at, &last.event_key))
    } else {
        None
    };

    Ok(LogsPage {
        records,
        next_cursor,
        total,
    })
}

/// Encodes a logs cursor as base64url(JSON{event_at,event_key}).
pub fn encode_cursor(event_at: &str, event_key: &str) -> String {
    let payload = CursorPayload {
        event_at: event_at.to_string(),
        event_key: event_key.to_string(),
    };
    let json = serde_json::to_vec(&payload).expect("cursor payload serializes");
    URL_SAFE_NO_PAD.encode(json)
}

/// Decodes a logs cursor. Invalid input returns `None`; HTTP turns that into
/// a 400 rather than an internal server error.
pub(crate) fn try_decode_cursor(raw: &str) -> Option<(String, String)> {
    decode_cursor(raw)
        .ok()
        .map(|payload| (payload.event_at, payload.event_key))
}

fn decode_cursor(raw: &str) -> Result<CursorPayload> {
    let bytes = URL_SAFE_NO_PAD.decode(raw).map_err(invalid_cursor)?;
    let payload: CursorPayload = serde_json::from_slice(&bytes).map_err(invalid_cursor)?;
    if payload.event_at.trim().is_empty() || payload.event_key.trim().is_empty() {
        return Err(invalid_cursor("cursor fields must be non-empty"));
    }
    Ok(payload)
}

fn invalid_cursor(error: impl std::fmt::Display) -> crate::LlmusageError {
    crate::LlmusageError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("invalid logs cursor: {error}"),
    ))
}

fn normalize_page_size(page_size: u32) -> u32 {
    if page_size == 0 {
        DEFAULT_PAGE_SIZE
    } else {
        page_size.min(MAX_PAGE_SIZE)
    }
}
