use anyhow::Result;
use crossterm::style::{Color, Stylize, style};
use tracing::info;

use crate::tui::report_table::ColorMode;
use crate::{app::AppContext, store::Store, tui};

const TUI_DEPRECATION_WARNING: &str = "warning: `tui` is deprecated, use `llmusage dash` instead";

/// Returns the deprecation warning message if `deprecated` is true.
pub fn deprecation_message(deprecated: bool) -> Option<String> {
    deprecation_message_with_color(deprecated, ColorMode::from_env())
}

pub fn deprecation_message_with_color(deprecated: bool, color_mode: ColorMode) -> Option<String> {
    if !deprecated {
        return None;
    }

    if color_mode.stderr_enabled() {
        let warning = style("warning:").with(Color::Yellow).bold();
        let command = style("llmusage dash").with(Color::Cyan).bold();
        Some(format!(
            "{warning} `tui` is deprecated, use `{command}` instead"
        ))
    } else {
        Some(TUI_DEPRECATION_WARNING.to_string())
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
    use crate::tui::report_table::ColorMode;

    use super::{TUI_DEPRECATION_WARNING, deprecation_message, deprecation_message_with_color};

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

    #[test]
    fn deprecation_warning_can_force_ansi_styles() {
        let message = deprecation_message_with_color(true, ColorMode::Always).unwrap();
        assert!(message.contains("\u{1b}["));
        assert!(message.contains("warning:"));
        assert!(message.contains("llmusage dash"));
    }

    #[test]
    fn deprecation_warning_can_disable_ansi_styles() {
        let message = deprecation_message_with_color(true, ColorMode::Never).unwrap();
        assert_eq!(message, TUI_DEPRECATION_WARNING);
        assert!(!message.contains("\u{1b}["));
    }
}
