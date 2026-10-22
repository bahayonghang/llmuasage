use crate::common::i18n::commands::message;
use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, store::Store};

pub async fn run(app: &AppContext) -> Result<()> {
    info!(
        event_code = "commands.init.01",
        "bootstrapping local runtime"
    );
    let store = Store::new(&app.paths)?;
    super::cli_errors::pricing_result(store.bootstrap_with_pricing_facts())?;
    super::run_tracked(&store, "init", async { Ok(()) }, |_| {
        Some("local runtime initialized".to_string())
    })
    .await?;

    println!("{}", message!(InitFinished));
    info!(
        event_code = "commands.init.02",
        "local runtime bootstrap completed"
    );
    Ok(())
}
