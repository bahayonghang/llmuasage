use anyhow::Result;
use tracing::debug;

use crate::{app::AppContext, query::reports, store::Store, tui::report_table};

use super::report_args::BlocksArgs;

pub async fn run(app: &AppContext, args: BlocksArgs) -> Result<()> {
    debug!("starting blocks report output");
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let filter = args.common.to_filter(None)?;
    let options = args.to_options();
    let report = reports::load_blocks_report(&store, &filter, &options)?;

    if args.common.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Usage blocks");
        println!(
            "{}",
            report_table::render_blocks_table(&report.blocks, args.common.compact)
        );
    }

    debug!("finished blocks report output");
    Ok(())
}
