use std::path::{Path, PathBuf};

use tracing::warn;
use walkdir::WalkDir;

use crate::util::resolve_home_dir;

/// Result of enumerating a file-backed source's candidate files.
#[derive(Debug, Clone, Default)]
pub(crate) struct SourceFileListing {
    /// Root directory used for enumeration and source-specific grouping.
    pub root: PathBuf,
    /// Existing candidate files that matched the source-specific predicate.
    pub paths: Vec<PathBuf>,
    /// Non-fatal filesystem enumeration errors seen while walking the source.
    pub errors: Vec<String>,
}

impl SourceFileListing {
    pub(crate) fn file_paths(&self) -> Vec<String> {
        self.paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect()
    }

    pub(crate) fn error_summary(&self) -> Option<String> {
        if self.errors.is_empty() {
            return None;
        }
        let mut summary = self.errors.iter().take(3).cloned().collect::<Vec<_>>();
        if self.errors.len() > summary.len() {
            summary.push(format!(
                "... and {} more source inventory errors",
                self.errors.len() - summary.len()
            ));
        }
        Some(summary.join("; "))
    }
}

pub(crate) fn list_codex_session_files() -> SourceFileListing {
    let home_dir = resolve_home_dir();
    let codex_home = std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir.join(".codex"));
    list_matching_files(codex_home.join("sessions"), |name, _path| {
        name.starts_with("rollout-") && name.ends_with(".jsonl")
    })
}

pub(crate) fn list_claude_project_logs() -> SourceFileListing {
    let home_dir = resolve_home_dir();
    list_matching_files(home_dir.join(".claude").join("projects"), |name, _path| {
        name.ends_with(".jsonl")
    })
}

fn list_matching_files(
    root: PathBuf,
    predicate: impl Fn(&str, &Path) -> bool,
) -> SourceFileListing {
    let mut listing = SourceFileListing {
        root: root.clone(),
        ..SourceFileListing::default()
    };
    if !root.exists() {
        return listing;
    }

    for entry in WalkDir::new(root).into_iter() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let message = format!("source file inventory error: {error}");
                warn!(error = %message, "failed to enumerate source file inventory");
                listing.errors.push(message);
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.into_path();
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| predicate(name, &path))
        {
            listing.paths.push(path);
        }
    }
    listing.paths.sort();
    listing
}
