use anyhow::Result;

use crate::{app::AppContext, commands, store::Store};

pub async fn run(app: &AppContext, _port: Option<u16>) -> Result<()> {
    Store::new(&app.paths).bootstrap()?;
    commands::not_implemented("serve")
}
