//! Human presentation of structured parser/domain errors; original error chains stay intact.
use clap::{
    Command,
    error::{ContextKind, ErrorKind},
};

use crate::{
    LlmusageError,
    common::i18n::Language,
    sync::{SyncRequestError, SyncRequestErrorCode, SyncRequestInput, ValidatedSyncRequest},
};

#[derive(Debug, thiserror::Error)]
#[error("{canonical}")]
pub struct Diagnostic {
    canonical: String,
    facts: Option<crate::common::i18n::commands::DiagnosticFacts>,
    english: String,
    chinese: String,
    original_chain_len: Option<usize>,
}

impl Diagnostic {
    pub fn new(english: impl Into<String>, chinese: impl Into<String>) -> Self {
        let english = english.into();
        Self::with_canonical(english.clone(), english, chinese)
    }

    pub fn with_canonical(
        canonical: impl Into<String>,
        english: impl Into<String>,
        chinese: impl Into<String>,
    ) -> Self {
        Self {
            canonical: canonical.into(),
            facts: None,
            english: english.into(),
            chinese: chinese.into(),
            original_chain_len: None,
        }
    }
    pub(crate) fn with_facts(
        canonical: impl Into<String>,
        facts: impl Into<crate::common::i18n::commands::DiagnosticFacts>,
    ) -> Self {
        let canonical = canonical.into();
        let facts = facts.into();
        Self {
            english: facts
                .render(Language::English)
                .unwrap_or_else(|| canonical.clone()),
            chinese: facts
                .render(Language::Chinese)
                .unwrap_or_else(|| canonical.clone()),
            canonical,
            facts: Some(facts),
            original_chain_len: None,
        }
    }

    /// Attach display facts without adding a duplicate to canonical run details.
    pub(crate) fn with_facts_context(
        error: &anyhow::Error,
        facts: impl Into<crate::common::i18n::commands::DiagnosticFacts>,
    ) -> Self {
        let mut diagnostic = Self::with_facts(error.to_string(), facts);
        diagnostic.original_chain_len = Some(error.chain().len());
        diagnostic
    }

    pub fn render(&self, language: Language) -> &str {
        language.select(&self.english, &self.chinese)
    }
}

