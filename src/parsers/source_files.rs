use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use tracing::warn;
use walkdir::WalkDir;

use super::feedback::{self, InventoryDiagnostic, InventoryIssue, ParserFeedback};
use crate::{models::SourceKind, util::resolve_home_dir};

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
    /// Typed provenance indexed into errors; arbitrary additional errors stay raw.
    pub(crate) inventory_facts: Vec<(usize, InventoryIssue)>,
}

impl SourceFileListing {
    pub(crate) fn file_paths(&self) -> Vec<String> {
        self.paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect()
    }

    fn push_inventory_error(&mut self, canonical: String, fact: InventoryIssue) {
        self.inventory_facts.push((self.errors.len(), fact));
        self.errors.push(canonical);
    }

    pub(crate) fn error_summary_with_feedback(&self, source: SourceKind) -> Option<String> {
        let summary = self.error_summary()?;
        let errors: Vec<_> = self
            .errors
            .iter()
            .enumerate()
            .filter(|(_, error)| !is_ownership_skip_note(error))
            .collect();
        let samples: Vec<_> = errors
            .iter()
            .take(3)
            .map(|(index, error)| {
                self.inventory_facts
                    .iter()
                    .find(|(known, _)| known == index)
                    .map(|(_, fact)| fact.clone())
                    .unwrap_or_else(|| InventoryIssue::Unknown {
                        detail: (*error).clone(),
                    })
            })
            .collect();
        let omitted = errors.len().saturating_sub(samples.len());
        feedback::emit(ParserFeedback::Inventory {
            source,
            diagnostic: InventoryDiagnostic {
                samples,
                total: errors.len(),
                omitted,
            },
        });
        Some(summary)
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
            listing.push_inventory_error(
                format!("source file inventory error: {error}"),
                InventoryIssue::ExternalIo {
                    detail: error.to_string(),
                },
            );
            return listing;
        }
    };
    for workspace_entry in workspace_dirs {
        let workspace_entry = match workspace_entry {
            Ok(entry) => entry,
            Err(error) => {
                listing.push_inventory_error(
                    format!("source file inventory error: {error}"),
                    InventoryIssue::ExternalIo {
                        detail: error.to_string(),
                    },
                );
                continue;
            }
        };
        if !workspace_entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let session_dirs = match std::fs::read_dir(workspace_entry.path()) {
            Ok(entries) => entries,
            Err(error) => {
                listing.push_inventory_error(
                    format!("source file inventory error: {error}"),
                    InventoryIssue::ExternalIo {
                        detail: error.to_string(),
                    },
                );
                continue;
            }
        };
        for session_entry in session_dirs {
            let session_entry = match session_entry {
                Ok(entry) => entry,
                Err(error) => {
                    listing.push_inventory_error(
                        format!("source file inventory error: {error}"),
                        InventoryIssue::ExternalIo {
                            detail: error.to_string(),
                        },
                    );
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

const CINDY_PROFILE_NAMES: [&str; 2] = ["Cindy", "CindyGlobal"];
const CINDY_SESSION_DIR_ENV: &str = "LLMUSAGE_CINDY_SESSION_DIRS";

/// Enumerates Cindy session JSONL files under official app profiles.
///
/// Roots are `{config_dir}/Cindy/pi-agent-home/sessions` and
/// `{config_dir}/CindyGlobal/pi-agent-home/sessions`, unless
/// `LLMUSAGE_CINDY_SESSION_DIRS` is a non-empty comma-separated list.
/// That override replaces the official roots and does not read
/// `PI_AGENT_DIR`. Files under `sessions/shared` are imported transcripts
/// and are omitted. A candidate whose canonical path overlaps a Pi or Oh
/// My Pi root is skipped; those sources keep the file.
pub(crate) fn list_cindy_session_files() -> SourceFileListing {
    list_cindy_session_files_from(cindy_session_roots(), &cindy_protected_roots())
}

/// Official or overridden Cindy session roots. Missing directories are empty.
pub(crate) fn cindy_session_roots() -> Vec<PathBuf> {
    std::env::var_os(CINDY_SESSION_DIR_ENV)
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
        .unwrap_or_else(official_cindy_session_roots)
}

/// Session roots for the published Cindy and CindyGlobal profiles.
pub(crate) fn official_cindy_session_roots() -> Vec<PathBuf> {
    cindy_profile_session_roots(dirs::config_dir().unwrap_or_default().as_path())
}

pub(crate) fn cindy_profile_session_roots(config_dir: &Path) -> Vec<PathBuf> {
    if config_dir.as_os_str().is_empty() {
        return Vec::new();
    }
    CINDY_PROFILE_NAMES
        .into_iter()
        .map(|name| config_dir.join(name).join("pi-agent-home").join("sessions"))
        .collect()
}

fn cindy_protected_roots() -> Vec<PathBuf> {
    let mut roots = pi_session_roots();
    roots.push(omp_session_root());
    roots
}

pub(crate) fn list_cindy_session_files_from(
    roots: Vec<PathBuf>,
    protected_roots: &[PathBuf],
) -> SourceFileListing {
    let canonical_roots = roots
        .iter()
        .map(|path| canonical_path(path))
        .collect::<Vec<_>>();
    let canonical_protected = protected_roots
        .iter()
        .map(|path| canonical_path(path))
        .collect::<Vec<_>>();
    let mut listing = list_jsonl_session_files_from_roots(roots);
    let mut kept = Vec::new();
    let mut skipped = 0usize;
    for path in listing.paths {
        let canonical = canonical_path(&path);
        if cindy_shared_transcript(&canonical_roots, &canonical) {
            continue;
        }
        if canonical_protected
            .iter()
            .any(|protected| paths_overlap(&canonical, protected))
        {
            skipped += 1;
            continue;
        }
        kept.push(canonical);
    }
    if skipped > 0 {
        listing.errors.push(format!(
            "skipped {skipped} cindy session file(s) already owned by pi or omp"
        ));
    }
    kept.sort();
    listing.paths = kept;
    listing
}

fn cindy_shared_transcript(roots: &[PathBuf], path: &Path) -> bool {
    roots.iter().any(|root| {
        path.strip_prefix(root).is_ok_and(|relative| {
            relative
                .components()
                .next()
                .is_some_and(|component| component.as_os_str() == "shared")
        })
    })
}

fn list_jsonl_session_files_from_roots(roots: Vec<PathBuf>) -> SourceFileListing {
    let mut merged = SourceFileListing {
        root: roots.first().cloned().unwrap_or_default(),
        ..SourceFileListing::default()
    };
    let mut seen = HashSet::new();
    for root in roots {
        let listing = list_matching_files(root, |name, _path| name.ends_with(".jsonl"));
        let offset = merged.errors.len();
        merged.inventory_facts.extend(
            listing
                .inventory_facts
                .into_iter()
                .map(|(index, fact)| (index + offset, fact)),
        );
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
    list_antigravity_files(false)
}

/// IDE SQLite artifacts share the Gemini home but have independent ownership.
pub(crate) fn list_antigravity_ide_conversation_files() -> SourceFileListing {
    list_antigravity_files(true)
}

fn list_antigravity_files(ide: bool) -> SourceFileListing {
    let home_dir = resolve_home_dir();
    let conversations = std::env::var_os("GEMINI_CLI_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.join(".gemini"))
        .join(if ide {
            "antigravity-ide"
        } else {
            "antigravity-cli"
        })
        .join("conversations");
    let mut listing = list_matching_files(conversations, |name, _path| name.ends_with(".db"));
    listing.paths = listing
        .paths
        .into_iter()
        .map(|path| canonical_path(&path))
        .collect();
    listing.paths.sort();
    listing.paths.dedup();
    listing
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
    list_matching_files_with_root_metadata(root, predicate, |path| std::fs::metadata(path))
}

fn list_matching_files_with_root_metadata(
    root: PathBuf,
    predicate: impl Fn(&str, &Path) -> bool,
    root_metadata: impl FnOnce(&Path) -> std::io::Result<std::fs::Metadata>,
) -> SourceFileListing {
    let mut listing = SourceFileListing {
        root: root.clone(),
        ..SourceFileListing::default()
    };
    match root_metadata(&root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            listing.push_inventory_error(
                "source file inventory error: root is not a directory".to_string(),
                InventoryIssue::RootNotDirectory,
            );
            return listing;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return listing,
        Err(error) => {
            let message = format!("source file inventory error: {error}");
            warn!(event_code = "parsers.source_files.01", inventory_error_kind = "external_io", inventory_error_detail = %error, error = %message, "failed to enumerate source file inventory");
            listing.push_inventory_error(
                message,
                InventoryIssue::ExternalIo {
                    detail: error.to_string(),
                },
            );
            return listing;
        }
    }

    for entry in WalkDir::new(root).into_iter() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let message = format!("source file inventory error: {error}");
                let fact = walk_inventory_issue(&error);
                let detail = error.io_error().map(ToString::to_string);
                warn!(event_code = "parsers.source_files.02", inventory_error_kind = if detail.is_some() { "external_io" } else { "traversal" }, inventory_error_detail = detail.as_deref(), error = %message, "failed to enumerate source file inventory");
                listing.push_inventory_error(message, fact);
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

fn walk_inventory_issue(error: &walkdir::Error) -> InventoryIssue {
    error
        .io_error()
        .map(|error| InventoryIssue::ExternalIo {
            detail: error.to_string(),
        })
        .unwrap_or(InventoryIssue::Traversal)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::TempDir;

    use super::{
        cindy_profile_session_roots, list_cindy_session_files_from, list_dsh_session_files_under,
        list_grok_session_files_under, list_jsonl_session_files_from_roots, list_matching_files,
        list_matching_files_with_root_metadata, list_omp_session_files_from,
    };

    #[tokio::test]
    async fn consumed_inventory_summary_has_typed_provenance_cap_and_unknown_details() {
        use super::{InventoryIssue, ParserFeedback, SourceFileListing, SourceKind};
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("private-root");
        fs::write(&root, "not directory").unwrap();
        let (mut listing, facts) = super::feedback::collect(async {
            let listing = list_matching_files(root, |_, _| true);
            assert!(listing.error_summary().is_some());
            listing
        })
        .await;
        assert!(
            facts.is_empty(),
            "discovery/coverage consult alone must not assign a parser failure"
        );
        listing.errors.push("custom additional failure".into());
        for _ in 0..3 {
            listing.push_inventory_error(
                "source file inventory error: permission denied".into(),
                InventoryIssue::ExternalIo {
                    detail: "permission denied".into(),
                },
            );
        }
        listing
            .errors
            .push("skipped 7 omp session file(s) already owned by pi".into());
        let canonical = listing.error_summary();
        let (result, facts) = super::feedback::collect(async {
            listing.error_summary_with_feedback(SourceKind::Codex)
        })
        .await;
        assert_eq!(result, canonical);
        let ParserFeedback::Inventory { source, diagnostic } = &facts[0] else {
            panic!("inventory fact");
        };
        assert_eq!(*source, SourceKind::Codex);
        assert_eq!(diagnostic.total, 5);
        assert_eq!(diagnostic.omitted, 2);
        assert_eq!(diagnostic.samples.len(), 3);
        assert_eq!(diagnostic.samples[0], InventoryIssue::RootNotDirectory);
        assert_eq!(
            diagnostic.samples[1],
            InventoryIssue::Unknown {
                detail: "custom additional failure".into()
            }
        );
        let ownership_only = SourceFileListing {
            errors: vec!["skipped 1 omp session file(s) already owned by pi".into()],
            ..Default::default()
        };
        let (summary, facts) = super::feedback::collect(async {
            ownership_only.error_summary_with_feedback(SourceKind::Omp)
        })
        .await;
        assert!(summary.is_none() && facts.is_empty());
    }

    #[test]
    fn walk_inventory_fact_retains_os_detail_without_embedded_private_path() {
        let temp = TempDir::new().unwrap();
        let private = temp.path().join("private-source-sentinel");
        let error = walkdir::WalkDir::new(private)
            .into_iter()
            .next()
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("private-source-sentinel"));
        let super::InventoryIssue::ExternalIo { detail } = super::walk_inventory_issue(&error)
        else {
            panic!("OS detail");
        };
        assert!(!detail.contains("private-source-sentinel"));
    }

    #[test]
    fn list_matching_files_retains_root_error_after_access_recovers() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("conversations");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("stronger-copy.db"), b"native input").unwrap();
        let listing = list_matching_files_with_root_metadata(
            root.clone(),
            |name, _| name.ends_with(".db"),
            |_| Err(std::io::ErrorKind::PermissionDenied.into()),
        );
        // A later successful probe must not erase the discovery-time failure.
        assert!(fs::metadata(&root).unwrap().is_dir());
        assert!(listing.paths.is_empty());
        assert_eq!(listing.errors.len(), 1);
        assert!(listing.error_summary().is_some());
        let recovered = list_matching_files(root.clone(), |name, _| name.ends_with(".db"));
        assert!(recovered.errors.is_empty());
        assert_eq!(recovered.paths, [root.join("stronger-copy.db")]);
    }

    #[test]
    fn list_matching_files_preserves_missing_root_as_empty() {
        let dir = TempDir::new().unwrap();
        let listing = list_matching_files(dir.path().join("missing"), |_, _| true);
        assert!(listing.paths.is_empty());
        assert!(listing.errors.is_empty());
    }

    #[test]
    fn list_matching_files_rejects_non_directory_root() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("conversations");
        fs::write(&root, b"not a directory").unwrap();
        let listing = list_matching_files(root, |_, _| true);
        assert!(listing.paths.is_empty());
        assert_eq!(listing.errors.len(), 1);
        assert!(listing.error_summary().is_some());
    }

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

    #[test]
    fn cindy_profile_roots_use_exact_published_names() {
        let config = PathBuf::from("/config");
        let roots = cindy_profile_session_roots(&config);
        let rendered = roots
            .iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();
        assert_eq!(
            rendered,
            vec![
                "/config/Cindy/pi-agent-home/sessions",
                "/config/CindyGlobal/pi-agent-home/sessions",
            ]
        );
        assert!(cindy_profile_session_roots(std::path::Path::new("")).is_empty());
    }

    #[test]
    fn cindy_listing_skips_shared_transcripts_and_owned_paths() {
        let temp = TempDir::new().expect("temp dir");
        let sessions = temp.path().join("sessions");
        write_session(&sessions, "project-a", "agent_own.jsonl");
        write_session(&sessions.join("shared"), "imported", "agent_shared.jsonl");
        let owned = sessions.join("project-owned");
        write_session(&sessions, "project-owned", "agent_owned.jsonl");

        let listing = list_cindy_session_files_from(vec![sessions], &[owned]);

        assert_eq!(listing_names(&listing), vec!["agent_own.jsonl"]);
        assert_eq!(
            listing.errors,
            vec!["skipped 1 cindy session file(s) already owned by pi or omp"]
        );
        assert!(listing.error_summary().is_none());
    }
}
