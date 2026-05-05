use anyhow::{Result, bail};
use tracing::debug;

use crate::{app::AppContext, query::reports, store::Store, tui::report_table};

use super::report_args::DailyArgs;

pub async fn run(app: &AppContext, args: DailyArgs) -> Result<()> {
    debug!("starting daily report output");
    let store = Store::new(&app.paths);
    store.bootstrap()?;
    let mut filter = args.common.to_filter(args.project.clone())?;
    if args.all && (filter.since.is_some() || filter.until.is_some()) {
        bail!("--all cannot be combined with --since or --until");
    }
    if !args.all && filter.since.is_none() && filter.until.is_none() {
        let today = reports::today_for_timezone(&filter.timezone);
        filter.since = Some(today);
        filter.until = Some(today);
    }

    if args.instances {
        let report = reports::load_daily_project_report(&store, &filter)?;
        if args.common.json {
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
                    true
                )
            );
        }
    } else {
        let report = reports::load_daily_report(&store, &filter)?;
        if args.common.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("Daily usage");
            println!(
                "{}",
                report_table::render_daily_table(
                    &report.daily,
                    Some(&report.totals),
                    args.common.compact,
                    false
                )
            );
        }
    }

    debug!("finished daily report output");
    Ok(())
}
