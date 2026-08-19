use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageMetric {
    pub label: String,
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub remaining_label: Option<String>,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageAccount {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageOutput {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<UsageAccount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_source: Option<String>,
    pub plan: Option<String>,
    pub email: Option<String>,
    pub metrics: Vec<UsageMetric>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageFetchDiagnosticSeverity {
    Info,
    Warning,
    #[default]
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageFetchDiagnostic {
    pub provider: String,
    pub message: String,
    #[serde(default)]
    pub severity: UsageFetchDiagnosticSeverity,
}

impl UsageFetchDiagnostic {
    pub fn error(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            message: message.into(),
            severity: UsageFetchDiagnosticSeverity::Error,
        }
    }

    pub fn display_name(&self) -> String {
        self.provider.clone()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UsageFetchReport {
    pub outputs: Vec<UsageOutput>,
    pub diagnostics: Vec<UsageFetchDiagnostic>,
}

impl UsageFetchReport {
    pub fn extend(&mut self, other: Self) {
        self.outputs.extend(other.outputs);
        self.diagnostics.extend(other.diagnostics);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageReadiness {
    Ready,
    Watch,
    Critical,
    Unknown,
}

impl UsageReadiness {
    pub fn is_at_risk(self) -> bool {
        matches!(self, Self::Watch | Self::Critical)
    }
}

pub fn output_score(output: &UsageOutput) -> f64 {
    output
        .metrics
        .iter()
        .map(|metric| metric.remaining_percent)
        .fold(None, |lowest: Option<f64>, remaining| match lowest {
            Some(value) => Some(value.min(remaining)),
            None => Some(remaining),
        })
        .unwrap_or(0.0)
}

pub fn readiness_status(output: &UsageOutput) -> UsageReadiness {
    if output.metrics.is_empty() {
        return UsageReadiness::Unknown;
    }
    let lowest = output_score(output);
    if lowest < 10.0 {
        UsageReadiness::Critical
    } else if lowest < 25.0 {
        UsageReadiness::Watch
    } else {
        UsageReadiness::Ready
    }
}

pub fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
