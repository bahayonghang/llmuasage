use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::{
    error::{LlmusageError, Result},
    models::SourceKind,
    util::resolve_home_dir,
};

/// CCR activation timeline indexed by llmusage source.
#[derive(Debug, Clone, Default)]
pub struct ProviderIndex {
    timelines: HashMap<SourceKind, Vec<ProviderActivation>>,
}

#[derive(Debug, Clone)]
struct ProviderActivation {
    activated_at: DateTime<Utc>,
    sequence: usize,
    provider_label: String,
}

#[derive(Debug, Deserialize)]
struct ProviderActivationLine {
    platform: Option<String>,
    provider: Option<String>,
    activated_at: Option<String>,
    event: Option<String>,
}

impl ProviderIndex {
    /// Loads a CCR provider activation JSONL file.
    ///
    /// The file-level read is strict; individual malformed lines are skipped so
    /// one bad CCR entry does not poison an otherwise useful timeline.
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        let mut timelines: HashMap<SourceKind, Vec<ProviderActivation>> = HashMap::new();

        for (line_index, line) in reader.lines().enumerate() {
            let line = line?;
            let raw = line.trim();
            if raw.is_empty() {
                continue;
            }

            let Some((source, activated_at, provider_label)) =
                parse_activation_line(raw, path, line_index + 1)
            else {
                continue;
            };
            timelines
                .entry(source)
                .or_default()
                .push(ProviderActivation {
                    activated_at,
                    sequence: line_index,
                    provider_label,
                });
        }

        for entries in timelines.values_mut() {
            entries.sort_by(|left, right| {
                left.activated_at
                    .cmp(&right.activated_at)
                    .then(left.sequence.cmp(&right.sequence))
            });
        }

        Ok(Self { timelines })
    }

    /// Resolves the provider map for a sync run.
    ///
    /// Explicit paths are strict. Without an explicit path, llmusage probes
    /// CCR's default activation log and silently continues unattributed when the
    /// default file is absent or unreadable.
    pub fn resolve_for_sync(explicit_path: Option<&Path>) -> Result<Option<Self>> {
        if let Some(path) = explicit_path {
            return Self::load(path)
                .map(Some)
                .map_err(|source| LlmusageError::ConfigInvalid {
                    detail: format!("failed to load provider map `{}`: {source}", path.display()),
                });
        }

        let path = default_provider_map_path();
        match Self::load(&path) {
            Ok(index) => Ok(Some(index)),
            Err(LlmusageError::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
                debug!(
                    path = %path.display(),
                    "CCR provider activation log not found; syncing without provider labels"
                );
                Ok(None)
            }
            Err(err) => {
                warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to load default CCR provider activation log; syncing without provider labels"
                );
                Ok(None)
            }
        }
    }

    /// Returns the provider active for `source` at `event_at`.
    ///
    /// Unknown sources, malformed timestamps, and times before the first
    /// activation all return the empty unattributed label.
    pub fn label_for(&self, source: SourceKind, event_at: &str) -> String {
        let Ok(event_at) = DateTime::parse_from_rfc3339(event_at) else {
            return String::new();
        };
        let event_at = event_at.with_timezone(&Utc);
        let Some(entries) = self.timelines.get(&source) else {
            return String::new();
        };
        let idx = entries.partition_point(|entry| entry.activated_at <= event_at);
        if idx == 0 {
            return String::new();
        }
        entries[idx - 1].provider_label.clone()
    }
}

