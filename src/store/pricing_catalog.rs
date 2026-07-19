use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

use rusqlite::OptionalExtension;
use serde::Serialize;

use super::{BootstrapProgressSink, Store};
use crate::{
    error::{LlmusageError, Result},
    query::{PricingCatalog, PricingStatus},
    util::hash_string,
};

const META_ACTIVE_VERSION: &str = "pricing_catalog_version";
const META_ACTIVE_FILE: &str = "pricing_catalog_file";
const META_BASE_VERSION: &str = "pricing_catalog_base_version";
const META_BASE_FILE: &str = "pricing_catalog_base_file";
const META_OVERLAY_VERSION: &str = "pricing_catalog_overlay_version";
const META_OVERLAY_FILE: &str = "pricing_catalog_overlay_file";

const CATALOG_META_KEYS: [&str; 6] = [
    META_ACTIVE_VERSION,
    META_ACTIVE_FILE,
    META_BASE_VERSION,
    META_BASE_FILE,
    META_OVERLAY_VERSION,
    META_OVERLAY_FILE,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogLayerStatus {
    /// Runtime identity persisted in SQLite and stamped into `pricing_source`.
    pub identity: String,
    /// Human-readable version declared by the catalog author.
    pub version: String,
    pub kind: String,
    pub file: Option<String>,
    pub schema_version: u32,
    pub model_count: usize,
    pub source_rule_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PricingCatalogStatus {
    pub base: CatalogLayerStatus,
    pub overlay: Option<CatalogLayerStatus>,
    pub effective: CatalogLayerStatus,
    pub rebase_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogApplyResult {
    pub base: CatalogLayerStatus,
    pub overlay: CatalogLayerStatus,
    pub effective: CatalogLayerStatus,
    pub updated_events: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogResetResult {
    pub effective: CatalogLayerStatus,
    pub updated_events: usize,
    pub removed_overlay: bool,
}

pub(super) struct PricingMetaChange {
    pub(super) deletes: Vec<String>,
    pub(super) upserts: Vec<(String, String)>,
}

impl PricingMetaChange {
    pub(super) fn for_catalog(store: &Store, catalog: &PricingCatalog) -> Result<Self> {
        if catalog.status == PricingStatus::Static
            && catalog.document() == PricingCatalog::embedded().document()
        {
            return Ok(Self::active(&catalog.version, None));
        }

        let canonical = catalog.document().canonical_json()?;
        let digest = hash_string(&canonical);
        let effective_identity = format!("effective-{digest}");
        let file = if catalog.version == effective_identity {
            format!("{effective_identity}.json")
        } else {
            format!("base-{digest}.json")
        };
        store.persist_catalog_file(&file, &canonical)?;
        Ok(Self::active(&catalog.version, Some(&file)))
    }

    pub(super) fn preserve() -> Self {
        Self {
            deletes: Vec::new(),
            upserts: Vec::new(),
        }
    }

    fn active(identity: &str, file: Option<&str>) -> Self {
        let mut change = Self::clearing_layers();
        change.upsert(META_ACTIVE_VERSION, identity);
        if let Some(file) = file {
            change.upsert(META_ACTIVE_FILE, file);
        }
        change
    }

    fn overlay(
        effective_identity: &str,
        effective_file: &str,
        base_identity: &str,
        base_file: &str,
        overlay_identity: &str,
        overlay_file: &str,
    ) -> Self {
        let mut change = Self {
            deletes: Vec::new(),
            upserts: Vec::new(),
        };
        change.upsert(META_ACTIVE_VERSION, effective_identity);
        change.upsert(META_ACTIVE_FILE, effective_file);
        change.upsert(META_BASE_VERSION, base_identity);
        change.upsert(META_BASE_FILE, base_file);
        change.upsert(META_OVERLAY_VERSION, overlay_identity);
        change.upsert(META_OVERLAY_FILE, overlay_file);
        change
    }

    fn clearing_layers() -> Self {
        Self {
            deletes: CATALOG_META_KEYS[1..]
                .iter()
                .map(|key| (*key).to_string())
                .collect(),
            upserts: Vec::new(),
        }
    }

    fn upsert(&mut self, key: &str, value: &str) {
        self.upserts.push((key.to_string(), value.to_string()));
    }
}

#[derive(Debug, Default)]
struct CatalogMeta {
    active_version: Option<String>,
    active_file: Option<String>,
    base_version: Option<String>,
    base_file: Option<String>,
    overlay_version: Option<String>,
    overlay_file: Option<String>,
}

impl CatalogMeta {
    fn has_overlay(&self) -> bool {
        self.overlay_version.is_some() || self.overlay_file.is_some()
    }

    fn require_pair<'a>(
        version: &'a Option<String>,
        file: &'a Option<String>,
        label: &str,
    ) -> Result<(&'a str, &'a str)> {
        match (version.as_deref(), file.as_deref()) {
            (Some(version), Some(file)) => Ok((version, file)),
            _ => Err(config_invalid(format!(
                "active pricing {label} metadata is incomplete"
            ))),
        }
    }
}

impl Store {
    /// Loads the selected pricing catalog. Once SQLite points at a user file,
    /// any missing, corrupted, or invalid file is a hard error.
    pub(crate) fn active_pricing_catalog(&self) -> Result<PricingCatalog> {
        let meta = self.pricing_meta()?;
        let identity = meta
            .active_version
            .as_deref()
            .unwrap_or(PricingCatalog::embedded().version.as_str());

        if meta.active_file.is_none() && identity == PricingCatalog::embedded().version {
            return Ok(PricingCatalog::embedded().clone());
        }

        let relative = meta
            .active_file
            .clone()
            .unwrap_or_else(|| format!("{identity}.json"));
        self.load_base_layer(identity, &relative, PricingStatus::Snapshot)
            .map_err(|error| {
                config_invalid(format!(
                    "active pricing catalog `{identity}` could not be loaded: {error}"
                ))
            })
    }

    /// Applies a v2 overlay to the recorded base layer and atomically selects
    /// the merged effective catalog after event and bucket recomputation.
    pub fn apply_pricing_overlay(&self, source_path: &Path) -> Result<CatalogApplyResult> {
        validate_local_file(source_path, "pricing overlay")?;
        let overlay_document = PricingCatalog::load_overlay(source_path)?;
        let overlay_json = overlay_document.canonical_json()?;
        let overlay_digest = hash_string(&overlay_json);
        let overlay_identity = overlay_document.version.clone();
        let overlay_file = format!("overlays/overlay-{overlay_digest}.json");

        let meta = self.pricing_meta()?;
        let (mut base, base_identity, base_file) = if meta.has_overlay() {
            let (identity, file) =
                CatalogMeta::require_pair(&meta.base_version, &meta.base_file, "base")?;
            (
                self.load_base_layer(identity, file, base_status(identity))?,
                identity.to_string(),
                file.to_string(),
            )
        } else {
            let catalog = self.active_pricing_catalog()?;
            let identity = catalog.version.clone();
            let file = self.persist_base_document(&catalog)?;
            (catalog, identity, file)
        };
        base.set_runtime_identity(base_identity.clone(), base_status(&base_identity));

        let mut effective = base.merge_overlay(overlay_document.clone())?;
        let effective_json = effective.document().canonical_json()?;
        let effective_digest = hash_string(&effective_json);
        let effective_identity = format!("effective-{effective_digest}");
        let effective_file = format!("{effective_identity}.json");
        effective.set_runtime_identity(effective_identity.clone(), PricingStatus::Snapshot);

        self.persist_catalog_file(&base_file, &base.document().canonical_json()?)?;
        self.persist_catalog_file(&overlay_file, &overlay_json)?;
        self.persist_catalog_file(&effective_file, &effective_json)?;

        let activation = PricingMetaChange::overlay(
            &effective_identity,
            &effective_file,
            &base_identity,
            &base_file,
            &overlay_identity,
            &overlay_file,
        );
        let updated_events = self.recompute_costs_with_meta(&effective, &activation)?;

        Ok(CatalogApplyResult {
            base: layer_from_catalog(&base_identity, Some(&base_file), "base", &base),
            overlay: layer_from_document(
                &overlay_identity,
                Some(&overlay_file),
                "overlay",
                &overlay_document,
            ),
            effective: layer_from_catalog(
                &effective_identity,
                Some(&effective_file),
                "effective",
                &effective,
            ),
            updated_events,
        })
    }

    /// Activates a complete base snapshot. This is the shared implementation
    /// behind the legacy `doctor --refresh-pricing` entrypoint.
    pub fn activate_pricing_snapshot(&self, source_path: &Path) -> Result<CatalogResetResult> {
        validate_local_file(source_path, "pricing snapshot")?;
        let mut catalog = PricingCatalog::load_snapshot(source_path)?;
        let canonical = catalog.document().canonical_json()?;
        let digest = hash_string(&canonical);
        let identity = catalog.declared_version().to_string();
        let file = format!("base-{digest}.json");
        self.persist_catalog_file(&file, &canonical)?;
        catalog.set_runtime_identity(identity.clone(), PricingStatus::Snapshot);

        let removed_overlay = self.pricing_meta()?.has_overlay();
        let activation = PricingMetaChange::active(&identity, Some(&file));
        let updated_events = self.recompute_costs_with_meta(&catalog, &activation)?;
        Ok(CatalogResetResult {
            effective: layer_from_catalog(&identity, Some(&file), "base", &catalog),
            updated_events,
            removed_overlay,
        })
    }

    /// Removes the active overlay. A snapshot base remains pinned; an embedded
    /// base returns to the current binary's embedded catalog.
    pub fn reset_pricing_catalog(&self) -> Result<CatalogResetResult> {
        let meta = self.pricing_meta()?;
        if meta.has_overlay() {
            let (base_identity, base_file) =
                CatalogMeta::require_pair(&meta.base_version, &meta.base_file, "base")?;
            if base_identity.starts_with("static-") {
                let catalog = PricingCatalog::embedded().clone();
                let activation = PricingMetaChange::active(&catalog.version, None);
                let updated_events = self.recompute_costs_with_meta(&catalog, &activation)?;
                return Ok(CatalogResetResult {
                    effective: layer_from_catalog(&catalog.version, None, "base", &catalog),
                    updated_events,
                    removed_overlay: true,
                });
            }

            let catalog =
                self.load_base_layer(base_identity, base_file, PricingStatus::Snapshot)?;
            let activation = PricingMetaChange::active(base_identity, Some(base_file));
            let updated_events = self.recompute_costs_with_meta(&catalog, &activation)?;
            return Ok(CatalogResetResult {
                effective: layer_from_catalog(base_identity, Some(base_file), "base", &catalog),
                updated_events,
                removed_overlay: true,
            });
        }

        let active_identity = meta
            .active_version
            .as_deref()
            .unwrap_or(PricingCatalog::embedded().version.as_str());
        if active_identity.starts_with("static-")
            && active_identity != PricingCatalog::embedded().version
        {
            let catalog = PricingCatalog::embedded().clone();
            let activation = PricingMetaChange::active(&catalog.version, None);
            let updated_events = self.recompute_costs_with_meta(&catalog, &activation)?;
            return Ok(CatalogResetResult {
                effective: layer_from_catalog(&catalog.version, None, "base", &catalog),
                updated_events,
                removed_overlay: false,
            });
        }

        let catalog = self.active_pricing_catalog()?;
        Ok(CatalogResetResult {
            effective: layer_from_catalog(
                &catalog.version,
                meta.active_file.as_deref(),
                "base",
                &catalog,
            ),
            updated_events: 0,
            removed_overlay: false,
        })
    }

    pub fn pricing_catalog_status(&self) -> Result<PricingCatalogStatus> {
        let meta = self.pricing_meta()?;
        let effective_catalog = self.active_pricing_catalog()?;
        let effective_identity = meta
            .active_version
            .as_deref()
            .unwrap_or(effective_catalog.version.as_str());
        let effective_kind = if meta.has_overlay() {
            "effective"
        } else {
            "base"
        };
        let effective = layer_from_catalog(
            effective_identity,
            meta.active_file.as_deref(),
            effective_kind,
            &effective_catalog,
        );

        if !meta.has_overlay() {
            return Ok(PricingCatalogStatus {
                base: effective.clone(),
                overlay: None,
                effective,
                rebase_available: false,
            });
        }

        let (base_identity, base_file) =
            CatalogMeta::require_pair(&meta.base_version, &meta.base_file, "base")?;
        let base_catalog =
            self.load_base_layer(base_identity, base_file, base_status(base_identity))?;
        let (overlay_identity, overlay_file) =
            CatalogMeta::require_pair(&meta.overlay_version, &meta.overlay_file, "overlay")?;
        let overlay_document = self.load_overlay_layer(overlay_identity, overlay_file)?;

        Ok(PricingCatalogStatus {
            base: layer_from_catalog(base_identity, Some(base_file), "base", &base_catalog),
            overlay: Some(layer_from_document(
                overlay_identity,
                Some(overlay_file),
                "overlay",
                &overlay_document,
            )),
            effective,
            rebase_available: base_identity.starts_with("static-")
                && base_identity != PricingCatalog::embedded().version,
        })
    }

    pub(super) fn upgrade_embedded_pricing_if_needed(
        &self,
        progress_sink: Option<BootstrapProgressSink<'_>>,
    ) -> Result<()> {
        let meta = self.pricing_meta()?;
        if meta.has_overlay() || meta.active_file.is_some() {
            return Ok(());
        }
        let Some(active) = meta.active_version.as_deref() else {
            return Ok(());
        };
        if active.starts_with("static-") && active != PricingCatalog::embedded().version {
            let catalog = PricingCatalog::embedded().clone();
            let activation = PricingMetaChange::active(&catalog.version, None);
            self.recompute_costs_with_meta_and_progress(&catalog, &activation, progress_sink)?;
        }
        Ok(())
    }

    fn pricing_meta(&self) -> Result<CatalogMeta> {
        let conn = self.open_connection()?;
        let read = |key: &str| -> Result<Option<String>> {
            Ok(conn
                .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                    row.get(0)
                })
                .optional()?)
        };
        Ok(CatalogMeta {
            active_version: read(META_ACTIVE_VERSION)?,
            active_file: read(META_ACTIVE_FILE)?,
            base_version: read(META_BASE_VERSION)?,
            base_file: read(META_BASE_FILE)?,
            overlay_version: read(META_OVERLAY_VERSION)?,
            overlay_file: read(META_OVERLAY_FILE)?,
        })
    }

    fn persist_base_document(&self, catalog: &PricingCatalog) -> Result<String> {
        let canonical = catalog.document().canonical_json()?;
        let file = format!("base-{}.json", hash_string(&canonical));
        self.persist_catalog_file(&file, &canonical)?;
        Ok(file)
    }

    fn persist_catalog_file(&self, relative: &str, contents: &str) -> Result<()> {
        let target = self.catalog_path(relative)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if target.exists() {
            let existing = fs::read_to_string(&target)?;
            if existing == contents {
                return Ok(());
            }
            return Err(config_invalid(format!(
                "pricing catalog digest collision at {}",
                target.display()
            )));
        }

        let temp = target.with_extension(format!("tmp-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        if let Err(error) = (|| -> std::io::Result<()> {
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temp, &target)?;
            Ok(())
        })() {
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }
        Ok(())
    }

    fn load_base_layer(
        &self,
        identity: &str,
        relative: &str,
        status: PricingStatus,
    ) -> Result<PricingCatalog> {
        let path = self.catalog_path(relative)?;
        let raw = fs::read_to_string(&path)?;
        let content_addressed = verify_content_digest(relative, &raw)?;
        let mut catalog = PricingCatalog::load_snapshot(&path)?;
        let is_effective = Path::new(relative)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("effective-"));
        if (!content_addressed || !is_effective) && catalog.declared_version() != identity {
            return Err(config_invalid(format!(
                "pricing catalog metadata expects `{identity}` but file declares `{}`",
                catalog.declared_version()
            )));
        }
        catalog.set_runtime_identity(identity.to_string(), status);
        Ok(catalog)
    }

    fn load_overlay_layer(
        &self,
        identity: &str,
        relative: &str,
    ) -> Result<crate::query::pricing_catalog::CatalogDocument> {
        let path = self.catalog_path(relative)?;
        let raw = fs::read_to_string(&path)?;
        if !verify_content_digest(relative, &raw)? {
            return Err(config_invalid(format!(
                "pricing overlay `{identity}` is not content-addressed"
            )));
        }
        let document = PricingCatalog::load_overlay(&path)?;
        if identity != document.version {
            return Err(config_invalid(format!(
                "pricing overlay metadata expects `{identity}` but file declares `{}`",
                document.version
            )));
        }
        Ok(document)
    }

    fn catalog_path(&self, relative: &str) -> Result<PathBuf> {
        let relative = Path::new(relative);
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(config_invalid(format!(
                "pricing catalog file must be a safe relative path: {}",
                relative.display()
            )));
        }
        Ok(self.paths.root_dir.join("pricing").join(relative))
    }
}

