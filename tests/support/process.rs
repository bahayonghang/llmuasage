use std::{path::PathBuf, process::Command};

/// Builds a command for the Cargo-provided binary after verifying the path.
pub(crate) fn llmusage_command() -> Command {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_llmusage"));
    assert!(
        binary.is_file(),
        "Cargo binary does not exist: {}",
        binary.display()
    );
    Command::new(binary)
}