pub fn default_provider_map_path() -> PathBuf {
    let root = std::env::var_os("CCR_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| resolve_home_dir().join(".ccr"));
    root.join("analytics").join("provider_activation.jsonl")
}

fn parse_activation_line(
    raw: &str,
    path: &Path,
    line_number: usize,
) -> Option<(SourceKind, DateTime<Utc>, String)> {
    let parsed: ProviderActivationLine = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(error) => {
            debug!(
                path = %path.display(),
                line = line_number,
                error = %error,
                "skipping malformed provider activation line"
            );
            return None;
        }
    };

    let platform = parsed.platform.as_deref()?.trim();
    let source = match SourceKind::parse_id(platform) {
        Some(source) => source,
        None => {
            debug!(
                path = %path.display(),
                line = line_number,
                platform,
                "skipping provider activation for unknown platform"
            );
            return None;
        }
    };

    let activated_at = match parsed
        .activated_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value.trim()).ok())
    {
        Some(value) => value.with_timezone(&Utc),
        None => {
            debug!(
                path = %path.display(),
                line = line_number,
                "skipping provider activation with invalid timestamp"
            );
            return None;
        }
    };

    let event = parsed.event.as_deref().map(str::trim).unwrap_or("activate");
    let provider_label = match event {
        "clear" => String::new(),
        "activate" => parsed
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
            .to_string(),
        _ => {
            debug!(
                path = %path.display(),
                line = line_number,
                event,
                "skipping provider activation with unknown event"
            );
            return None;
        }
    };

    Some((source, activated_at, provider_label))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_map(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let path = temp.path().join("provider_activation.jsonl");
        std::fs::write(&path, contents).expect("provider map fixture");
        (temp, path)
    }

    #[test]
    fn label_for_uses_half_open_windows_and_clear_events() {
        let (_temp, path) = write_map(
            r#"
{"platform":"codex","provider":"anyrouter","activated_at":"2026-07-01T12:00:00+00:00","event":"activate"}
{"platform":"codex","provider":"methink","activated_at":"2026-07-01T12:30:00+00:00","event":"activate"}
{"platform":"codex","provider":null,"activated_at":"2026-07-01T13:00:00+00:00","event":"clear"}
"#,
        );
        let index = ProviderIndex::load(&path).expect("load map");

        assert_eq!(
            index.label_for(SourceKind::Codex, "2026-07-01T11:59:59Z"),
            ""
        );
        assert_eq!(
            index.label_for(SourceKind::Codex, "2026-07-01T12:00:00Z"),
            "anyrouter"
        );
        assert_eq!(
            index.label_for(SourceKind::Codex, "2026-07-01T12:29:59Z"),
            "anyrouter"
        );
        assert_eq!(
            index.label_for(SourceKind::Codex, "2026-07-01T12:30:00Z"),
            "methink"
        );
        assert_eq!(
            index.label_for(SourceKind::Codex, "2026-07-01T13:00:00Z"),
            ""
        );
    }

    #[test]
    fn label_for_compares_parsed_rfc3339_offsets() {
        let (_temp, path) = write_map(
            r#"
{"platform":"claude","provider":"glm","activated_at":"2026-07-01T20:00:00+08:00","event":"activate"}
"#,
        );
        let index = ProviderIndex::load(&path).expect("load map");

        assert_eq!(
            index.label_for(SourceKind::Claude, "2026-07-01T12:00:00Z"),
            "glm"
        );
    }

    #[test]
    fn load_skips_malformed_unknown_and_bad_timestamp_lines() {
        let (_temp, path) = write_map(
            r#"
not-json
{"platform":"unknown","provider":"bad","activated_at":"2026-07-01T12:00:00Z","event":"activate"}
{"platform":"codex","provider":"bad","activated_at":"nope","event":"activate"}
{"platform":"codex","provider":"ok","activated_at":"2026-07-01T12:00:00Z","event":"activate"}
"#,
        );
        let index = ProviderIndex::load(&path).expect("load map");

        assert_eq!(
            index.label_for(SourceKind::Codex, "2026-07-01T12:00:00Z"),
            "ok"
        );
        assert_eq!(
            index.label_for(SourceKind::Claude, "2026-07-01T12:00:00Z"),
            ""
        );
    }

    #[test]
    fn explicit_provider_map_path_is_strict() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let missing = temp.path().join("missing.jsonl");

        let err = ProviderIndex::resolve_for_sync(Some(&missing))
            .expect_err("explicit missing provider map should fail");
        assert!(err.to_string().contains("failed to load provider map"));
        assert!(err.to_string().contains("missing.jsonl"));
    }
}
