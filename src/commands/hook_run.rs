use anyhow::Result;

use crate::{app::AppContext, commands, models::SourceKind, store::Store};

pub async fn run(app: &AppContext, _source: SourceKind, _trigger: &str, _auto: bool) -> Result<()> {
    Store::new(&app.paths).bootstrap()?;
    commands::not_implemented("hook-run")
}
