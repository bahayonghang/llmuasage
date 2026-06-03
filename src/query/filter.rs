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
    /// Interpret dates with the machine's current local UTC offset.
    ///
    /// This is a fixed-offset snapshot taken when the query runs, not an
    /// IANA/DST-aware timezone. Historical dates use the same current offset.
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
    use chrono::{FixedOffset, Local, NaiveDate, Offset, TimeZone, Utc, offset::LocalResult};
    use rusqlite::types::Value;

    #[test]
    fn default_filter_timezone_is_local() {
        assert!(matches!(
            QueryFilter::default().timezone,
            ReportTimezone::Local
        ));
    }

    #[test]
    fn fixed_timezone_date_bounds_use_fixed_offset_without_dst_rules() {
        let filter = QueryFilter {
            since: Some(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap()),
            timezone: ReportTimezone::Fixed(FixedOffset::west_opt(8 * 3600).unwrap()),
            ..QueryFilter::default()
        };

        let sql_filter = filter.event_filter(None);

        assert_eq!(
            sql_filter.where_sql(),
            " WHERE event_at >= ? AND event_at < ?"
        );
        assert_eq!(
            sql_filter.params(),
            &[
                Value::Text("2026-03-08T08:00:00Z".to_string()),
                Value::Text("2026-03-09T08:00:00Z".to_string())
            ]
        );
    }

    #[test]
    fn local_timezone_date_bounds_use_current_fixed_offset_snapshot() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let filter = QueryFilter {
            since: Some(date),
            until: Some(date),
            timezone: ReportTimezone::Local,
            ..QueryFilter::default()
        };
        let current_offset = Local::now().offset().fix();
        let expected_start = local_midnight_to_utc_text(date, current_offset);
        let expected_end = local_midnight_to_utc_text(date.succ_opt().unwrap(), current_offset);

        let sql_filter = filter.event_filter(None);

        assert_eq!(
            sql_filter.params(),
            &[Value::Text(expected_start), Value::Text(expected_end)],
            "`local` must use one current fixed offset for all date bounds"
        );
    }

    fn local_midnight_to_utc_text(date: NaiveDate, offset: FixedOffset) -> String {
        let local_start = date.and_hms_opt(0, 0, 0).expect("midnight is always valid");
        let utc = match offset.from_local_datetime(&local_start) {
            LocalResult::Single(value) => value.with_timezone(&Utc),
            LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
            LocalResult::None => offset.from_utc_datetime(&local_start).with_timezone(&Utc),
        };
        utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }
}
