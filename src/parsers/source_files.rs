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
        let errors = self
            .errors
            .iter()
            .filter(|error| !is_ownership_skip_note(error))
            .cloned()
            .collect::<Vec<_>>();
        if errors.is_empty() {
            return None;
        }
        let mut summary = errors.iter().take(3).cloned().collect::<Vec<_>>();
        if errors.len() > summary.len() {
            summary.push(format!(
                "... and {} more source inventory errors",
                errors.len() - summary.len()
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

/// Enumerates Pi session JSONL files.
///
/// Roots are `PI_AGENT_DIR` (comma-separated) or `~/.pi/agent/sessions`.
/// The Oh My Pi root is not included; overlapping `.omp` files stay on the
/// Pi source only when `PI_AGENT_DIR` itself points at them.
pub(crate) fn list_pi_session_files() -> SourceFileListing {
    list_jsonl_session_files_from_roots(pi_session_roots())
}

/// Enumerates Oh My Pi session JSONL files under `~/.omp/agent/sessions`.
///
/// A candidate is skipped when its canonical path overlaps any Pi root
/// (`equal` / ancestor / descendant). Pi wins; unconflicted `.omp` files
/// remain. The skip count is recorded in `errors`.
pub(crate) fn list_omp_session_files() -> SourceFileListing {
    list_omp_session_files_from(omp_session_root(), &pi_session_roots())
}

pub(crate) fn pi_session_roots() -> Vec<PathBuf> {
    let home_dir = resolve_home_dir();
    let default_pi_root = home_dir.join(".pi").join("agent").join("sessions");
    std::env::var_os("PI_AGENT_DIR")
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
        .unwrap_or_else(|| vec![default_pi_root])
}

pub(crate) fn omp_session_root() -> PathBuf {
    resolve_home_dir()
        .join(".omp")
        .join("agent")
        .join("sessions")
}

fn list_jsonl_session_files_from_roots(roots: Vec<PathBuf>) -> SourceFileListing {
    let mut merged = SourceFileListing {
        root: roots.first().cloned().unwrap_or_default(),
        ..SourceFileListing::default()
    };
    let mut seen = HashSet::new();
    for root in roots {
        let listing = list_matching_files(root, |name, _path| name.ends_with(".jsonl"));
        merged.errors.extend(listing.errors);
        for path in listing.paths {
            let canonical = canonical_path(&path);
            if seen.insert(canonical.clone()) {
                merged.paths.push(canonical);
            }
        }
    }
    merged.paths.sort();
    merged
}

fn list_omp_session_files_from(root: PathBuf, pi_roots: &[PathBuf]) -> SourceFileListing {
    let canonical_pi_roots = pi_roots
        .iter()
        .map(|path| canonical_path(path))
        .collect::<Vec<_>>();
    let mut listing = list_matching_files(root, |name, _path| name.ends_with(".jsonl"));
    let mut kept = Vec::new();
    let mut skipped = 0usize;
    for path in listing.paths {
        let canonical = canonical_path(&path);
        if canonical_pi_roots
            .iter()
            .any(|pi_root| paths_overlap(&canonical, pi_root))
        {
            skipped += 1;
            continue;
        }
        kept.push(canonical);
    }
    if skipped > 0 {
        listing.errors.push(format!(
            "skipped {skipped} omp session file(s) already owned by pi"
        ));
    }
    kept.sort();
    listing.paths = kept;
    listing
}

fn canonical_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn is_ownership_skip_note(error: &str) -> bool {
    error.starts_with("skipped ") && error.contains("already owned by")
}

/// Enumerates Antigravity CLI conversation SQLite files.
///
/// Root: `$GEMINI_CLI_HOME/antigravity-cli/conversations` (default
/// `~/.gemini/...`). `GEMINI_CLI_HOME` carries the same meaning as the gemini
/// platform monitor (the Gemini root, not the conversations directory).
pub(crate) fn list_antigravity_conversation_files() -> SourceFileListing {
    let home_dir = resolve_home_dir();
    let conversations = std::env::var_os("GEMINI_CLI_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.join(".gemini"))
        .join("antigravity-cli")
        .join("conversations");
    list_matching_files(conversations, |name, _path| name.ends_with(".db"))
}

/// Enumerates DeepSeek Harness session logs under `$DSH_HOME/sessions`
/// (default `~/.dsh/sessions`) at any depth.
///
/// Only files whose name is exactly `session.jsonl` or `session.jsonl.zstd`
/// match (tokscale `dsh-session-log`). Other jsonl/zstd files in the same
/// tree are excluded.
pub(crate) fn list_dsh_session_files() -> SourceFileListing {
    let home_dir = resolve_home_dir();
    let sessions_root = std::env::var_os("DSH_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.join(".dsh"))
        .join("sessions");
    list_dsh_session_files_under(sessions_root)
}

fn list_dsh_session_files_under(root: PathBuf) -> SourceFileListing {
    list_matching_files(root, |name, _path| {
        name == "session.jsonl" || name == "session.jsonl.zstd"
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

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::TempDir;

    use super::{
        list_dsh_session_files_under, list_grok_session_files_under,
        list_jsonl_session_files_from_roots, list_omp_session_files_from,
    };

    fn write_session(root: &std::path::Path, project: &str, name: &str) -> PathBuf {
        let path = root.join(project).join(name);
        fs::create_dir_all(path.parent().unwrap()).expect("create session dir");
        fs::write(&path, "{}\n").expect("write session file");
        path
    }

    fn listing_names(listing: &super::SourceFileListing) -> Vec<String> {
        listing
            .paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect()
    }

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

    #[test]
    fn dsh_listing_matches_exact_session_log_names_at_any_depth() {
        let temp = TempDir::new().expect("temp dir");
        let sessions = temp.path().join("sessions");
        let nested = sessions
            .join("--D-work--")
            .join("session-abc")
            .join("deeper");
        fs::create_dir_all(&nested).expect("create fixture layout");
        fs::write(sessions.join("session.jsonl"), "{}").expect("write uncompressed root log");
        fs::write(
            sessions
                .join("--D-work--")
                .join("session-abc")
                .join("session.jsonl.zstd"),
            b"zstd",
        )
        .expect("write compressed nested log");
        fs::write(nested.join("session.jsonl"), "{}").expect("write deep uncompressed log");
        fs::write(sessions.join("other.jsonl"), "skip").expect("write excluded jsonl");
        fs::write(sessions.join("--D-work--").join("notes.zstd"), "skip")
            .expect("write excluded zstd");
        fs::write(sessions.join("session.jsonl.bak"), "skip").expect("write excluded bak");

        let listing = list_dsh_session_files_under(sessions);
        let relative = listing
            .paths
            .iter()
            .map(|path| {
                path.strip_prefix(&listing.root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();

        assert!(listing.errors.is_empty());
        assert_eq!(
            relative,
            vec![
                "--D-work--/session-abc/deeper/session.jsonl",
                "--D-work--/session-abc/session.jsonl.zstd",
                "session.jsonl",
            ]
        );
    }

    #[test]
    fn dsh_listing_returns_empty_when_root_is_missing() {
        let temp = TempDir::new().expect("temp dir");
        let listing = list_dsh_session_files_under(temp.path().join("missing-sessions"));
        assert!(listing.paths.is_empty());
        assert!(listing.errors.is_empty());
    }

    #[test]
    fn pi_and_omp_listings_are_disjoint_when_roots_do_not_overlap() {
        let temp = TempDir::new().expect("temp dir");
        let pi_root = temp.path().join(".pi").join("agent").join("sessions");
        let omp_root = temp.path().join(".omp").join("agent").join("sessions");
        write_session(&pi_root, "project-pi", "agent_pi.jsonl");
        write_session(&omp_root, "project-omp", "agent_omp.jsonl");

        let pi = list_jsonl_session_files_from_roots(vec![pi_root.clone()]);
        let omp = list_omp_session_files_from(omp_root, &[pi_root]);

        assert_eq!(listing_names(&pi), vec!["agent_pi.jsonl"]);
        assert_eq!(listing_names(&omp), vec!["agent_omp.jsonl"]);
        assert!(pi.errors.is_empty());
        assert!(omp.errors.is_empty());
        let pi_set = pi
            .paths
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        assert!(omp.paths.iter().all(|path| !pi_set.contains(path)));
    }

    #[test]
    fn omp_skips_files_when_roots_are_equal() {
        let temp = TempDir::new().expect("temp dir");
        let shared = temp.path().join("sessions");
        write_session(&shared, "project-a", "agent_shared.jsonl");

        let pi = list_jsonl_session_files_from_roots(vec![shared.clone()]);
        let omp = list_omp_session_files_from(shared, std::slice::from_ref(&pi.root));

        assert_eq!(listing_names(&pi), vec!["agent_shared.jsonl"]);
        assert!(omp.paths.is_empty());
        assert_eq!(
            omp.errors,
            vec!["skipped 1 omp session file(s) already owned by pi"]
        );
        assert!(
            omp.error_summary().is_none(),
            "ownership skips must not skip the missing-file sweep"
        );
    }

    #[test]
    fn omp_skips_files_when_pi_root_contains_omp_root() {
        let temp = TempDir::new().expect("temp dir");
        let pi_root = temp.path().join("agent");
        let omp_root = pi_root.join("sessions");
        write_session(&omp_root, "project-a", "agent_nested.jsonl");

        let pi = list_jsonl_session_files_from_roots(vec![pi_root.clone()]);
        let omp = list_omp_session_files_from(omp_root, &[pi_root]);

        assert_eq!(listing_names(&pi), vec!["agent_nested.jsonl"]);
        assert!(omp.paths.is_empty());
        assert_eq!(
            omp.errors,
            vec!["skipped 1 omp session file(s) already owned by pi"]
        );
        assert!(omp.error_summary().is_none());
    }

    #[test]
    fn omp_keeps_unconflicted_files_when_omp_root_contains_pi_root() {
        let temp = TempDir::new().expect("temp dir");
        let omp_root = temp.path().join(".omp").join("agent").join("sessions");
        let pi_root = omp_root.join("project-owned");
        write_session(&omp_root, "project-owned", "agent_owned.jsonl");
        write_session(&omp_root, "project-free", "agent_free.jsonl");

        let pi = list_jsonl_session_files_from_roots(vec![pi_root.clone()]);
        let omp = list_omp_session_files_from(omp_root, &[pi_root]);

        assert_eq!(listing_names(&pi), vec!["agent_owned.jsonl"]);
        assert_eq!(listing_names(&omp), vec!["agent_free.jsonl"]);
        assert_eq!(
            omp.errors,
            vec!["skipped 1 omp session file(s) already owned by pi"]
        );
        assert!(omp.error_summary().is_none());
    }
}
