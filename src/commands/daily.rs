use anyhow::{Result, bail};
use tracing::debug;

use crate::{app::AppContext, query::reports, store::Store, tui::report_table};

use super::{report_args::DailyArgs, unified_report};

pub async fn run(app: &AppContext, args: DailyArgs) -> Result<()> {
    debug!("starting daily report output");
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let mut filter = args.common.to_filter(args.project.clone())?;
    if args.all && (filter.since.is_some() || filter.until.is_some()) {
        bail!("--all cannot be combined with --since or --until");
    }
    if args.instances && !args.unified.sections.is_empty() {
        bail!("--sections cannot be combined with --instances");
    }

    if !args.instances && !args.unified.sections.is_empty() {
        let reports = unified_report::load_sections(
            &store,
            &filter,
            reports::PeriodKind::Daily,
            &args.unified.sections,
            args.all,
        )?;
        unified_report::print_sections(
            &reports,
            reports::PeriodKind::Daily,
            args.common.json,
            args.unified.by_agent,
            args.common.compact,
            args.common.no_cost,
        )?;
        debug!("finished daily report output");
        return Ok(());
    }

    if !args.all {
        unified_report::apply_daily_default(&mut filter);
    }

    if args.instances {
        let report = reports::load_daily_project_report(&store, &filter)?;
        if args.common.json {
            let mut report = serde_json::to_value(&report)?;
            if args.common.no_cost {
                unified_report::strip_cost_json(&mut report);
            }
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            let mut rows = report
                .projects
                .values()
                .flat_map(|items| items.iter().cloned())
                .collect::<Vec<_>>();
            rows.sort_by(|left, right| {
                let left_key = (left.date.clone(), left.project.clone());
                let right_key = (right.date.clone(), right.project.clone());
                if matches!(filter.order, reports::SortOrder::Desc) {
                    right_key.cmp(&left_key)
                } else {
                    left_key.cmp(&right_key)
                }
            });
            println!("Daily usage by project");
            println!(
                "{}",
                report_table::render_daily_table(
                    &rows,
                    Some(&report.totals),
                    args.common.compact,
                    true,
                    args.common.no_cost
                )
            );
        }
    } else {
        if args.common.json {
            let report = reports::load_unified_report(&store, &filter, reports::PeriodKind::Daily)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&unified_report::report_json(
                    &report,
                    args.unified.by_agent,
                    args.common.no_cost
                )?)?
            );
        } else {
            let report = reports::load_unified_report(&store, &filter, reports::PeriodKind::Daily)?;
            let color_mode = report_table::ColorMode::from_env();
            println!(
                "{}",
                report_table::render_unified_table(
                    &report,
                    args.common.compact,
                    args.common.no_cost,
                    color_mode
                )
            );
        }
    }

    debug!("finished daily report output");
    Ok(())
}
