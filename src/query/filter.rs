use chrono::{
    DateTime, FixedOffset, Local, NaiveDate, Offset, SecondsFormat, TimeZone, Utc,
    offset::LocalResult,
};
use rusqlite::types::Value;

use crate::models::SourceKind;

/// Timezone used by report and dashboard queries when interpreting date filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportTimezone {
    /// Interpret dates as UTC calendar days.
    Utc,
    /// Interpret dates with the machine's current local offset.
    Local,
    /// Interpret dates with a caller-provided fixed offset.
    Fixed(FixedOffset),
}

/// Stable read-side filter accepted by dashboard and ccr-ui integration APIs.
///
/// All persisted timestamps remain UTC. `since` and `until` are interpreted as
/// local calendar dates in [`QueryFilter::timezone`] and converted back to UTC
/// bounds before the SQLite query runs.
#[derive(Debug, Clone)]
pub struct QueryFilter {
    /// Optional source/platform filter.
    pub source: Option<SourceKind>,
    /// Optional exact model filter.
    pub model: Option<String>,
    /// Optional inclusive local start date.
    pub since: Option<NaiveDate>,
    /// Optional inclusive local end date.
    pub until: Option<NaiveDate>,
    /// Optional exact project hash filter.
    pub project_hash: Option<String>,
    /// Timezone used to interpret `since`/`until` and date groupings.
    pub timezone: ReportTimezone,
}

impl Default for QueryFilter {
    fn default() -> Self {
        Self {
            source: None,
            model: None,
            since: None,
            until: None,
            project_hash: None,
            timezone: ReportTimezone::Local,
        }
    }
}

impl QueryFilter {
    pub(crate) fn bucket_filter(&self, alias: Option<&str>) -> SqlFilter {
        self.sql_filter(alias, "hour_start")
    }

    pub(crate) fn event_filter(&self, alias: Option<&str>) -> SqlFilter {
        self.sql_filter(alias, "event_at")
    }

    pub(crate) fn turn_filter(&self, alias: Option<&str>) -> SqlFilter {
        self.sql_filter_with_model_column(alias, "started_at", "primary_model")
    }

    pub(crate) fn tool_filter(&self, alias: Option<&str>) -> SqlFilter {
        self.sql_filter(alias, "occurred_at")
    }

    pub(crate) fn local_time_modifier(&self) -> String {
        let seconds = self.timezone.fixed_offset().local_minus_utc();
        if seconds >= 0 {
            format!("+{seconds} seconds")
        } else {
            format!("{seconds} seconds")
        }
    }

    fn sql_filter(&self, alias: Option<&str>, time_column: &str) -> SqlFilter {
        self.sql_filter_with_model_column(alias, time_column, "model")
    }

    fn sql_filter_with_model_column(
        &self,
        alias: Option<&str>,
        time_column: &str,
        model_column: &str,
    ) -> SqlFilter {
        let mut filter = SqlFilter::default();

        if let Some(source) = self.source {
            filter.push(
                format!("{} = ?", column(alias, "source")),
                source.as_str().to_string(),
            );
        }
        if let Some(model) = self
            .model
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            filter.push(
                format!("{} = ?", column(alias, model_column)),
                model.to_string(),
            );
        }
        if let Some(project_hash) = self
            .project_hash
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            filter.push(
                format!("{} = ?", column(alias, "project_hash")),
                project_hash.to_string(),
            );
        }
        if let Some(since) = self.since {
            filter.push(
                format!("{} >= ?", column(alias, time_column)),
                self.local_date_start_utc(since)
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            );
        }
        if let Some(until) = self.until
            && let Some(exclusive) = until.succ_opt()
        {
            filter.push(
                format!("{} < ?", column(alias, time_column)),
                self.local_date_start_utc(exclusive)
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            );
        }

        filter
    }

    fn local_date_start_utc(&self, date: NaiveDate) -> DateTime<Utc> {
        let local_start = date
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always a valid NaiveDateTime");
        let offset = self.timezone.fixed_offset();
        match offset.from_local_datetime(&local_start) {
            LocalResult::Single(value) => value.with_timezone(&Utc),
            LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
            LocalResult::None => offset.from_utc_datetime(&local_start).with_timezone(&Utc),
        }
    }
}

impl ReportTimezone {
    fn fixed_offset(&self) -> FixedOffset {
        match self {
            Self::Utc => Utc.fix(),
            Self::Local => Local::now().offset().fix(),
            Self::Fixed(offset) => *offset,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SqlFilter {
    clauses: Vec<String>,
    params: Vec<Value>,
}

impl SqlFilter {
    pub(crate) fn push(&mut self, clause: impl Into<String>, value: impl Into<String>) {
        self.clauses.push(clause.into());
        self.params.push(Value::Text(value.into()));
    }

    pub(crate) fn push_value(&mut self, value: Value) {
        self.params.push(value);
    }

    pub(crate) fn push_raw(&mut self, clause: impl Into<String>) {
        self.clauses.push(clause.into());
    }

    pub(crate) fn where_sql(&self) -> String {
        if self.clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.clauses.join(" AND "))
        }
    }

    pub(crate) fn params(&self) -> &[Value] {
        &self.params
    }

    pub(crate) fn into_params(self) -> Vec<Value> {
        self.params
    }
}

fn column(alias: Option<&str>, name: &str) -> String {
    alias
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}.{name}"))
        .unwrap_or_else(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::{QueryFilter, ReportTimezone};

    #[test]
    fn default_filter_timezone_is_local() {
        assert!(matches!(
            QueryFilter::default().timezone,
            ReportTimezone::Local
        ));
    }
}
