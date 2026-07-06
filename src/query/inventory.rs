//! Read-only installed-skill / MCP inventory detection for zero-call ("zombie")
//! analysis.
//!
//! Scans local skill directories and MCP config files across the three supported
//! CLIs so the behavior layer can diff *installed* against *actually used*. This
//! is pure filesystem reads: it never writes, executes, or mutates any config.
//!
//! Only user-authored skill directories (`<root>/skills/<name>/SKILL.md`) are
//! scanned. Plugin/bundled skill caches (e.g. `~/.claude/plugins/cache`) are
//! deliberately *not* enumerated, so a bundled skill that is never called is not
//! mislabeled as a removable zombie — its calls still count via the event sources.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::util::resolve_home_dir;

/// Which CLI an installed item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InventorySource {
    Claude,
    Codex,
    Opencode,
}

impl InventorySource {
    /// Stable lowercase id matching `usage_tool_call.source`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }
}

/// Installed-item family: a skill or an MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InventoryKind {
    Skill,
    Mcp,
}

impl InventoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Mcp => "mcp",
        }
    }
}

/// One locally-installed, user-authored skill or configured MCP server.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct InstalledItem {
    pub source: InventorySource,
    pub kind: InventoryKind,
    pub name: String,
}

/// Filesystem roots for inventory detection. Injected explicitly so callers
/// resolve real paths once (via [`InventoryRoots::discover`]) and tests can point
/// at temp directories.
#[derive(Debug, Clone)]
pub struct InventoryRoots {
    pub claude_skills: PathBuf,
    pub claude_mcp_config: PathBuf,
    pub codex_skills: PathBuf,
    pub codex_mcp_config: PathBuf,
    pub opencode_skills: PathBuf,
    pub opencode_mcp_config: PathBuf,
}

impl InventoryRoots {
    /// Resolves real local roots, honoring the same env overrides the parsers use
    /// (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_CONFIG_DIR`). Never hardcodes
    /// macOS `~/.…` layout; falls back to platform `config_dir` for OpenCode.
    pub fn discover() -> Self {
        let home = resolve_home_dir();
        let claude_root =
            first_env_path("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home.join(".claude"));
        let codex_root = env_path("CODEX_HOME").unwrap_or_else(|| home.join(".codex"));
        let opencode_root = env_path("OPENCODE_CONFIG_DIR").unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| home.join(".config"))
                .join("opencode")
        });

        Self {
            claude_skills: claude_root.join("skills"),
            // Claude Code keeps MCP servers in ~/.claude.json (home, not the config dir).
            claude_mcp_config: home.join(".claude.json"),
            codex_skills: codex_root.join("skills"),
            codex_mcp_config: codex_root.join("config.toml"),
            opencode_skills: opencode_root.join("skills"),
            opencode_mcp_config: opencode_root.join("opencode.json"),
        }
    }

    /// Enumerates installed skills and MCP servers across all three CLIs.
    ///
    /// Best-effort: a missing directory or unparsable config for one source is
    /// skipped without failing the others. Codex skills are intentionally *not*
    /// scanned — Codex emits no discrete skill-call event, so a Codex skill could
    /// never be confirmed "used" and would always look like a zombie.
    pub fn scan(&self) -> Vec<InstalledItem> {
        let mut items = Vec::new();
        collect_skills(&self.claude_skills, InventorySource::Claude, &mut items);
        collect_skills(&self.opencode_skills, InventorySource::Opencode, &mut items);
        collect_json_mcp(
            &self.claude_mcp_config,
            InventorySource::Claude,
            ClaudeMcpShape,
            &mut items,
        );
        collect_codex_mcp(&self.codex_mcp_config, &mut items);
        collect_json_mcp(
            &self.opencode_mcp_config,
            InventorySource::Opencode,
            OpencodeMcpShape,
            &mut items,
        );
        items.sort();
        items
    }
}

/// Reads a single env var as a path, ignoring empty values.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// `CLAUDE_CONFIG_DIR` may be comma-separated; take the first non-empty entry.
fn first_env_path(name: &str) -> Option<PathBuf> {
    let raw = std::env::var(name).ok()?;
    raw.split(',')
        .map(str::trim)
        .find(|segment| !segment.is_empty())
        .map(PathBuf::from)
}

/// Collects `<skills_dir>/<name>/SKILL.md` directory names as installed skills.
fn collect_skills(skills_dir: &Path, source: InventorySource, out: &mut Vec<InstalledItem>) {
    let Ok(entries) = fs::read_dir(skills_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join("SKILL.md").is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            let name = name.trim();
            if !name.is_empty() {
                out.push(InstalledItem {
                    source,
                    kind: InventoryKind::Skill,
                    name: name.to_string(),
                });
            }
        }
    }
}

/// Per-CLI JSON layout for MCP server keys.
trait JsonMcpShape {
    fn collect(&self, root: &serde_json::Value, out: &mut BTreeSet<String>);
}

