use crate::common::i18n::commands::{
    DiagnosticFacts as Facts, reports::DiagnosticFacts as ReportFacts,
};
use std::{
    fs,
    io::{self, IsTerminal, Read},
};

use anyhow::Result;
use serde_json::Value;
use tracing::debug;

use crate::{
    app::AppContext,
    common::i18n::{commands::reports::Message as Msg, current},
    query::{
        Dashboard,
        reports::{self, ReportTimezone},
    },
    store::Store,
    tui::report_table,
};

use super::{
    cli_errors::Diagnostic,
    report_args::{CostSourceArg, StatuslineArgs},
};

pub async fn run(app: &AppContext, args: StatuslineArgs) -> Result<()> {
    debug!(
        event_code = "commands.statusline.01",
        "starting statusline output"
    );
    let hook_input = read_hook_input()?;
    let store = Store::new(&app.paths)?;
    store.require_initialized()?;
    let cache_path = app
        .paths
        .root_dir
        .join("statusline-cache")
        .join(format!("latest.{}.txt", current().code()));

    let dashboard = Dashboard::open(&store)?;
    let line = match reports::load_statusline_summary(dashboard.connection(), ReportTimezone::Local)
    {
        Ok(summary) => {
            let active = summary
                .active_block
                .as_ref()
                .map(|block| {
                    format!(
                        "{} {} {} {}",
                        Msg::StatusBlock.text(),
                        report_table::format_count(block.totals.total_tokens),
                        Msg::StatusProjected.text(),
                        report_table::format_count(block.projected_total_tokens)
                    )
                })
                .unwrap_or_else(|| Msg::NoActiveBlock.text().to_string());
            let cost = format_statusline_cost(args.cost_source, &summary.today, &hook_input);
            format!(
                "{} | {} {} {} | {}",
                hook_input
                    .model
                    .clone()
                    .unwrap_or_else(|| "llmusage".to_string()),
                Msg::Today.text(),
                report_table::format_count(summary.today.total_tokens),
                cost,
                active
            )
        }
        Err(err) if args.use_cache() => fs::read_to_string(&cache_path)
            .unwrap_or_else(|_| format!("llmusage | {}: {err}", Msg::Unavailable.text())),
        Err(err) => format!("llmusage | {}: {err}", Msg::Unavailable.text()),
    };
    let line = line.replace(['\r', '\n'], " ");

    println!("{line}");
    if args.use_cache() {
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| statusline_io(error, ReportFacts::StatuslineCacheDirectory {}))?;
        }
        fs::write(cache_path, &line)
            .map_err(|error| statusline_io(error, ReportFacts::StatuslineCacheWrite {}))?;
    }

    debug!(
        event_code = "commands.statusline.02",
        "finished statusline output"
    );
    Ok(())
}

fn statusline_io(error: io::Error, facts: ReportFacts) -> anyhow::Error {
    let canonical = error.to_string();
    anyhow::Error::new(error).context(Diagnostic::with_facts(canonical, Facts::Reports(facts)))
}

fn format_statusline_cost(
    cost_source: CostSourceArg,
    today: &reports::TokenTotals,
    hook_input: &HookInput,
) -> String {
    let db_cost = report_table::format_cost(today.estimated_cost_usd);
    let hook_cost = hook_input.cost_usd.map(report_table::format_cost);
    match cost_source {
        CostSourceArg::Llmusage => db_cost,
        CostSourceArg::Hook => hook_cost.unwrap_or_else(|| Msg::HookUnavailable.text().to_string()),
        CostSourceArg::Both => hook_cost
            .map(|value| format!("{} {db_cost} {} {value}", Msg::Db.text(), Msg::Hook.text()))
            .unwrap_or_else(|| {
                format!(
                    "{} {db_cost} {}",
                    Msg::Db.text(),
                    Msg::HookUnavailable.text()
                )
            }),
        CostSourceArg::Auto => hook_cost
            .map(|value| format!("{} {db_cost} {} {value}", Msg::Db.text(), Msg::Hook.text()))
            .unwrap_or(db_cost),
    }
}

#[derive(Debug, Default)]
struct HookInput {
    model: Option<String>,
    cost_usd: Option<f64>,
}

fn read_hook_input() -> Result<HookInput> {
    let mut stdin = io::stdin();
    if stdin.is_terminal() {
        return Ok(HookInput::default());
    }
    let mut raw = String::new();
    stdin
        .read_to_string(&mut raw)
        .map_err(|error| statusline_io(error, ReportFacts::StatuslineRead {}))?;
    if raw.trim().is_empty() {
        return Ok(HookInput::default());
    }
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Ok(HookInput::default());
    };
    Ok(HookInput {
        model: find_model_value(&value),
        cost_usd: find_cost_value(&value),
    })
}

fn find_model_value(value: &Value) -> Option<String> {
    value
        .get("model")
        .and_then(Value::as_str)
        .or_else(|| value.get("model_name").and_then(Value::as_str))
        .or_else(|| {
            value
                .get("workspace")
                .and_then(|workspace| workspace.get("model"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn find_cost_value(value: &Value) -> Option<f64> {
    match value {
        Value::Object(object) => {
            for key in [
                "costUSD",
                "costUsd",
                "cost_usd",
                "total_cost_usd",
                "session_cost_usd",
            ] {
                if let Some(cost) = object.get(key).and_then(Value::as_f64) {
                    return Some(cost);
                }
            }
            object
                .iter()
                .filter(|(key, _)| key.to_ascii_lowercase().contains("cost"))
                .find_map(|(_, value)| value.as_f64())
                .or_else(|| object.values().find_map(find_cost_value))
        }
        Value::Array(items) => items.iter().find_map(find_cost_value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::i18n::{Language, with_language};

    #[test]
    fn statusline_io_preserves_original_display_and_typed_source_with_paired_context() {
        for language in [Language::English, Language::Chinese] {
            for facts in [
                ReportFacts::StatuslineRead {},
                ReportFacts::StatuslineCacheDirectory {},
                ReportFacts::StatuslineCacheWrite {},
            ] {
                with_language(language, || {
                    let error = statusline_io(
                        io::Error::new(io::ErrorKind::PermissionDenied, "external OS detail"),
                        facts.clone(),
                    );
                    assert_eq!(error.to_string(), "external OS detail");
                    assert_eq!(
                        error.downcast_ref::<io::Error>().unwrap().kind(),
                        io::ErrorKind::PermissionDenied
                    );
                    let human = crate::commands::cli_errors::application_error(&error, language);
                    assert!(
                        human.contains(&facts.render(language).unwrap())
                            && human.contains("external OS detail"),
                        "{human}"
                    );
                });
            }
        }
    }
}
