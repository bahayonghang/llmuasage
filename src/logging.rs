use anyhow::Result;
use tracing_subscriber::{EnvFilter, fmt};

pub fn init_logging() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
    Ok(())
}
