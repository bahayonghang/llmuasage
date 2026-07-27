use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use tracing::warn;
use walkdir::WalkDir;

use crate::util::resolve_home_dir;

pub(crate) const GROK_SIDECAR_NAMES: [&str; 4] = [
    "updates.jsonl",
    "signals.json",
    "summary.json",
    "events.jsonl",
];

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

pub(crate) fn list_kimi_wire_files() -> SourceFileListing {
    let home_dir = resolve_home_dir();
    let sessions_root = std::env::var_os("KIMI_CODE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|root| root.join("sessions"))
        .unwrap_or_else(|| home_dir.join(".kimi-code").join("sessions"));
    list_matching_files(sessions_root, |name, _path| name == "wire.jsonl")
}

/// Enumerates only direct Grok Build session sidecars.
///
/// The fixed `sessions/*/*` walk is intentional. Grok session directories can
/// contain a `terminal/` subtree with blocking special files, so this source
/// must never use the recursive [`list_matching_files`] helper.
pub(crate) fn list_grok_session_files() -> SourceFileListing {
    let home_dir = resolve_home_dir();
    let sessions_root = std::env::var_os("GROK_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|root| root.join("sessions"))
        .unwrap_or_else(|| home_dir.join(".grok").join("sessions"));
    list_grok_session_files_under(sessions_root)
}

fn list_grok_session_files_under(root: PathBuf) -> SourceFileListing {
    let mut listing = SourceFileListing {
        root: root.clone(),
        ..SourceFileListing::default()
    };
    if !root.exists() {
        return listing;
    }

    let workspace_dirs = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) => {
            listing
                .errors
                .push(format!("source file inventory error: {error}"));
            return listing;
        }
    };
    for workspace_entry in workspace_dirs {
        let workspace_entry = match workspace_entry {
            Ok(entry) => entry,
            Err(error) => {
                listing
                    .errors
                    .push(format!("source file inventory error: {error}"));
                continue;
            }
        };
        if !workspace_entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let session_dirs = match std::fs::read_dir(workspace_entry.path()) {
            Ok(entries) => entries,
            Err(error) => {
                listing
                    .errors
                    .push(format!("source file inventory error: {error}"));
                continue;
            }
        };
        for session_entry in session_dirs {
            let session_entry = match session_entry {
                Ok(entry) => entry,
                Err(error) => {
                    listing
                        .errors
                        .push(format!("source file inventory error: {error}"));
                    continue;
                }
            };
            if !session_entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let session_dir = session_entry.path();
            for file_name in GROK_SIDECAR_NAMES {
                let sidecar = session_dir.join(file_name);
                if sidecar.is_file() {
                    listing.paths.push(sidecar);
                }
            }
        }
    }
    listing.paths.sort();
    listing
}

/// Enumerates Pi / Oh My Pi session JSONL files across both default roots.
///
/// Pi and Oh My Pi share one stable `pi` source. Discovery merges the Pi root
/// (`PI_AGENT_DIR` when set, else `~/.pi/agent/sessions`) with the Oh My Pi root
/// (`~/.omp/agent/sessions`) and dedupes by canonical path, so a file reachable
/// under both roots is only counted once. The root only affects discovery and
/// the per-file path hash; every parsed event still carries `source = pi`.
pub(crate) fn list_pi_session_files() -> SourceFileListing {
    let home_dir = resolve_home_dir();
    let default_pi_root = home_dir.join(".pi").join("agent").join("sessions");
    let mut pi_roots = std::env::var_os("PI_AGENT_DIR")
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .to_string_lossy()
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .filter(|roots| !roots.is_empty())
        .unwrap_or_else(|| vec![default_pi_root.clone()]);
    let omp_root = home_dir.join(".omp").join("agent").join("sessions");

    let mut merged = SourceFileListing {
        root: pi_roots
            .first()
            .cloned()
            .unwrap_or_else(|| default_pi_root.clone()),
        ..SourceFileListing::default()
    };
    pi_roots.push(omp_root);
    let mut seen = HashSet::new();
    for root in pi_roots {
        let listing = list_matching_files(root, |name, _path| name.ends_with(".jsonl"));
        merged.errors.extend(listing.errors);
        for path in listing.paths {
            let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.insert(canonical.clone()) {
                merged.paths.push(canonical);
            }
        }
    }
    merged.paths.sort();
    merged
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::list_grok_session_files_under;

    #[test]
    fn grok_listing_only_returns_root_sidecars_from_two_directory_levels() {
        let temp = TempDir::new().expect("temp dir");
        let sessions = temp.path().join("sessions");
        let session = sessions.join("D%3A%5Cwork").join("session-1");
        fs::create_dir_all(session.join("terminal").join("nested")).expect("create fixture layout");
        for name in [
            "updates.jsonl",
            "signals.json",
            "summary.json",
            "events.jsonl",
        ] {
            fs::write(session.join(name), "{}").expect("write sidecar");
        }
        fs::write(session.join("updates.jsonl.lock"), "").expect("write lock");
        fs::write(session.join("chat_history.jsonl"), "private").expect("write private file");
        fs::write(
            session
                .join("terminal")
                .join("nested")
                .join("updates.jsonl"),
            "sentinel",
        )
        .expect("write nested sentinel");

        let listing = list_grok_session_files_under(sessions);
        let names = listing
            .paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(listing.errors.is_empty());
        assert_eq!(
            names,
            vec![
                "events.jsonl",
                "signals.json",
                "summary.json",
                "updates.jsonl"
            ]
        );
        assert!(
            listing
                .paths
                .iter()
                .all(|path| path.parent() == Some(session.as_path()))
        );
    }
}
