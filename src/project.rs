use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::models::ProjectInfo;

pub fn resolve_project_info(_start_dir: &Path) -> Result<Option<ProjectInfo>> {
    Ok(None)
}

pub fn normalize_project_label(project_ref: Option<&str>, fallback: &Path) -> String {
    if let Some(project_ref) = project_ref {
        let trimmed = project_ref.trim_end_matches(".git").trim_end_matches('/');
        if let Some(repo) = trimmed.rsplit('/').next() {
            if let Some(owner) = trimmed.split('/').rev().nth(1) {
                return format!("{owner}/{repo}");
            }
        }
    }

    fallback
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-project")
        .to_string()
}

pub fn find_git_root(_start_dir: &Path) -> Option<PathBuf> {
    None
}
