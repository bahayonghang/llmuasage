use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, store::Store};

pub async fn run(app: &AppContext) -> Result<()> {
    info!("bootstrapping local runtime");
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    super::run_tracked(&store, "init", async { Ok(()) }, |_| {
        Some("local runtime initialized".to_string())
    })
    .await?;

    println!("Init finished. Run `llmusage sync` to import local usage artifacts.");
    info!("local runtime bootstrap completed");
    Ok(())
}
