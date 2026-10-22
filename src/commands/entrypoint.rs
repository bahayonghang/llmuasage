//! Read-only startup selection from the same schema that performs formal parsing.
use std::{ffi::OsString, path::PathBuf, process::ExitCode};

use anyhow::Result;
use clap::{Command, CommandFactory, FromArgMatches};

use crate::{
    common::i18n::{self, Language},
    paths::AppPaths,
    runtime::{self, preferences},
};

use super::{Cli, Commands, cli_errors, help};

#[derive(Debug, Default)]
pub struct StartupSelection {
    pub home: Option<PathBuf>,
    pub path: Vec<String>,
    pub help: bool,
    pub chinese_help: bool,
    chinese_help_index: Option<usize>,
}

pub fn schema() -> Command {
    let mut command = Cli::command();
    command.build();
    command
}

/// Only identifies routing and complete home values; Clap alone accepts or rejects argv.
pub fn select(command: &Command, args: &[OsString]) -> StartupSelection {
    let mut selection = StartupSelection::default();
    let mut current = command;
    let mut help_route = false;
    let mut first_home = None;
    let mut home_depth = None;
    let mut ambiguous_home = false;
    let mut index = 1;
    while index < args.len() {
        let token = &args[index];
        if token == "--" {
            break;
        }
        let value = token.to_str().unwrap_or("");
        if help_route && value == "--zh" {
            selection.chinese_help = true;
            selection.chinese_help_index.get_or_insert(index);
            index += 1;
            continue;
        }
        // Built-in help parses its remaining tokens as a command path, not as
        // the target command's options. In particular a trailing --home is not
        // a runtime-root override accepted by Clap.
        if help_route && value.starts_with('-') {
            index += 1;
            continue;
        }
        let mut consumed_value = false;
        if let Some((name, attached)) = long_option(token) {
            let has_attached = attached.is_some();
            if let Some(arg) = current.get_arguments().find(|arg| {
                arg.get_long() == Some(name)
                    || arg
                        .get_all_aliases()
                        .is_some_and(|aliases| aliases.contains(&name))
            }) {
                if arg.get_id() == "help" {
                    selection.help = true;
                }
                if arg
                    .get_num_args()
                    .is_some_and(|arity| arity.max_values() > 0)
                {
                    let candidate = attached.or_else(|| {
                        args.get(index + 1)
                            .filter(|next| !next.to_string_lossy().starts_with('-'))
                            .cloned()
                    });
                    if arg.get_id() == "home"
                        && let Some(home) = candidate.clone().map(PathBuf::from)
                    {
                        let depth = selection.path.len();
                        if first_home.is_none() {
                            first_home = Some(home.clone());
                        }
                        if home_depth == Some(depth) {
                            ambiguous_home = true;
                        }
                        if ambiguous_home {
                            selection.home = first_home.clone();
                        } else {
                            // Clap permits a global option at multiple command depths and
                            // propagates the deepest occurrence back to the root matches.
                            selection.home = Some(home);
                            home_depth = Some(depth);
                        }
                    }
                    consumed_value = !has_attached && candidate.is_some();
                }
            }
        } else if value.starts_with('-') && value != "-" {
            let cluster = &value[1..];
            for (offset, short) in cluster.char_indices() {
                let Some(arg) = current.get_arguments().find(|arg| {
                    arg.get_short() == Some(short)
                        || arg
                            .get_all_short_aliases()
                            .is_some_and(|aliases| aliases.contains(&short))
                }) else {
                    continue;
                };
                if arg.get_id() == "help" {
                    selection.help = true;
                }
                if arg
                    .get_num_args()
                    .is_some_and(|arity| arity.max_values() > 0)
                {
                    let rest = &cluster[offset + short.len_utf8()..];
                    consumed_value = rest.is_empty()
                        && args
                            .get(index + 1)
                            .is_some_and(|next| !next.to_string_lossy().starts_with('-'));
                    break;
                }
            }
        } else if !help_route && value == "help" && current.find_subcommand("help").is_some() {
            selection.help = true;
            help_route = true;
        } else if let Some(child) = current
            .find_subcommand(value)
            .filter(|_| !selection.help || help_route)
        {
            current = child;
            selection.path.push(child.get_name().to_owned());
        }
        index += 1 + usize::from(consumed_value);
    }
    selection
}

