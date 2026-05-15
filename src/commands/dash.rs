use anyhow::Result;
use tracing::info;

use crate::{app::AppContext, store::Store, tui};

/// Returns the deprecation warning message if `deprecated` is true.
pub fn deprecation_message(deprecated: bool) -> Option<&'static str> {
    if deprecated {
        Some("warning: `tui` is deprecated, use `llmusage dash` instead")
    } else {
        None
    }
}

pub async fn run(app: &AppContext, deprecated: bool) -> Result<()> {
    if let Some(msg) = deprecation_message(deprecated) {
        eprintln!("{msg}");
    }

    info!("开始启动本地 TUI");

    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    tui::run_terminal(&store)?;

    info!("完成本地 TUI 会话");
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::commands::{Cli, Commands};

    use super::deprecation_message;

    #[test]
    fn dash_parses_from_args() {
        let cli = Cli::try_parse_from(["llmusage", "dash"]).expect("should parse `dash`");
        assert!(matches!(cli.command, Some(Commands::Dash)));
    }

    #[test]
    fn tui_parses_from_args_hidden_but_functional() {
        let cli = Cli::try_parse_from(["llmusage", "tui"]).expect("should parse `tui`");
        assert!(matches!(cli.command, Some(Commands::Tui)));
    }

    #[test]
    fn dash_visible_in_help_text() {
        let help = <Cli as clap::CommandFactory>::command()
            .render_help()
            .to_string();
        assert!(
            help.contains("dash"),
            "expected `dash` in help output, got: {help}"
        );
    }

    #[test]
    fn tui_hidden_from_help_text() {
        let help = <Cli as clap::CommandFactory>::command()
            .render_help()
            .to_string();
        // `tui` should not appear as a listed subcommand (it's hidden).
        // It may appear in the description of `dash` ("replaces `tui`"), which is fine.
        // Check that there's no line starting with "  tui" in the commands section.
        let has_tui_command = help.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("tui ") || trimmed == "tui"
        });
        assert!(
            !has_tui_command,
            "expected `tui` to not be listed as a subcommand in help output"
        );
    }

    #[test]
    fn deprecation_emitted_for_tui_variant() {
        // `Commands::Tui` is dispatched with `deprecated=true` in dispatch().
        // Verify the deprecation message is produced when deprecated flag is set.
        assert!(deprecation_message(true).is_some());
        assert!(deprecation_message(true).unwrap().contains("deprecated"));
        assert!(deprecation_message(true).unwrap().contains("llmusage dash"));
    }

    #[test]
    fn no_deprecation_for_dash_variant() {
        // `Commands::Dash` is dispatched with `deprecated=false`.
        assert!(deprecation_message(false).is_none());
    }
}
