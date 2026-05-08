use std::{
    fs,
    io::{self, IsTerminal, Read},
};

use anyhow::Result;
use serde_json::Value;
use tracing::debug;

use crate::{
    app::AppContext,
    query::reports::{self, ReportTimezone},
    store::Store,
    tui::report_table,
};

use super::report_args::{CostSourceArg, StatuslineArgs};

pub async fn run(app: &AppContext, args: StatuslineArgs) -> Result<()> {
    debug!("starting statusline output");
    let hook_input = read_hook_input()?;
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let cache_path = app
        .paths
        .root_dir
        .join("statusline-cache")
        .join("latest.txt");

    let line = match reports::load_statusline_summary(&store, ReportTimezone::Local) {
        Ok(summary) => {
            let active = summary
                .active_block
                .as_ref()
                .map(|block| {
                    format!(
                        "block {} projected {}",
                        report_table::format_count(block.totals.total_tokens),
                        report_table::format_count(block.projected_total_tokens)
                    )
                })
                .unwrap_or_else(|| "no active block".to_string());
            let cost = format_statusline_cost(args.cost_source, &summary.today, &hook_input);
            format!(
                "{} | today {} {} | {}",
                hook_input
                    .model
                    .clone()
                    .unwrap_or_else(|| "llmusage".to_string()),
                report_table::format_count(summary.today.total_tokens),
                cost,
                active
            )
        }
        Err(err) if args.use_cache() => fs::read_to_string(&cache_path)
            .unwrap_or_else(|_| format!("llmusage | unavailable: {err}")),
        Err(err) => format!("llmusage | unavailable: {err}"),
    };

    println!("{line}");
    if args.use_cache() {
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(cache_path, &line)?;
    }

    debug!("finished statusline output");
    Ok(())
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
        CostSourceArg::Hook => hook_cost.unwrap_or_else(|| "hook n/a".to_string()),
        CostSourceArg::Both => hook_cost
            .map(|value| format!("db {db_cost} hook {value}"))
            .unwrap_or_else(|| format!("db {db_cost} hook n/a")),
        CostSourceArg::Auto => hook_cost
            .map(|value| format!("db {db_cost} hook {value}"))
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
    stdin.read_to_string(&mut raw)?;
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
