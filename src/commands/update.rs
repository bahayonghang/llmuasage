use std::{
    fmt,
    io::{self, BufRead, Write},
    process::{Command, Stdio},
};

use anyhow::{Result, anyhow, bail};
use clap::ValueEnum;

const GITHUB_REPOSITORY: &str = "https://github.com/bahayonghang/llmuasage";
const CARGO_PACKAGE: &str = "llmusage";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum UpdateChannel {
    #[default]
    Main,
    Dev,
}

impl UpdateChannel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Dev => "dev",
        }
    }
}

impl fmt::Display for UpdateChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallPlan {
    program: &'static str,
    args: Vec<String>,
}

impl InstallPlan {
    fn for_channel(channel: UpdateChannel) -> Self {
        Self {
            program: "cargo",
            args: install_args(channel),
        }
    }

    fn command_line(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallOutcome {
    Success,
    Failed(Option<i32>),
}

pub fn run(channel: UpdateChannel, check_only: bool) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_with(
        channel,
        check_only,
        &mut stdin.lock(),
        &mut stdout.lock(),
        execute_install,
    )
}

fn run_with<R, W, F>(
    channel: UpdateChannel,
    check_only: bool,
    reader: &mut R,
    writer: &mut W,
    mut executor: F,
) -> Result<()>
where
    R: BufRead,
    W: Write,
    F: FnMut(&InstallPlan) -> Result<InstallOutcome>,
{
    let plan = InstallPlan::for_channel(channel);
    print_header(writer, channel, &plan)?;

    if check_only {
        writeln!(writer, "Check only: no update was performed.")?;
        return Ok(());
    }

    if !confirm_update(reader, writer)? {
        writeln!(writer, "Update cancelled.")?;
        return Ok(());
    }

    writeln!(writer, "Starting update...")?;
    writer.flush()?;
    let outcome = executor(&plan).map_err(|error| {
        anyhow!(
            "failed to start Cargo update from `{channel}`: {error:#}\nManual command: {}",
            plan.command_line()
        )
    })?;

    match outcome {
        InstallOutcome::Success => {
            writeln!(writer, "Update completed successfully.")?;
            writeln!(
                writer,
                "Run `llmusage --version` to verify the installed version."
            )?;
            Ok(())
        }
        InstallOutcome::Failed(exit_code) => {
            let detail = exit_code.map_or_else(
                || "terminated without an exit code".to_string(),
                |code| format!("exit code {code}"),
            );
            bail!(
                "Cargo update from `{channel}` failed with {detail}.\nManual command: {}",
                plan.command_line()
            )
        }
    }
}

fn print_header(writer: &mut impl Write, channel: UpdateChannel, plan: &InstallPlan) -> Result<()> {
    writeln!(writer, "llmusage update")?;
    writeln!(writer, "Current version: {}", env!("CARGO_PKG_VERSION"))?;
    writeln!(writer, "Repository: {GITHUB_REPOSITORY}")?;
    writeln!(writer, "Channel: {channel}")?;
    writeln!(writer, "Command: {}", plan.command_line())?;
    writeln!(writer)?;
    Ok(())
}

fn confirm_update(reader: &mut impl BufRead, writer: &mut impl Write) -> Result<bool> {
    loop {
        write!(writer, "Continue with the update? [Y/n]: ")?;
        writer.flush()?;

        let mut input = String::new();
        let bytes_read = reader.read_line(&mut input)?;
        if bytes_read == 0 {
            bail!("confirmation input reached EOF; update was not started");
        }

        match input.trim().to_ascii_lowercase().as_str() {
            "" | "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => writeln!(writer, "Please answer `y` or `n`.")?,
        }
    }
}

fn install_args(channel: UpdateChannel) -> Vec<String> {
    [
        "install",
        "--git",
        GITHUB_REPOSITORY,
        CARGO_PACKAGE,
        "--branch",
        channel.as_str(),
        "--locked",
        "--force",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn execute_install(plan: &InstallPlan) -> Result<InstallOutcome> {
    let status = Command::new(plan.program)
        .args(&plan.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if status.success() {
        Ok(InstallOutcome::Success)
    } else {
        Ok(InstallOutcome::Failed(status.code()))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        io::{self, BufReader, Cursor, Read},
    };

    use anyhow::anyhow;

    use super::*;

    #[test]
    fn install_plan_uses_locked_official_branch_install() {
        let main = InstallPlan::for_channel(UpdateChannel::Main);
        assert_eq!(
            main.args,
            [
                "install",
                "--git",
                GITHUB_REPOSITORY,
                CARGO_PACKAGE,
                "--branch",
                "main",
                "--locked",
                "--force",
            ]
        );

        let dev = InstallPlan::for_channel(UpdateChannel::Dev);
        assert_eq!(dev.args[5], "dev");
    }

    #[test]
    fn confirmation_accepts_default_yes_and_no() -> Result<()> {
        let mut output = Vec::new();
        assert!(confirm_update(&mut Cursor::new("\n"), &mut output)?);
        assert!(confirm_update(&mut Cursor::new("YES\n"), &mut output)?);
        assert!(!confirm_update(&mut Cursor::new("no\n"), &mut output)?);
        Ok(())
    }

    #[test]
    fn confirmation_reprompts_invalid_input_and_refuses_eof() -> Result<()> {
        let mut output = Vec::new();
        assert!(confirm_update(&mut Cursor::new("maybe\ny\n"), &mut output)?);
        assert!(String::from_utf8(output)?.contains("Please answer"));

        let error = confirm_update(&mut Cursor::new(Vec::<u8>::new()), &mut Vec::new())
            .expect_err("EOF must not confirm an update");
        assert!(error.to_string().contains("EOF"));
        Ok(())
    }

    #[test]
    fn confirmation_read_failure_never_calls_executor() {
        struct FailingReader;

        impl Read for FailingReader {
            fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("input unavailable"))
            }
        }

        let called = Cell::new(false);
        let error = run_with(
            UpdateChannel::Main,
            false,
            &mut BufReader::new(FailingReader),
            &mut Vec::new(),
            |_| {
                called.set(true);
                Ok(InstallOutcome::Success)
            },
        )
        .expect_err("confirmation I/O failure must stop the update");

        assert!(!called.get());
        assert!(error.to_string().contains("input unavailable"));
    }

    #[test]
    fn check_only_never_calls_executor() -> Result<()> {
        let called = Cell::new(false);
        let mut output = Vec::new();
        run_with(
            UpdateChannel::Main,
            true,
            &mut Cursor::new(Vec::<u8>::new()),
            &mut output,
            |_| {
                called.set(true);
                Ok(InstallOutcome::Success)
            },
        )?;

        assert!(!called.get());
        let output = String::from_utf8(output)?;
        assert!(output.contains("Channel: main"));
        assert!(output.contains("--branch main --locked --force"));
        assert!(output.contains("no update was performed"));
        Ok(())
    }

    #[test]
    fn cancellation_never_calls_executor() -> Result<()> {
        let called = Cell::new(false);
        let mut output = Vec::new();
        run_with(
            UpdateChannel::Dev,
            false,
            &mut Cursor::new("n\n"),
            &mut output,
            |_| {
                called.set(true);
                Ok(InstallOutcome::Success)
            },
        )?;

        assert!(!called.get());
        assert!(String::from_utf8(output)?.contains("Update cancelled"));
        Ok(())
    }

    #[test]
    fn executor_success_and_failures_are_propagated() -> Result<()> {
        let called = Cell::new(false);
        run_with(
            UpdateChannel::Dev,
            false,
            &mut Cursor::new("y\n"),
            &mut Vec::new(),
            |plan| {
                called.set(true);
                assert_eq!(plan.args[5], "dev");
                Ok(InstallOutcome::Success)
            },
        )?;
        assert!(called.get());

        let exit_error = run_with(
            UpdateChannel::Dev,
            false,
            &mut Cursor::new("y\n"),
            &mut Vec::new(),
            |_| Ok(InstallOutcome::Failed(Some(101))),
        )
        .expect_err("non-zero Cargo exit must fail");
        let exit_error = exit_error.to_string();
        assert!(exit_error.contains("exit code 101"));
        assert!(exit_error.contains("--branch dev --locked --force"));

        let start_error = run_with(
            UpdateChannel::Main,
            false,
            &mut Cursor::new("y\n"),
            &mut Vec::new(),
            |_| Err(anyhow!("cargo is missing")),
        )
        .expect_err("Cargo startup failure must fail");
        let start_error = start_error.to_string();
        assert!(start_error.contains("cargo is missing"));
        assert!(start_error.contains("--branch main --locked --force"));
        Ok(())
    }
}
