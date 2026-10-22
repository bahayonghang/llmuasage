use super::cli_errors::pricing_result;
use crate::common::i18n::commands::message;
use std::path::Path;

use anyhow::Result;

use crate::{app::AppContext, store::Store};

pub async fn apply(app: &AppContext, path: &Path) -> Result<()> {
    let store = initialized_store(app)?;
    let result = pricing_result(store.apply_pricing_overlay_with_facts(path))?;
    println!("{}", message!(CatalogApplied));
    println!(
        "{}",
        message!(
            CatalogBase,
            result.base.version,
            result.base.identity,
            result.base.model_count
        )
    );
    println!(
        "{}",
        message!(
            CatalogOverlay,
            result.overlay.version,
            result.overlay.model_count
        )
    );
    println!(
        "{}",
        message!(
            CatalogEffective,
            result.effective.identity,
            result.effective.model_count,
            result.effective.source_rule_count
        )
    );
    println!("{}", message!(CatalogRecomputed, result.updated_events));
    Ok(())
}

pub async fn status(app: &AppContext, json: bool) -> Result<()> {
    let store = Store::new(&app.paths)?;
    store.require_initialized()?;
    let status = pricing_result(store.pricing_catalog_status_with_facts())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    println!("{}", message!(CatalogTitle));
    print_layer(crate::common::i18n::text("base", "基础"), &status.base);
    if let Some(overlay) = &status.overlay {
        print_layer(crate::common::i18n::text("overlay", "覆盖"), overlay);
    } else {
        println!("{}", message!(CatalogNoOverlay));
    }
    print_layer(
        crate::common::i18n::text("effective", "生效"),
        &status.effective,
    );
    println!(
        "{}",
        message!(
            CatalogRebase,
            if status.rebase_available {
                crate::common::i18n::text("yes", "是")
            } else {
                crate::common::i18n::text("no", "否")
            }
        )
    );
    Ok(())
}

pub async fn reset(app: &AppContext) -> Result<()> {
    let store = initialized_store(app)?;
    let result = pricing_result(store.reset_pricing_catalog_with_facts())?;
    println!("{}", message!(CatalogReset));
    println!(
        "{}",
        message!(
            CatalogEffectiveVersion,
            result.effective.version,
            result.effective.identity,
            result.effective.model_count
        )
    );
    println!(
        "{}",
        message!(
            CatalogRemoved,
            if result.removed_overlay {
                crate::common::i18n::text("yes", "是")
            } else {
                crate::common::i18n::text("no", "否")
            }
        )
    );
    println!("{}", message!(CatalogRecomputed, result.updated_events));
    Ok(())
}

fn initialized_store(app: &AppContext) -> Result<Store> {
    let store = Store::new(&app.paths)?;
    pricing_result(store.bootstrap_with_pricing_facts())?;
    Ok(store)
}

fn print_layer(label: &str, layer: &crate::store::CatalogLayerStatus) {
    println!(
        "{}",
        message!(
            CatalogLayer,
            label,
            layer.version,
            layer.identity,
            layer.schema_version,
            layer.model_count,
            layer.source_rule_count
        )
    );
    if let Some(file) = &layer.file {
        println!("{}", message!(CatalogFile, file));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LlmusageError,
        common::i18n::Language,
        domain::pricing_catalog::diagnostic::{DiagnosticFacts, Failure},
    };

    #[test]
    fn pricing_adapter_preserves_original_display_variant_and_external_source() {
        for language in [Language::English, Language::Chinese] {
            let error = pricing_result::<()>(Err(Failure::config(
                "original own validator reason",
                DiagnosticFacts::NoLiteRows {},
            )))
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "invalid config: original own validator reason"
            );
            assert!(
                matches!(error.downcast_ref::<LlmusageError>(), Some(LlmusageError::ConfigInvalid { detail }) if detail == "original own validator reason")
            );
            let rendered = super::super::cli_errors::application_error(&error, language);
            assert!(
                rendered.contains(language.select("LiteLLM rows", "LiteLLM 记录")),
                "{rendered}"
            );
            assert!(
                !rendered.contains("original own validator reason"),
                "{rendered}"
            );

            let parse = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
            let canonical = LlmusageError::Parse {
                context: "pricing snapshot",
                source: parse,
            };
            let display = canonical.to_string();
            let error = pricing_result::<()>(Err(canonical.into())).unwrap_err();
            assert_eq!(error.to_string(), display);
            assert!(matches!(
                error.downcast_ref::<LlmusageError>(),
                Some(LlmusageError::Parse { .. })
            ));
            assert!(
                super::super::cli_errors::application_error(&error, language)
                    .contains("EOF while parsing an object")
            );
        }
    }
}