fn long_option(token: &std::ffi::OsStr) -> Option<(&str, Option<OsString>)> {
    let bytes = token.as_encoded_bytes().strip_prefix(b"--")?;
    match bytes.iter().position(|byte| *byte == b'=') {
        None => Some((std::str::from_utf8(bytes).ok()?, None)),
        Some(index) => {
            let name = std::str::from_utf8(&bytes[..index]).ok()?;
            // '=' is a self-synchronizing ASCII boundary in OsStr's encoding. The suffix
            // originates from this same platform's OsStr, including non-Unicode paths.
            let value =
                unsafe { OsString::from_encoded_bytes_unchecked(bytes[index + 1..].to_vec()) };
            Some((name, Some(value)))
        }
    }
}

pub async fn run_args(args: Vec<OsString>) -> Result<()> {
    let command = schema();
    let selection = select(&command, &args);
    let clean = selection.path.first().is_some_and(|name| name == "clean");
    let paths = match selection.home.clone() {
        Some(home) => AppPaths::with_root(home)?,
        None if clean => AppPaths::with_cli_home(None)?,
        None => AppPaths::discover()?,
    };
    let preferences = preferences::read(&paths.root_dir);
    let language = if selection.chinese_help {
        Language::Chinese
    } else {
        preferences.language
    };
    let result = i18n::scope(language, async move {
        if let Some(diagnostic) = preferences.diagnostic {
            eprintln!(
                "{} {}. {} {}: {}",
                language.select(
                    "Cannot read CLI language settings at",
                    "无法读取 CLI 语言设置："
                ),
                paths.root_dir.join(preferences::SETTINGS_FILE).display(),
                language.select(
                    "Using English; repair with `llmusage lang en` or `llmusage lang zh`.",
                    "请使用 `llmusage lang en` 或 `llmusage lang zh` 修复。"
                ),
                language.select("Original detail", "原始详情"),
                diagnostic.render(language)
            );
        }
        let mut parse_args = args;
        // Compatibility flag belongs only to the real `help` route, never an option value.
        if let Some(index) = selection.chinese_help_index {
            parse_args.remove(index);
        }
        let mut parser = command.clone();
        let matches = match parser.try_get_matches_from_mut(&parse_args) {
            Ok(matches) => matches,
            Err(error) => {
                let text = match error.kind() {
                    clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                        help::command_help(&command, &selection.path, language)
                    }
                    clap::error::ErrorKind::DisplayVersion => {
                        format!("llmusage {}\n", env!("CARGO_PKG_VERSION"))
                    }
                    _ => cli_errors::parser_error(&error, &command, &selection.path, language),
                };
                return Err(cli_errors::ProcessOutput {
                    text,
                    stderr: error.use_stderr(),
                    code: error.exit_code(),
                }
                .into());
            }
        };
        let cli = Cli::from_arg_matches(&matches)?;
        if let Some(Commands::Lang { ref language }) = cli.command {
            return super::lang::run(&paths.root_dir, language.as_deref());
        }
        let app = if clean {
            runtime::logging::init_cli_stderr_logging(language)?;
            runtime::app::AppContext::from_paths(paths)?
        } else {
            let app = runtime::app::AppContext::from_paths(paths)?;
            runtime::logging::init_cli_logging_for_paths(&app.paths, language)?;
            app
        };
        // Keep the large command future outside the startup/scope state machine.
        // This also preserves the Windows main-thread stack budget during sync.
        Box::pin(super::dispatch(app, cli)).await
    })
    .await;
    result.map_err(|error| error.context(cli_errors::FailureLanguage(language)))
}

/// Owns process output and returns normally so scopes and rendering guards unwind first.
pub async fn process() -> ExitCode {
    match run_args(std::env::args_os().collect()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(output) = error.downcast_ref::<cli_errors::ProcessOutput>() {
                output.print();
                ExitCode::from(output.code as u8)
            } else {
                let language = error
                    .downcast_ref::<cli_errors::FailureLanguage>()
                    .map(|failure| failure.0)
                    .unwrap_or_default();
                eprintln!("{}", cli_errors::application_error(&error, language));
                ExitCode::FAILURE
            }
        }
    }
}
