//! Paired CLI report copy. Report data and serialized values remain neutral.
use crate::{
    common::i18n::{Language, text},
    models::SourceKind,
    query::reports::PeriodKind,
};
use serde::{Deserialize, Serialize};

/// Closed presentation facts for new report errors; no translated text is persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiagnosticFacts {
    AllDates {},
    SectionsInstances {},
    SectionsId {},
    FocusedInstances {},
    SourceConflict {
        source: SourceKind,
        explicit: SourceKind,
    },
    UnknownHost {
        label: String,
        labels: Vec<String>,
    },
    DateFormat {},
    DateValue {},
    TimezoneRange {},
    TimezoneOffset {},
    TimezoneFormat {},
    StatuslineRead {},
    StatuslineCacheDirectory {},
    StatuslineCacheWrite {},
}

impl DiagnosticFacts {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AllDates {} => "reports.all_dates",
            Self::SectionsInstances {} => "reports.sections_instances",
            Self::SectionsId {} => "reports.sections_id",
            Self::FocusedInstances {} => "reports.focused_instances",
            Self::SourceConflict { .. } => "reports.source_conflict",
            Self::UnknownHost { .. } => "reports.unknown_host",
            Self::DateFormat {} => "reports.date_format",
            Self::DateValue {} => "reports.date_value",
            Self::TimezoneRange {} => "reports.timezone_range",
            Self::TimezoneOffset {} => "reports.timezone_offset",
            Self::TimezoneFormat {} => "reports.timezone_format",
            Self::StatuslineRead {} => "reports.statusline_read",
            Self::StatuslineCacheDirectory {} => "reports.statusline_cache_directory",
            Self::StatuslineCacheWrite {} => "reports.statusline_cache_write",
        }
    }

    pub fn render(&self, language: Language) -> Option<String> {
        let pair = match self {
            Self::AllDates {} => (
                "--all cannot be combined with --since or --until",
                "--all 不能与 --since 或 --until 同时使用",
            ),
            Self::SectionsInstances {} => (
                "--sections cannot be combined with --instances",
                "--sections 不能与 --instances 同时使用",
            ),
            Self::SectionsId {} => (
                "--sections cannot be combined with --id",
                "--sections 不能与 --id 同时使用",
            ),
            Self::FocusedInstances {} => (
                "--instances is not supported for focused source reports",
                "单一来源报表不支持 --instances",
            ),
            Self::DateFormat {} => (
                "date must use YYYY-MM-DD or YYYYMMDD format",
                "日期必须使用 YYYY-MM-DD 或 YYYYMMDD 格式",
            ),
            Self::DateValue {} => (
                "date must use a valid YYYY-MM-DD or YYYYMMDD value",
                "日期必须是有效的 YYYY-MM-DD 或 YYYYMMDD 值",
            ),
            Self::TimezoneRange {} => (
                "timezone offset must be between -23:59 and +23:59",
                "时区偏移必须介于 -23:59 与 +23:59 之间",
            ),
            Self::TimezoneOffset {} => ("invalid timezone offset", "无效的时区偏移"),
            Self::TimezoneFormat {} => (
                "timezone must be UTC, local (current fixed offset), or a fixed offset like +08:00",
                "时区必须是 UTC、local（当前固定偏移）或 +08:00 等固定偏移",
            ),
            Self::StatuslineRead {} => (
                "failed to read statusline hook input",
                "无法读取状态栏钩子输入",
            ),
            Self::StatuslineCacheDirectory {} => (
                "failed to create statusline cache directory",
                "无法创建状态栏缓存目录",
            ),
            Self::StatuslineCacheWrite {} => {
                ("failed to write statusline cache", "无法写入状态栏缓存")
            }
            Self::SourceConflict { source, explicit } => {
                return Some(match language {
                    Language::English => format!(
                        "`{}` source command conflicts with `--source {}`",
                        source.as_str(),
                        explicit.as_str()
                    ),
                    Language::Chinese => format!(
                        "`{}` 来源命令与 `--source {}` 冲突",
                        source.as_str(),
                        explicit.as_str()
                    ),
                });
            }
            Self::UnknownHost { label, labels } => {
                let listed = if labels.is_empty() {
                    language.select("(none)", "（无）").to_string()
                } else {
                    labels.join(", ")
                };
                return Some(match language {
                    Language::English => {
                        format!("unknown host label '{label}'; registered labels: {listed}")
                    }
                    Language::Chinese => format!("未知主机标签 '{label}'；已注册标签：{listed}"),
                });
            }
        };
        Some(language.select(pair.0, pair.1).to_string())
    }
}

macro_rules! messages {
    ($($key:ident => ($en:literal, $zh:literal)),+ $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub enum Message { $($key),+ }
        impl Message {
            pub fn text(self) -> &'static str {
                match self { $(Self::$key => text($en, $zh)),+ }
            }
        }
    };
}

