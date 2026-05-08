use anyhow::Result;
use tracing::debug;

use crate::{app::AppContext, query::reports, store::Store, tui::report_table};

use super::report_args::SessionArgs;

pub async fn run(app: &AppContext, args: SessionArgs) -> Result<()> {
    debug!("starting session report output");
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let filter = args.common.to_filter(args.project.clone())?;

    if let Some(id) = args.id.as_deref() {
        let report = reports::load_single_session_report(&store, &filter, id)?;
        if args.common.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else if let Some(session) = report.session {
            println!("Session usage");
            println!(
                "{}",
                report_table::render_session_table(&[session], None, args.common.compact)
            );
        } else {
            println!("No usage data matched session id `{id}`.");
        }
    } else {
        let report = reports::load_session_report(&store, &filter, None)?;
        if args.common.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("Session usage");
            println!(
                "{}",
                report_table::render_session_table(
                    &report.sessions,
                    Some(&report.totals),
                    args.common.compact
                )
            );
        }
    }

    debug!("finished session report output");
    Ok(())
}