pub(crate) fn canonical_detail(error: &anyhow::Error) -> String {
    let chain = error.chain().collect::<Vec<_>>();
    let presentation_index = error
        .downcast_ref::<Diagnostic>()
        .and_then(|diagnostic| diagnostic.original_chain_len)
        .and_then(|original_len| chain.len().checked_sub(original_len + 1));
    chain
        .into_iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != presentation_index)
        .map(|(_, cause)| cause.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

pub(crate) fn pricing_result<T>(
    result: crate::domain::pricing_catalog::diagnostic::Result<T>,
) -> anyhow::Result<T> {
    result.map_err(|failure| {
        let error = anyhow::Error::new(failure.error);
        if let Some(facts) = failure.facts {
            let diagnostic = Diagnostic::with_facts_context(&error, *facts);
            error.context(diagnostic)
        } else {
            error
        }
    })
}

pub(crate) fn log_facts(error: &anyhow::Error) -> Option<(&'static str, String)> {
    use crate::common::i18n::commands::{DiagnosticFacts, DomainDiagnostic};
    let domain;
    let facts = if let Some(facts) = error
        .downcast_ref::<Diagnostic>()
        .and_then(|diagnostic| diagnostic.facts.as_ref())
    {
        facts
    } else {
        domain = DiagnosticFacts::Domain(DomainDiagnostic::from_error(
            error.downcast_ref::<LlmusageError>()?,
        ));
        &domain
    };
    Some((facts.code(), serde_json::to_string(facts).ok()?))
}

#[derive(Debug, thiserror::Error)]
#[error("{text}")]
pub struct ProcessOutput {
    pub text: String,
    pub stderr: bool,
    pub code: i32,
}

impl ProcessOutput {
    pub(crate) fn print(&self) {
        if self.stderr {
            eprint!("{}", self.text);
        } else {
            print!("{}", self.text);
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("CLI invocation failed")]
pub(crate) struct FailureLanguage(pub Language);

/// Keep CLI input available for presentation without changing domain validation or errors.
pub(crate) fn validate_sync_request(
    input: SyncRequestInput,
) -> anyhow::Result<ValidatedSyncRequest> {
    ValidatedSyncRequest::new(input.clone()).map_err(|error| {
        let (argument, value) = match error.code {
            SyncRequestErrorCode::UnknownSource => ("--source", input.source.unwrap_or_default()),
            SyncRequestErrorCode::InvalidRecentDays => (
                "--recent-days",
                input.recent_days.unwrap_or_default().to_string(),
            ),
            SyncRequestErrorCode::InvalidParallelism => (
                "--parallelism",
                input.parallelism.unwrap_or_default().to_string(),
            ),
        };
        anyhow::Error::new(error).context(Diagnostic::new(
            format!("Invalid argument {argument}: {value}"),
            format!("参数 {argument} 无效：{value}"),
        ))
    })
}

pub fn parser_error(
    error: &clap::Error,
    root: &Command,
    path: &[String],
    language: Language,
) -> String {
    let mut output = format!(
        "{}: {} [{:?}]\n",
        language.select("error", "错误"),
        kind_message(error.kind(), language),
        error.kind()
    );
    // Context labels are ours; values are structured argument/value data, never rendered English.
    for (kind, value) in error.context() {
        let pair = match kind {
            ContextKind::InvalidArg => Some(("Argument", "参数")),
            ContextKind::InvalidSubcommand => Some(("Command", "命令")),
            ContextKind::InvalidValue => Some(("Value", "值")),
            ContextKind::PriorArg => Some(("Conflicts with", "冲突参数")),
            ContextKind::ValidValue => Some(("possible values", "可选值")),
            ContextKind::ValidSubcommand => Some(("Commands", "可选命令")),
            ContextKind::ActualNumValues => Some(("Actual values", "实际值数量")),
            ContextKind::ExpectedNumValues => Some(("Expected values", "所需值数量")),
            ContextKind::MinValues => Some(("Minimum values", "最少值数量")),
            ContextKind::SuggestedArg => Some(("Suggested argument", "建议参数")),
            ContextKind::SuggestedValue => Some(("Suggested value", "建议值")),
            ContextKind::SuggestedSubcommand => Some(("Suggested command", "建议命令")),
            ContextKind::TrailingArg => Some(("Trailing argument", "尾随参数")),
            _ => None,
        };
        if let Some((en, zh)) = pair {
            output.push_str(&format!("  {}: {value}\n", language.select(en, zh)));
        }
    }
    if let Some(invalid) = error.get(ContextKind::InvalidArg) {
        let invalid = invalid.to_string();
        let command = super::help::find_command(root, path);
        if let Some(arg) = command.get_arguments().find(|arg| {
            let syntax = super::help::argument_syntax(arg);
            invalid == syntax
                || arg.get_long().is_some_and(|long| {
                    invalid.split_whitespace().next() == Some(format!("--{long}").as_str())
                })
                || invalid == arg.get_id().as_str()
        }) && let Some(constraint) = super::help::constraint(arg.get_id().as_str(), language)
        {
            output.push_str(&format!("  {constraint}\n"));
        }
    }
    output.push_str(&format!(
        "{} `llmusage{} --help`.\n",
        language.select("For usage, run", "查看用法，请运行"),
        if path.is_empty() {
            String::new()
        } else {
            format!(" {}", path.join(" "))
        }
    ));
    output
}

pub fn kind_message(kind: ErrorKind, language: Language) -> &'static str {
    let (en, zh) = match kind {
        ErrorKind::InvalidValue => ("Invalid value", "参数值无效"),
        ErrorKind::UnknownArgument => ("Unexpected argument or command", "未知参数或命令"),
        ErrorKind::InvalidSubcommand => ("Unknown command", "未知命令"),
        ErrorKind::NoEquals => ("Use '=' to supply the option value", "请使用等号提供参数值"),
        ErrorKind::ValueValidation => (
            "Value does not meet the argument constraints",
            "参数值不符合约束",
        ),
        ErrorKind::TooManyValues => ("Too many values", "参数值过多"),
        ErrorKind::TooFewValues => ("Too few values", "参数值不足"),
        ErrorKind::WrongNumberOfValues => ("Wrong number of values", "参数值数量不正确"),
        ErrorKind::ArgumentConflict => ("Arguments cannot be used together", "参数不能同时使用"),
        ErrorKind::MissingRequiredArgument => ("Required arguments are missing", "缺少必需参数"),
        ErrorKind::MissingSubcommand => ("A command is required", "请选择子命令"),
        ErrorKind::InvalidUtf8 => ("Argument is not valid UTF-8", "参数不是有效 UTF-8"),
        ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            ("Command help", "命令帮助")
        }
        ErrorKind::DisplayVersion => ("Version", "版本"),
        ErrorKind::Io => ("Could not read command arguments", "无法读取命令参数"),
        ErrorKind::Format => ("Could not format command output", "无法格式化命令输出"),
        _ => ("Could not parse command arguments", "无法解析命令参数"),
    };
    language.select(en, zh)
}

pub fn application_error(error: &anyhow::Error, language: Language) -> String {
    let mut parts = Vec::new();
    // anyhow context values are downcastable through anyhow::Error, but its
    // std::error::Error chain exposes context wrappers rather than the value itself.
    if let Some(diagnostic) = error.downcast_ref::<Diagnostic>() {
        parts.push(diagnostic.render(language).to_owned());
        let skip = 1 + usize::from(error.downcast_ref::<FailureLanguage>().is_some());
        for (index, source) in error.chain().skip(skip).enumerate() {
            if index == 0
                && matches!(
                    diagnostic.facts,
                    Some(
                        crate::common::i18n::commands::DiagnosticFacts::Pricing(_)
                            | crate::common::i18n::commands::DiagnosticFacts::RemoteImport(_)
                            | crate::common::i18n::commands::DiagnosticFacts::Operations(
                                crate::common::i18n::commands::OperationDiagnostic::Registration { .. }
                            )
                    )
                )
                && matches!(
                    source.downcast_ref::<LlmusageError>(),
                    Some(LlmusageError::ConfigInvalid { .. })
                )
            {
                continue;
            }
            parts.push(error_cause(source, language));
        }
    } else {
        let skip = usize::from(error.downcast_ref::<FailureLanguage>().is_some());
        for cause in error.chain().skip(skip) {
            parts.push(error_cause(cause, language));
        }
    }
    format!(
        "{}: {}",
        language.select("Error", "错误"),
        parts.join("\n  ")
    )
}

fn error_cause(cause: &(dyn std::error::Error + 'static), language: Language) -> String {
    if let Some(error) = cause.downcast_ref::<crate::sync::failure::SyncFailure>() {
        crate::common::i18n::sync::failure(&error.kind, language)
    } else if let Some(error) = cause.downcast_ref::<SyncRequestError>() {
        let explanation = match error.code {
            SyncRequestErrorCode::UnknownSource => language.select(
                "Unknown source; choose a supported source id",
                "未知来源；请选择支持的来源标识",
            ),
            SyncRequestErrorCode::InvalidParallelism => language.select(
                "parallelism must be between 1 and 32",
                "parallelism 必须在 1 到 32 之间",
            ),
            SyncRequestErrorCode::InvalidRecentDays => language.select(
                "recent_days must be between 1 and 3650",
                "recent_days 必须在 1 到 3650 之间",
            ),
        };
        format!("[{code}] {explanation}", code = error.code.as_str())
    } else if let Some(error) = cause.downcast_ref::<LlmusageError>() {
        domain_error(error, language)
    } else {
        format!(
            "{}: {cause}",
            language.select("Original detail", "原始详情")
        )
    }
}

fn domain_error(error: &LlmusageError, language: Language) -> String {
    use LlmusageError::*;
    match error {
        NotInitialized => language
            .select(
                "Not initialized; run `llmusage init`",
                "尚未初始化；请运行 `llmusage init`",
            )
            .into(),
        Io(_) => language
            .select("Filesystem operation failed", "文件系统操作失败")
            .into(),
        Db(_) => language
            .select("Database operation failed", "数据库操作失败")
            .into(),
        MigrationFailed { version, name, .. } => format!(
            "{} {version} ({name})",
            language.select("Migration failed:", "迁移失败：")
        ),
        Parse { context, .. } => format!(
            "{} {context}",
            language.select("Could not parse", "无法解析")
        ),
        SchemaTooNew {
            db_version,
            binary_version,
        } => format!(
            "{} db={db_version}, binary={binary_version}",
            language.select(
                "Database schema is too new; upgrade llmusage.",
                "数据库版本过新；请升级 llmusage。"
            )
        ),
        SchemaVersionCorrupt { raw } => format!(
            "{} {raw:?}",
            language.select(
                "Database schema_version is corrupt:",
                "数据库 schema_version 损坏："
            )
        ),
        LockBusy { holder } => format!(
            "{} {holder}",
            language.select("Worker lock busy:", "工作锁被占用：")
        ),
        LockLost => language
            .select("Worker lock lost; stop writing", "工作锁已失效；停止写入")
            .into(),
        ConfigInvalid { detail } => format!(
            "{}: {detail}",
            language.select(
                "Invalid configuration; original detail",
                "配置无效；原始详情"
            )
        ),
        Cancelled { operation } => {
            format!("{} {operation}", language.select("Cancelled:", "已取消："))
        }
        PricingMissing { source_id, model } => format!(
            "{} {source_id}:{model}",
            language.select("Pricing missing:", "缺少价格：")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_detail_skips_only_marked_context_and_preserves_original_nested_causes() {
        use crate::common::i18n::commands::OperationDiagnostic;
        let original = anyhow::Error::new(std::io::Error::other("OS_DETAIL"))
            .context("same original context")
            .context("same original context");
        let expected = format!("{original:#}");
        assert_eq!(canonical_detail(&original), expected);
        let diagnostic = Diagnostic::with_facts_context(
            &original,
            OperationDiagnostic::CleanRead {
                path: "fixture".into(),
            },
        );
        let error = original.context(diagnostic).context("later outer context");
        assert_eq!(
            canonical_detail(&error),
            format!("later outer context: {expected}")
        );
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::Other
        );
        for language in [Language::English, Language::Chinese] {
            assert_eq!(
                crate::common::i18n::with_language(language, || canonical_detail(&error)),
                format!("later outer context: {expected}")
            );
        }
        let original = anyhow::anyhow!("original");
        let unchanged = original.context(Diagnostic::new("extra context", "额外上下文"));
        assert_eq!(canonical_detail(&unchanged), format!("{unchanged:#}"));
    }

    #[test]
    fn validation_context_preserves_the_original_typed_source() {
        let input = SyncRequestInput {
            source: Some("bad-source".into()),
            recent_days: Some(4000),
            parallelism: Some(40),
            ..Default::default()
        };
        let expected = ValidatedSyncRequest::new(input.clone()).unwrap_err();
        let error = validate_sync_request(input).unwrap_err();
        assert_eq!(error.downcast_ref::<SyncRequestError>(), Some(&expected));
        for language in [Language::English, Language::Chinese] {
            let rendered = application_error(&error, language);
            assert!(rendered.contains("bad-source") && rendered.contains("unknown_source"));
            assert!(!rendered.contains("4000") && !rendered.contains("40"));
        }
    }
}
