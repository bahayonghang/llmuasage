use anyhow::Result;
use tracing::debug;

use crate::{app::AppContext, query::reports, store::Store, tui::report_table};

use super::{report_args::SessionArgs, unified_report};

pub async fn run(app: &AppContext, args: SessionArgs) -> Result<()> {
    debug!("starting session report output");
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let filter = args.common.to_filter(args.project.clone())?;

    if args.id.is_some() && !args.unified.sections.is_empty() {
        anyhow::bail!("--sections cannot be combined with --id");
    }
    if !args.unified.sections.is_empty() {
        let reports = unified_report::load_sections(
            &store,
            &filter,
            reports::PeriodKind::Session,
            &args.unified.sections,
            false,
        )?;
        unified_report::print_sections(
            &reports,
            reports::PeriodKind::Session,
            args.common.json,
            args.unified.by_agent,
            args.common.compact,
            args.common.no_cost,
        )?;
        debug!("finished session report output");
        return Ok(());
    }

    let report = reports::load_unified_session_report(&store, &filter, args.id.as_deref())?;
    if args.common.json {
        // Session rows are already source-specific, so --by-agent is deliberately a no-op.
        println!(
            "{}",
            serde_json::to_string_pretty(&unified_report::report_json(
                &report,
                false,
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

    debug!("finished session report output");
    Ok(())
}
