use std::path::Path;

use anyhow::Result;

use crate::{app::AppContext, store::Store};

pub async fn apply(app: &AppContext, path: &Path) -> Result<()> {
    let store = initialized_store(app)?;
    let result = store.apply_pricing_overlay(path)?;
    println!("Catalog overlay applied:");
    println!(
        "- base: {} ({}, {} models)",
        result.base.version, result.base.identity, result.base.model_count
    );
    println!(
        "- overlay: {} ({} models)",
        result.overlay.version, result.overlay.model_count
    );
    println!(
        "- effective: {} ({} models, {} source rules)",
        result.effective.identity, result.effective.model_count, result.effective.source_rule_count
    );
    println!("- recomputed events: {}", result.updated_events);
    Ok(())
}

pub async fn status(app: &AppContext, json: bool) -> Result<()> {
    let store = initialized_store(app)?;
    let status = store.pricing_catalog_status()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    println!("Pricing catalog:");
    print_layer("base", &status.base);
    if let Some(overlay) = &status.overlay {
        print_layer("overlay", overlay);
    } else {
        println!("- overlay: none");
    }
    print_layer("effective", &status.effective);
    println!(
        "- rebase available: {}",
        if status.rebase_available { "yes" } else { "no" }
    );
    Ok(())
}

pub async fn reset(app: &AppContext) -> Result<()> {
    let store = initialized_store(app)?;
    let result = store.reset_pricing_catalog()?;
    println!("Pricing catalog reset:");
    println!(
        "- effective: {} ({}, {} models)",
        result.effective.version, result.effective.identity, result.effective.model_count
    );
    println!(
        "- overlay removed: {}",
        if result.removed_overlay { "yes" } else { "no" }
    );
    println!("- recomputed events: {}", result.updated_events);
    Ok(())
}

fn initialized_store(app: &AppContext) -> Result<Store> {
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    Ok(store)
}

fn print_layer(label: &str, layer: &crate::store::CatalogLayerStatus) {
    println!(
        "- {label}: {} ({}, schema {}, {} models, {} source rules)",
        layer.version,
        layer.identity,
        layer.schema_version,
        layer.model_count,
        layer.source_rule_count
    );
    if let Some(file) = &layer.file {
        println!("  file: {file}");
    }
}
