//! Private presentation facts beside unchanged remote import errors and outcomes.
use crate::error::LlmusageError;
use crate::models::SourceKind;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DiagnosticFacts {
    Protocol {
        remote: u32,
        schema: u32,
    },
    FirstRecord {},
    BeforeHeader {
        host: String,
    },
    NoHeader {
        host: String,
        stderr: String,
    },
    NoTrailer {
        host: String,
        stderr: String,
    },
    ExitWithoutTrailer {
        status: i32,
        stderr: String,
    },
    MissingVersions {
        host: String,
    },
    UnknownSource {
        host: String,
        source: String,
    },
    Accounting {
        host: String,
        source: SourceKind,
        remote: Option<u32>,
        expected: u32,
    },
    Historical {
        host: String,
        source: SourceKind,
        marker: Option<u32>,
        expected: u32,
    },
    Unlisted {
        host: String,
        source: SourceKind,
    },
    SkippedLines {
        host: String,
        lines: u64,
    },
}

#[derive(Debug)]
pub(crate) struct ImportFailure {
    pub(crate) error: LlmusageError,
    pub(crate) facts: Option<Box<DiagnosticFacts>>,
}

pub(crate) type Result<T> = std::result::Result<T, ImportFailure>;

impl ImportFailure {
    pub(crate) fn known(error: LlmusageError, facts: DiagnosticFacts) -> Self {
        Self {
            error,
            facts: Some(Box::new(facts)),
        }
    }

    pub(crate) fn config(detail: impl Into<String>, facts: DiagnosticFacts) -> Self {
        Self::known(
            LlmusageError::ConfigInvalid {
                detail: detail.into(),
            },
            facts,
        )
    }
}

impl From<LlmusageError> for ImportFailure {
    fn from(error: LlmusageError) -> Self {
        Self { error, facts: None }
    }
}

impl std::fmt::Display for ImportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for ImportFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
