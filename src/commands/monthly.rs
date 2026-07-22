use anyhow::Result;
use tracing::debug;

use crate::{app::AppContext, query::reports, store::Store, tui::report_table};

use super::{report_args::MonthlyArgs, unified_report};

pub async fn run(app: &AppContext, args: MonthlyArgs) -> Result<()> {
    debug!("starting monthly report output");
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let filter = args.common.to_filter(None)?;
    if !args.unified.sections.is_empty() {
        let reports = unified_report::load_sections(
            &store,
            &filter,
            reports::PeriodKind::Monthly,
            &args.unified.sections,
            false,
        )?;
        unified_report::print_sections(
            &reports,
            reports::PeriodKind::Monthly,
            args.common.json,
            args.unified.by_agent,
            args.common.compact,
            args.common.no_cost,
        )?;
        debug!("finished monthly report output");
        return Ok(());
    }
    let report = reports::load_unified_report(&store, &filter, reports::PeriodKind::Monthly)?;

    if args.common.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&unified_report::report_json(
                &report,
                args.unified.by_agent,
                args.common.no_cost
            )?)?
        );
    } else {
        println!(
            "{}",
            report_table::render_unified_table(
                &report,
                args.common.compact,
                args.common.no_cost,
                report_table::ColorMode::from_env()
            )
        );
    }

    debug!("finished monthly report output");
    Ok(())
}
