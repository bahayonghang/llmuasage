use anyhow::Result;
use tracing::debug;

use crate::{app::AppContext, query::reports, store::Store, tui::report_table};

use super::report_args::MonthlyArgs;

pub async fn run(app: &AppContext, args: MonthlyArgs) -> Result<()> {
    debug!("starting monthly report output");
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let filter = args.common.to_filter(None)?;
    let report = reports::load_monthly_report(&store, &filter)?;

    if args.common.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Monthly usage");
        println!(
            "{}",
            report_table::render_monthly_table(
                &report.monthly,
                Some(&report.totals),
                args.common.compact
            )
        );
    }

    debug!("finished monthly report output");
    Ok(())
}