fn layer_from_catalog(
    identity: &str,
    file: Option<&str>,
    kind: &str,
    catalog: &PricingCatalog,
) -> CatalogLayerStatus {
    layer_from_document(identity, file, kind, catalog.document())
}

fn layer_from_document(
    identity: &str,
    file: Option<&str>,
    kind: &str,
    document: &crate::query::pricing_catalog::CatalogDocument,
) -> CatalogLayerStatus {
    CatalogLayerStatus {
        identity: identity.to_string(),
        version: document.version.clone(),
        kind: kind.to_string(),
        file: file.map(str::to_string),
        schema_version: document.schema_version,
        model_count: document.model_count(),
        source_rule_count: document.source_rule_count(),
    }
}

fn base_status(identity: &str) -> PricingStatus {
    if identity.starts_with("static-") {
        PricingStatus::Static
    } else {
        PricingStatus::Snapshot
    }
}

fn validate_local_file(path: &Path, label: &str) -> Result<()> {
    if let Some(raw) = path.to_str()
        && (raw.starts_with("http://") || raw.starts_with("https://"))
    {
        return Err(config_invalid(format!(
            "{label} must be a local file; URLs are not supported"
        )));
    }
    if !path.is_file() {
        return Err(config_invalid(format!(
            "{label} path does not exist or is not a file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn verify_content_digest(relative: &str, raw: &str) -> Result<bool> {
    let Some(file_name) = Path::new(relative)
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return Ok(false);
    };
    let digest = ["base-", "effective-", "overlay-"]
        .iter()
        .find_map(|prefix| {
            file_name
                .strip_prefix(prefix)
                .and_then(|value| value.strip_suffix(".json"))
        });
    let Some(digest) = digest else {
        return Ok(false);
    };
    if digest.len() != 64 || !digest.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(config_invalid(format!(
            "invalid content-addressed pricing filename `{relative}`"
        )));
    }
    let actual = hash_string(raw);
    if actual != digest {
        return Err(config_invalid(format!(
            "pricing catalog digest mismatch for `{relative}`: expected {digest}, got {actual}"
        )));
    }
    Ok(true)
}

fn config_invalid(detail: impl Into<String>) -> LlmusageError {
    LlmusageError::ConfigInvalid {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::params;
    use tempfile::TempDir;

    use super::*;
    use crate::{paths::AppPaths, store::BootstrapProgressEvent};

    fn test_store(temp: &TempDir) -> anyhow::Result<Store> {
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        Ok(store)
    }

    fn write_overlay(temp: &TempDir, version: &str, model_id: &str) -> anyhow::Result<PathBuf> {
        let path = temp.path().join(format!("{version}.json"));
        fs::write(
            &path,
            format!(
                r#"{{
  "schema_version": 2,
  "kind": "overlay",
  "version": "{version}",
  "models": [
    {{
      "id": "{model_id}",
      "sources": ["codex"],
      "matches": [{{ "value": "{model_id}", "mode": "exact" }}],
      "rates": {{
        "default": {{
          "input_per_mtok": 3.0,
          "cached_per_mtok": 0.3,
          "cache_creation_per_mtok": 3.75,
          "output_per_mtok": 18.0
        }}
      }},
      "context_window": 900000
    }}
  ]
}}"#
            ),
        )?;
        Ok(path)
    }

    fn write_snapshot(temp: &TempDir) -> anyhow::Result<PathBuf> {
        let path = temp.path().join("snapshot.json");
        fs::write(
            &path,
            r#"{
  "schema_version": 2,
  "kind": "base",
  "version": "private-pricing-1",
  "models": [
    {
      "id": "private-model",
      "sources": ["codex"],
      "matches": [{ "value": "private-model", "mode": "exact" }],
      "rates": {
        "default": {
          "input_per_mtok": 7.0,
          "cached_per_mtok": 0.7,
          "output_per_mtok": 21.0
        }
      }
    }
  ]
}"#,
        )?;
        Ok(path)
    }

    #[test]
    fn catalog_overlay_apply_survives_restart_and_reset_restores_embedded() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let overlay = write_overlay(&temp, "team-models-1", "team-model")?;

        let applied = store.apply_pricing_overlay(&overlay)?;
        assert_eq!(applied.base.identity, "static-v2");
        assert!(
            applied
                .base
                .file
                .as_deref()
                .is_some_and(|file| file.starts_with("base-"))
        );
        assert_eq!(applied.overlay.version, "team-models-1");
        assert_eq!(applied.effective.kind, "effective");
        assert!(applied.effective.identity.starts_with("effective-"));

        store.recompute_costs()?;
        assert_eq!(
            store
                .pricing_catalog_status()?
                .overlay
                .as_ref()
                .map(|layer| layer.version.as_str()),
            Some("team-models-1"),
            "recomputing the active catalog must preserve overlay metadata"
        );

        let restarted = test_store(&temp)?;
        let active = restarted.active_pricing_catalog()?;
        assert!(active.find("codex", "team-model").is_some());
        assert_eq!(active.version, applied.effective.identity);
        let status = restarted.pricing_catalog_status()?;
        assert_eq!(status.base.identity, "static-v2");
        assert_eq!(
            status.overlay.as_ref().map(|layer| layer.version.as_str()),
            Some("team-models-1")
        );
        assert!(!status.rebase_available);

        let reset = restarted.reset_pricing_catalog()?;
        assert!(reset.removed_overlay);
        assert_eq!(reset.effective.identity, "static-v2");
        assert!(
            restarted
                .active_pricing_catalog()?
                .find("codex", "team-model")
                .is_none()
        );
        assert!(restarted.pricing_catalog_status()?.overlay.is_none());
        Ok(())
    }

    #[test]
    fn recompute_with_snapshot_persists_catalog_for_restart() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let snapshot = write_snapshot(&temp)?;
        let catalog = PricingCatalog::load_snapshot(&snapshot)?;

        store.recompute_costs_with(&catalog)?;

        let restarted = test_store(&temp)?;
        let active = restarted.active_pricing_catalog()?;
        assert_eq!(active.version, "private-pricing-1");
        assert!(active.find("codex", "private-model").is_some());
        assert!(restarted.pricing_catalog_status()?.base.file.is_some());
        Ok(())
    }

    #[test]
    fn recompute_with_custom_static_catalog_persists_catalog_for_restart() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let snapshot = write_snapshot(&temp)?;
        let mut catalog = PricingCatalog::load_snapshot(&snapshot)?;
        catalog.status = PricingStatus::Static;

        store.recompute_costs_with(&catalog)?;

        let restarted = test_store(&temp)?;
        let active = restarted.active_pricing_catalog()?;
        assert_eq!(active.version, "private-pricing-1");
        assert!(active.find("codex", "private-model").is_some());
        Ok(())
    }

    #[test]
    fn paged_recompute_can_retry_after_a_later_page_fails() -> anyhow::Result<()> {
        const EVENT_COUNT: usize = 5_001;

        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let snapshot = write_snapshot(&temp)?;
        let catalog = PricingCatalog::load_snapshot(&snapshot)?;

        let mut conn = store.open_connection()?;
        let tx = conn.transaction()?;
        {
            let mut insert = tx.prepare(
                r#"
                INSERT INTO usage_event(
                    event_key, source, model, event_at, hour_start,
                    input_tokens, cache_read_tokens, cache_creation_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens, created_at
                ) VALUES (?1, 'codex', 'private-model', ?2, ?2, 1, 0, 0, 0, 0, 1, ?2)
                "#,
            )?;
            for index in 0..EVENT_COUNT {
                insert.execute(params![
                    format!("paged-recompute-{index:05}"),
                    "2026-07-10T00:00:00Z"
                ])?;
            }
        }
        tx.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, provider_label, model, hour_start, project_hash,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                event_count, updated_at
            ) VALUES (
                'codex', '', 'private-model', ?1, '',
                ?2, 0, 0, 0, 0, ?2, ?2, ?1
            )
            "#,
            params!["2026-07-10T00:00:00Z", EVENT_COUNT as i64],
        )?;
        tx.commit()?;
        conn.execute_batch(
            r#"
            CREATE TRIGGER fail_second_recompute_page
            BEFORE UPDATE OF cost_with_cache_usd ON usage_event
            WHEN NEW.event_key = 'paged-recompute-05000'
            BEGIN
                SELECT RAISE(FAIL, 'injected second-page failure');
            END;
            "#,
        )?;
        drop(conn);

        let activation = PricingMetaChange::for_catalog(&store, &catalog)?;
        let mut progress_events = Vec::new();
        let mut progress_sink = |event| progress_events.push(event);
        let error = store
            .recompute_costs_with_meta_and_progress(&catalog, &activation, Some(&mut progress_sink))
            .expect_err("the injected second-page failure must abort recomputation");
        assert!(error.to_string().contains("injected second-page failure"));
        assert!(matches!(
            progress_events.first(),
            Some(BootstrapProgressEvent::PricingUpgradeStarted { .. })
        ));
        assert!(
            !progress_events.iter().any(|event| matches!(
                event,
                BootstrapProgressEvent::PricingUpgradeFinished { .. }
            )),
            "a failed final activation must not emit a finished event"
        );
        let conn = store.open_connection()?;
        let priced_after_failure: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE pricing_source = 'private-pricing-1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(priced_after_failure, 5_000);
        assert_eq!(
            store.meta_value(META_ACTIVE_VERSION)?.as_deref(),
            Some("static-v2"),
            "activation metadata must not switch before bucket reconciliation"
        );
        conn.execute_batch("DROP TRIGGER fail_second_recompute_page;")?;
        drop(conn);

        assert_eq!(store.recompute_costs_with(&catalog)?, EVENT_COUNT);
        let conn = store.open_connection()?;
        let priced_after_retry: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE pricing_source = 'private-pricing-1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(priced_after_retry, EVENT_COUNT as i64);
        let (bucket_events, bucket_cost): (i64, f64) = conn.query_row(
            r#"
            SELECT event_count, cost_with_cache_usd
            FROM usage_bucket_30m
            WHERE source = 'codex' AND model = 'private-model'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(bucket_events, EVENT_COUNT as i64);
        assert!((bucket_cost - EVENT_COUNT as f64 * 7.0 / 1_000_000.0).abs() < 1e-12);
        assert_eq!(
            store.meta_value(META_ACTIVE_VERSION)?.as_deref(),
            Some("private-pricing-1")
        );
        Ok(())
    }

    #[test]
    fn repeated_overlay_apply_uses_recorded_base_not_previous_effective() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let first = write_overlay(&temp, "team-models-1", "first-private-model")?;
        store.apply_pricing_overlay(&first)?;

        let second = write_overlay(&temp, "team-models-2", "second-private-model")?;
        store.apply_pricing_overlay(&second)?;
        let active = store.active_pricing_catalog()?;
        assert!(active.find("codex", "second-private-model").is_some());
        assert!(active.find("codex", "first-private-model").is_none());
        Ok(())
    }

    #[test]
    fn selected_catalog_digest_mismatch_fails_closed() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let overlay = write_overlay(&temp, "team-models-1", "team-model")?;
        let applied = store.apply_pricing_overlay(&overlay)?;
        let effective_file = applied.effective.file.expect("effective file");
        fs::write(temp.path().join("pricing").join(effective_file), "{}")?;

        let error = store
            .active_pricing_catalog()
            .expect_err("tampered active catalog must not fall back to embedded");
        assert!(error.to_string().contains("digest mismatch"), "{error}");
        Ok(())
    }

    #[test]
    fn overlay_on_snapshot_resets_to_pinned_snapshot() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let snapshot = write_snapshot(&temp)?;
        let activated = store.activate_pricing_snapshot(&snapshot)?;
        let snapshot_identity = activated.effective.identity.clone();

        let overlay = write_overlay(&temp, "team-models-1", "team-model")?;
        store.apply_pricing_overlay(&overlay)?;
        let reset = store.reset_pricing_catalog()?;
        assert_eq!(reset.effective.identity, snapshot_identity);
        let active = store.active_pricing_catalog()?;
        assert!(active.find("codex", "private-model").is_some());
        assert!(active.find("codex", "team-model").is_none());
        Ok(())
    }

    #[test]
    fn activating_snapshot_clears_existing_overlay_metadata() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let overlay = write_overlay(&temp, "team-models-1", "team-model")?;
        store.apply_pricing_overlay(&overlay)?;

        let snapshot = write_snapshot(&temp)?;
        let activated = store.activate_pricing_snapshot(&snapshot)?;
        assert!(activated.removed_overlay);
        let status = store.pricing_catalog_status()?;
        assert!(status.overlay.is_none());
        assert_eq!(status.base.version, "private-pricing-1");
        assert!(store.meta_value(META_BASE_VERSION)?.is_none());
        assert!(store.meta_value(META_OVERLAY_VERSION)?.is_none());
        Ok(())
    }

    #[test]
    fn bootstrap_upgrades_unpinned_old_static_catalog() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        store.set_meta_value(META_ACTIVE_VERSION, "static-v1")?;
        store.bootstrap()?;
        assert_eq!(
            store.meta_value(META_ACTIVE_VERSION)?.as_deref(),
            Some("static-v2")
        );
        Ok(())
    }

    #[test]
    fn pricing_progress_reports_ordered_upgrade_lifecycle_and_noop_is_silent() -> anyhow::Result<()>
    {
        let temp = TempDir::new()?;
        let store = test_store(&temp)?;
        let conn = store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens, created_at
            ) VALUES ('pricing-progress', 'codex', 'gpt-5', ?1, ?1,
                      100, 0, 0, 10, 0, 110, ?1)
            "#,
            ["2026-07-16T00:00:00Z"],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, provider_label, model, hour_start, project_hash,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                event_count, updated_at
            ) VALUES ('codex', '', 'gpt-5', ?1, '', 100, 0, 0, 10, 0, 110, 1, ?1)
            "#,
            ["2026-07-16T00:00:00Z"],
        )?;
        drop(conn);
        store.set_meta_value(META_ACTIVE_VERSION, "static-v1")?;

        let mut events = Vec::new();
        let mut sink = |event| events.push(event);
        store.bootstrap_with_progress(Some(&mut sink))?;

        assert!(matches!(
            events.as_slice(),
            [
                BootstrapProgressEvent::PricingUpgradeStarted {
                    from_version,
                    to_version,
                    total_events: 1,
                },
                BootstrapProgressEvent::PricingUpgradeProgress {
                    from_version: progress_from,
                    to_version: progress_to,
                    processed_events: 1,
                    total_events: 1,
                    ..
                },
                BootstrapProgressEvent::PricingBucketReconcileStarted {
                    to_version: reconcile_to,
                    bucket_count: 1,
                },
                BootstrapProgressEvent::PricingUpgradeFinished {
                    from_version: finished_from,
                    to_version: finished_to,
                    updated_events: 1,
                    bucket_count: 1,
                    deleted_orphan_buckets: 0,
                    ..
                }
            ] if from_version == "static-v1"
                && to_version == "static-v2"
                && progress_from == from_version
                && progress_to == to_version
                && reconcile_to == to_version
                && finished_from == from_version
                && finished_to == to_version
        ));
        assert_eq!(
            store.meta_value(META_ACTIVE_VERSION)?.as_deref(),
            Some("static-v2")
        );

        let mut noop_events = Vec::new();
        let mut noop_sink = |event| noop_events.push(event);
        store.bootstrap_with_progress(Some(&mut noop_sink))?;
        assert!(noop_events.is_empty());

        store.set_meta_value(META_ACTIVE_VERSION, "static-v1")?;
        store.set_meta_value(META_ACTIVE_FILE, "pinned-snapshot.json")?;
        let mut pinned_events = Vec::new();
        let mut pinned_sink = |event| pinned_events.push(event);
        store.bootstrap_with_progress(Some(&mut pinned_sink))?;
        assert!(pinned_events.is_empty());
        Ok(())
    }
}