messages! {
    Date => ("Date", "日期"), Week => ("Week", "周"), Month => ("Month", "月份"),
    Session => ("Session", "会话"), Project => ("Project", "项目"),
    Models => ("Models", "模型"), Input => ("Input", "输入"), Output => ("Output", "输出"),
    Reasoning => ("Reasoning", "推理"), CacheRead => ("Cache Read", "缓存读取"),
    CacheCreate => ("Cache Create", "缓存创建"), TotalTokens => ("Total Tokens", "总 Token"),
    CostUsd => ("Cost (USD)", "成本 (USD)"), Cost => ("Cost", "成本"),
    Conv => ("Conv", "会话数"), Cache => ("Cache", "缓存"), Reason => ("Reason", "推理"),
    All => ("All", "全部"), Notes => ("Notes", "说明"), Agent => ("Agent", "来源"),
    LastActivity => ("Last Activity", "最近活动"), ActiveSpan => ("Active/Span", "活跃/跨度"),
    Block => ("Block", "用量块"), Burn => ("Burn/h", "消耗/h"), Projected => ("Projected", "预测"),
    Limit => ("Limit", "限额"), Active => ("Active", "活跃"),
    Total => ("Total", "合计"), TotalUpper => ("TOTAL", "合计"),
    Yes => ("yes", "是"), No => ("no", "否"), None => ("None", "无"),
    Empty => ("No usage data matched the report filters.", "没有符合报表筛选条件的用量数据。"),
    Unpriced => ("unpriced", "未定价"), ReasonNotReported => ("reason not reported", "未报告推理用量"),
    DailyProjects => ("Daily usage by project", "按项目统计的每日用量"),
    UsageBlocks => ("Usage blocks", "用量块"), ByHost => ("By host", "按主机统计"),
    Today => ("today", "今日"), StatusBlock => ("block", "用量块"),
    StatusProjected => ("projected", "预测"), NoActiveBlock => ("no active block", "无活跃用量块"),
    Unavailable => ("unavailable", "不可用"), Db => ("db", "数据库"),
    Hook => ("hook", "钩子"), HookUnavailable => ("hook n/a", "钩子 n/a")
}

pub fn period(kind: PeriodKind) -> &'static str {
    match kind {
        PeriodKind::Daily => text("Daily", "每日"),
        PeriodKind::Weekly => text("Weekly", "每周"),
        PeriodKind::Monthly => text("Monthly", "每月"),
        PeriodKind::Session => text("Session", "会话"),
    }
}

pub fn period_column(kind: PeriodKind) -> Message {
    match kind {
        PeriodKind::Daily => Message::Date,
        PeriodKind::Weekly => Message::Week,
        PeriodKind::Monthly => Message::Month,
        PeriodKind::Session => Message::Session,
    }
}

pub fn unified_title(kind: PeriodKind, detected: &str) -> String {
    let period = period(kind);
    match crate::common::i18n::current() {
        crate::common::i18n::Language::English => {
            format!("Coding (Agent) CLI Usage Report - {period}\nDetected: {detected}")
        }
        crate::common::i18n::Language::Chinese => {
            format!("编程来源 CLI 用量报表 - {period}\n已检测：{detected}")
        }
    }
}

pub fn focused_title(source: &str, kind: PeriodKind) -> String {
    let period = period(kind);
    match crate::common::i18n::current() {
        crate::common::i18n::Language::English => format!("{source} Usage Report - {period}"),
        crate::common::i18n::Language::Chinese => format!("{source} 用量报表 - {period}"),
    }
}

pub fn more_models(count: usize) -> String {
    match crate::common::i18n::current() {
        crate::common::i18n::Language::English => format!("- +{count} more"),
        crate::common::i18n::Language::Chinese => format!("- +{count} 个模型"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_diagnostic_facts_round_trip_without_translated_copy_and_reject_unknown_shapes() {
        let facts = DiagnosticFacts::SourceConflict {
            source: SourceKind::Codex,
            explicit: SourceKind::Claude,
        };
        let encoded = serde_json::to_string(&facts).unwrap();
        assert_eq!(
            encoded,
            r#"{"kind":"source_conflict","source":"codex","explicit":"claude"}"#
        );
        let decoded: DiagnosticFacts = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.code(), "reports.source_conflict");
        assert_eq!(
            decoded.render(Language::English).unwrap(),
            "`codex` source command conflicts with `--source claude`"
        );
        assert_eq!(
            decoded.render(Language::Chinese).unwrap(),
            "`codex` 来源命令与 `--source claude` 冲突"
        );
        for invalid in [
            r#"{"kind":"future_error"}"#,
            r#"{"kind":"all_dates","translated":"文案"}"#,
            r#"{"kind":"source_conflict","source":"future-source","explicit":"claude"}"#,
        ] {
            assert!(
                serde_json::from_str::<DiagnosticFacts>(invalid).is_err(),
                "{invalid}"
            );
        }
    }
}
