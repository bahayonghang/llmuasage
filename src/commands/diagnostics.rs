use std::path::PathBuf;

use anyhow::Result;

use crate::{app::AppContext, commands, store::Store};

pub async fn run(app: &AppContext, _out: Option<PathBuf>) -> Result<()> {
    Store::new(&app.paths).bootstrap()?;
    commands::not_implemented("diagnostics")
}
