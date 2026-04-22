use std::path::PathBuf;

use anyhow::Result;

use crate::paths::AppPaths;

#[derive(Debug, Clone)]
pub struct AppContext {
    pub paths: AppPaths,
    pub current_exe: PathBuf,
}

impl AppContext {
    pub fn discover() -> Result<Self> {
        Ok(Self {
            paths: AppPaths::discover()?,
            current_exe: std::env::current_exe()?,
        })
    }
}