struct ClaudeMcpShape;
impl JsonMcpShape for ClaudeMcpShape {
    fn collect(&self, root: &serde_json::Value, out: &mut BTreeSet<String>) {
        collect_object_keys(root.get("mcpServers"), out);
        if let Some(projects) = root.get("projects").and_then(|value| value.as_object()) {
            for project in projects.values() {
                collect_object_keys(project.get("mcpServers"), out);
            }
        }
    }
}

struct OpencodeMcpShape;
impl JsonMcpShape for OpencodeMcpShape {
    fn collect(&self, root: &serde_json::Value, out: &mut BTreeSet<String>) {
        collect_object_keys(root.get("mcp"), out);
        collect_object_keys(root.get("mcpServers"), out);
    }
}

fn collect_json_mcp(
    config: &Path,
    source: InventorySource,
    shape: impl JsonMcpShape,
    out: &mut Vec<InstalledItem>,
) {
    let Some(value) = fs::read_to_string(config)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return;
    };
    let mut names = BTreeSet::new();
    shape.collect(&value, &mut names);
    for name in names {
        out.push(InstalledItem {
            source,
            kind: InventoryKind::Mcp,
            name,
        });
    }
}

fn collect_object_keys(value: Option<&serde_json::Value>, out: &mut BTreeSet<String>) {
    if let Some(object) = value.and_then(|value| value.as_object()) {
        for key in object.keys() {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                out.insert(trimmed.to_string());
            }
        }
    }
}

/// Parses Codex `config.toml` for `[mcp_servers.NAME]` table keys.
fn collect_codex_mcp(config: &Path, out: &mut Vec<InstalledItem>) {
    let Some(document) = fs::read_to_string(config)
        .ok()
        .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok())
    else {
        return;
    };
    let Some(table) = document.get("mcp_servers").and_then(|item| item.as_table()) else {
        return;
    };
    for (name, _) in table.iter() {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            out.push(InstalledItem {
                source: InventorySource::Codex,
                kind: InventoryKind::Mcp,
                name: trimmed.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn scan_collects_skills_and_mcp_across_sources() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Claude: one skill + ~/.claude.json mcpServers (top-level + per-project).
        write(&root.join("claude/skills/alpha/SKILL.md"), "# alpha");
        write(&root.join("claude/skills/beta/SKILL.md"), "# beta");
        write(
            &root.join("claude.json"),
            r#"{ "mcpServers": { "context7": {} },
                "projects": { "/repo": { "mcpServers": { "playwright": {} } } } }"#,
        );
        // Codex: TOML mcp_servers; its skills dir is intentionally ignored.
        write(
            &root.join("codex/config.toml"),
            "[mcp_servers.codebase]\ncommand = \"x\"\n[mcp_servers.search]\ncommand = \"y\"\n",
        );
        write(&root.join("codex/skills/ignored/SKILL.md"), "# ignored");
        // OpenCode: opencode.json mcp block + one skill.
        write(
            &root.join("opencode/opencode.json"),
            r#"{ "mcp": { "fetcher": {} } }"#,
        );
        write(&root.join("opencode/skills/gamma/SKILL.md"), "# gamma");

        let roots = InventoryRoots {
            claude_skills: root.join("claude/skills"),
            claude_mcp_config: root.join("claude.json"),
            codex_skills: root.join("codex/skills"),
            codex_mcp_config: root.join("codex/config.toml"),
            opencode_skills: root.join("opencode/skills"),
            opencode_mcp_config: root.join("opencode/opencode.json"),
        };
        let items = roots.scan();

        let names = |source: InventorySource, kind: InventoryKind| {
            items
                .iter()
                .filter(|item| item.source == source && item.kind == kind)
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>()
        };

        assert_eq!(
            names(InventorySource::Claude, InventoryKind::Skill),
            vec!["alpha", "beta"]
        );
        assert_eq!(
            names(InventorySource::Claude, InventoryKind::Mcp),
            vec!["context7", "playwright"]
        );
        assert_eq!(
            names(InventorySource::Codex, InventoryKind::Mcp),
            vec!["codebase", "search"]
        );
        assert_eq!(
            names(InventorySource::Opencode, InventoryKind::Skill),
            vec!["gamma"]
        );
        assert_eq!(
            names(InventorySource::Opencode, InventoryKind::Mcp),
            vec!["fetcher"]
        );
        // Codex skills are not enumerated (no discrete skill-call signal).
        assert!(names(InventorySource::Codex, InventoryKind::Skill).is_empty());
    }

    #[test]
    fn scan_is_best_effort_on_missing_roots() {
        let temp = tempfile::tempdir().unwrap();
        let roots = InventoryRoots {
            claude_skills: temp.path().join("nope/skills"),
            claude_mcp_config: temp.path().join("nope/claude.json"),
            codex_skills: temp.path().join("nope/codex/skills"),
            codex_mcp_config: temp.path().join("nope/config.toml"),
            opencode_skills: temp.path().join("nope/opencode/skills"),
            opencode_mcp_config: temp.path().join("nope/opencode.json"),
        };
        assert!(roots.scan().is_empty());
    }
}
