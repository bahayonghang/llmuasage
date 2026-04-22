use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{models::ProjectInfo, util::hash_string};

#[derive(Debug, Default)]
pub struct ProjectResolver {
    project_cache: HashMap<PathBuf, Option<ProjectInfo>>,
    repo_root_cache: HashMap<PathBuf, Option<PathBuf>>,
    project_ref_cache: HashMap<PathBuf, Option<String>>,
}

impl ProjectResolver {
    pub fn resolve(&mut self, start_dir: &Path) -> Result<Option<ProjectInfo>> {
        if let Some(cached) = self.project_cache.get(start_dir) {
            return Ok(cached.clone());
        }

        let resolved = self.resolve_project_info(start_dir)?;
        self.project_cache
            .insert(start_dir.to_path_buf(), resolved.clone());
        Ok(resolved)
    }

    fn resolve_project_info(&mut self, start_dir: &Path) -> Result<Option<ProjectInfo>> {
        let Some(repo_root) = self.find_git_root(start_dir) else {
            return Ok(None);
        };

        let project_ref = if let Some(cached) = self.project_ref_cache.get(&repo_root) {
            cached.clone()
        } else {
            let resolved = resolve_git_config_path(&repo_root)
                .as_deref()
                .and_then(read_git_remote_url)
                .and_then(|value| canonicalize_project_ref(&value));
            self.project_ref_cache
                .insert(repo_root.clone(), resolved.clone());
            resolved
        };

        let repo_root_text = repo_root.to_string_lossy();
        let repo_root_hash = hash_string(&repo_root_text);
        let path_hash = hash_string(&start_dir.to_string_lossy());
        let project_hash = hash_string(&repo_root_text);
        let project_label = normalize_project_label(project_ref.as_deref(), &repo_root);

        Ok(Some(ProjectInfo {
            project_hash,
            project_label,
            project_ref,
            repo_root_hash,
            path_hash,
        }))
    }

    fn find_git_root(&mut self, start_dir: &Path) -> Option<PathBuf> {
        if let Some(cached) = self.repo_root_cache.get(start_dir) {
            return cached.clone();
        }

        let mut current = start_dir.to_path_buf();
        let mut visited = Vec::new();

        loop {
            if let Some(cached) = self.repo_root_cache.get(&current) {
                let resolved = cached.clone();
                for path in visited {
                    self.repo_root_cache.insert(path, resolved.clone());
                }
                return resolved;
            }

            visited.push(current.clone());
            let git_entry = current.join(".git");
            if git_entry.exists() {
                let resolved = Some(current.clone());
                for path in visited {
                    self.repo_root_cache.insert(path, resolved.clone());
                }
                return resolved;
            }

            let next = current.parent()?.to_path_buf();
            if next == current {
                for path in visited {
                    self.repo_root_cache.insert(path, None);
                }
                return None;
            }
            current = next;
        }
    }
}

pub fn resolve_project_info(start_dir: &Path) -> Result<Option<ProjectInfo>> {
    let Some(repo_root) = find_git_root(start_dir) else {
        return Ok(None);
    };

    let git_config_path = resolve_git_config_path(&repo_root);
    let project_ref = git_config_path
        .as_deref()
        .and_then(read_git_remote_url)
        .and_then(|value| canonicalize_project_ref(&value));

    let repo_root_text = repo_root.to_string_lossy();
    let repo_root_hash = hash_string(&repo_root_text);
    let path_hash = hash_string(&start_dir.to_string_lossy());
    let project_hash = hash_string(&repo_root_text);
    let project_label = normalize_project_label(project_ref.as_deref(), &repo_root);

    Ok(Some(ProjectInfo {
        project_hash,
        project_label,
        project_ref,
        repo_root_hash,
        path_hash,
    }))
}

pub fn normalize_project_label(project_ref: Option<&str>, fallback: &Path) -> String {
    if let Some(project_ref) = project_ref {
        let trimmed = project_ref.trim_end_matches(".git").trim_end_matches('/');
        let segments: Vec<&str> = trimmed
            .split('/')
            .filter(|value| !value.is_empty())
            .collect();
        if segments.len() >= 2 {
            return format!(
                "{}/{}",
                segments[segments.len() - 2],
                segments[segments.len() - 1]
            );
        }
    }

    fallback
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-project")
        .to_string()
}

pub fn find_git_root(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();

    loop {
        let git_entry = current.join(".git");
        if git_entry.exists() {
            return Some(current);
        }

        let next = current.parent()?.to_path_buf();
        if next == current {
            return None;
        }
        current = next;
    }
}

fn resolve_git_config_path(repo_root: &Path) -> Option<PathBuf> {
    let git_path = repo_root.join(".git");
    let metadata = fs::metadata(&git_path).ok()?;

    if metadata.is_dir() {
        let config_path = git_path.join("config");
        return config_path.exists().then_some(config_path);
    }

    let content = fs::read_to_string(&git_path).ok()?;
    let git_dir = content
        .lines()
        .find_map(|line| line.strip_prefix("gitdir:").map(str::trim))
        .map(PathBuf::from)?;
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        repo_root.join(git_dir)
    };

    let config_path = git_dir.join("config");
    if config_path.exists() {
        return Some(config_path);
    }

    let common_dir = fs::read_to_string(git_dir.join("commondir")).ok()?;
    let common_dir = PathBuf::from(common_dir.trim());
    let common_dir = if common_dir.is_absolute() {
        common_dir
    } else {
        git_dir.join(common_dir)
    };
    let common_config = common_dir.join("config");
    common_config.exists().then_some(common_config)
}

fn read_git_remote_url(config_path: &Path) -> Option<String> {
    let raw = fs::read_to_string(config_path).ok()?;
    let mut current_remote: Option<String> = None;
    let mut remotes = HashMap::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[remote ") && trimmed.ends_with(']') {
            let name = trimmed.trim_start_matches("[remote ").trim_end_matches(']');
            let name = name.trim_matches('"');
            current_remote = Some(name.to_string());
            continue;
        }

        if trimmed.starts_with('[') {
            current_remote = None;
            continue;
        }

        if let Some(remote_name) = &current_remote
            && let Some(value) = trimmed.strip_prefix("url =")
        {
            remotes.insert(remote_name.clone(), value.trim().to_string());
        }
    }

    remotes
        .get("origin")
        .cloned()
        .or_else(|| remotes.into_values().next())
}

fn canonicalize_project_ref(remote_url: &str) -> Option<String> {
    let raw = remote_url.trim();
    if raw.is_empty() || raw.starts_with("file://") {
        return None;
    }

    if let Some(value) = raw.strip_prefix("git@") {
        let mut split = value.splitn(2, ':');
        let host = split.next()?.trim();
        let path = split.next()?.trim().trim_end_matches(".git");
        return Some(format!("https://{host}/{path}"));
    }

    if raw.starts_with("ssh://") || raw.starts_with("https://") || raw.starts_with("http://") {
        let normalized = raw
            .replace("ssh://", "https://")
            .replace("http://", "https://")
            .trim_end_matches(".git")
            .trim_end_matches('/')
            .to_string();
        return (!normalized.is_empty()).then_some(normalized);
    }

    None
}
