use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use tracing::debug;

use crate::{
    app::AppContext,
    models::SourceKind,
    query::reports::{self, PeriodKind},
    store::Store,
    tui::report_table,
};

use super::{
    report_args::{
        DailyArgs, MonthlyArgs, ReportCommonArgs, SessionArgs, UnifiedReportArgs, WeeklyArgs,
    },
    unified_report,
};

/// One source host in the top-level command tree, for example `llmusage codex`.
#[derive(Debug, Args)]
pub struct SourceReportArgs {
    #[command(subcommand)]
    pub command: SourceReportCommand,
}

#[derive(Debug, Subcommand)]
pub enum SourceReportCommand {
    /// Show daily usage for this source.
    Daily(DailyArgs),
    /// Show weekly usage for this source.
    Weekly(WeeklyArgs),
    /// Show monthly usage for this source.
    Monthly(MonthlyArgs),
    /// Show session usage for this source.
    Session(SessionArgs),
}

pub async fn run(app: &AppContext, source: SourceKind, command: SourceReportCommand) -> Result<()> {
    match command {
        SourceReportCommand::Daily(mut args) => {
            inject_source(&mut args.common, source)?;
            run_daily(app, source, args).await
        }
        SourceReportCommand::Weekly(mut args) => {
            inject_source(&mut args.common, source)?;
            run_period(app, source, args.common, args.unified, PeriodKind::Weekly).await
        }
        SourceReportCommand::Monthly(mut args) => {
            inject_source(&mut args.common, source)?;
            run_period(app, source, args.common, args.unified, PeriodKind::Monthly).await
        }
        SourceReportCommand::Session(mut args) => {
            inject_source(&mut args.common, source)?;
            run_session(app, source, args).await
        }
    }
}

fn inject_source(common: &mut ReportCommonArgs, source: SourceKind) -> Result<()> {
    match common.source {
        Some(explicit) if explicit != source => bail!(
            "`{}` source command conflicts with `--source {}`",
            source.as_str(),
            explicit.as_str()
        ),
        _ => {
            common.source = Some(source);
            Ok(())
        }
    }
}

async fn run_daily(app: &AppContext, source: SourceKind, args: DailyArgs) -> Result<()> {
    if args.instances {
        bail!("--instances is not supported for focused source reports");
    }

    debug!(
        source = source.as_str(),
        "starting focused daily report output"
    );
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let mut filter = args.common.to_filter(args.project.clone())?;
    if args.all && (filter.since.is_some() || filter.until.is_some()) {
        bail!("--all cannot be combined with --since or --until");
    }

    if !args.unified.sections.is_empty() {
        let reports = focused_sections(
            &store,
            &filter,
            source,
            PeriodKind::Daily,
            &args.unified,
            args.all,
        )?;
        print_focused_sections(
            &reports,
            source,
            PeriodKind::Daily,
            args.common.json,
            args.common.compact,
            args.common.no_cost,
        )?;
        return Ok(());
    }

    if !args.all {
        unified_report::apply_daily_default(&mut filter);
    }
    let report = reports::load_unified_report(&store, &filter, PeriodKind::Daily)?;
    print_focused_report(
        &unified_report::focused_report(&report, source),
        source,
        args.common.json,
        args.common.compact,
        args.common.no_cost,
    )
}

async fn run_period(
    app: &AppContext,
    source: SourceKind,
    common: ReportCommonArgs,
    unified: UnifiedReportArgs,
    kind: PeriodKind,
) -> Result<()> {
    debug!(
        source = source.as_str(),
        period = kind.rows_key(),
        "starting focused report output"
    );
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let filter = common.to_filter(None)?;

    if !unified.sections.is_empty() {
        let reports = focused_sections(&store, &filter, source, kind, &unified, false)?;
        return print_focused_sections(
            &reports,
            source,
            kind,
            common.json,
            common.compact,
            common.no_cost,
        );
    }

    let report = reports::load_unified_report(&store, &filter, kind)?;
    print_focused_report(
        &unified_report::focused_report(&report, source),
        source,
        common.json,
        common.compact,
        common.no_cost,
    )
}

async fn run_session(app: &AppContext, source: SourceKind, args: SessionArgs) -> Result<()> {
    if args.id.is_some() && !args.unified.sections.is_empty() {
        bail!("--sections cannot be combined with --id");
    }

    debug!(
        source = source.as_str(),
        "starting focused session report output"
    );
    let store = Store::new(&app.paths)?;
    store.bootstrap()?;
    let filter = args.common.to_filter(args.project.clone())?;

    if !args.unified.sections.is_empty() {
        let reports = focused_sections(
            &store,
            &filter,
            source,
            PeriodKind::Session,
            &args.unified,
            false,
        )?;
        return print_focused_sections(
            &reports,
            source,
            PeriodKind::Session,
            args.common.json,
            args.common.compact,
            args.common.no_cost,
        );
    }

    let report = reports::load_unified_session_report(&store, &filter, args.id.as_deref())?;
    print_focused_report(
        &unified_report::focused_report(&report, source),
        source,
        args.common.json,
        args.common.compact,
        args.common.no_cost,
    )
}

fn focused_sections(
    store: &Store,
    filter: &reports::ReportFilter,
    source: SourceKind,
    command_kind: PeriodKind,
    unified: &UnifiedReportArgs,
    daily_all: bool,
) -> Result<Vec<reports::UnifiedReport>> {
    unified_report::load_sections(store, filter, command_kind, &unified.sections, daily_all).map(
        |reports| {
            reports
                .iter()
                .map(|report| unified_report::focused_report(report, source))
                .collect()
        },
    )
}

fn print_focused_report(
    report: &reports::UnifiedReport,
    source: SourceKind,
    json: bool,
    compact: bool,
    no_cost: bool,
) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&unified_report::focused_report_json(report, no_cost)?)?
        );
    } else {
        println!(
            "{}",
            report_table::render_focused_table(
                report,
                source,
                compact,
                no_cost,
                report_table::ColorMode::from_env()
            )
        );
    }
    Ok(())
}

fn print_focused_sections(
    reports: &[reports::UnifiedReport],
    source: SourceKind,
    command_kind: PeriodKind,
    json: bool,
    compact: bool,
    no_cost: bool,
) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&unified_report::focused_sections_json(
                reports,
                command_kind,
                no_cost
            )?)?
        );
    } else {
        let color_mode = report_table::ColorMode::from_env();
        for (index, report) in reports.iter().enumerate() {
            if index > 0 {
                println!();
            }
            println!(
                "{}",
                report_table::render_focused_table(report, source, compact, no_cost, color_mode)
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::commands::Cli;

    #[test]
    fn all_source_period_commands_parse() {
        for command in ["claude", "codex", "opencode", "antigravity"] {
            for period in ["daily", "weekly", "monthly", "session"] {
                assert!(
                    Cli::try_parse_from(["llmusage", command, period]).is_ok(),
                    "{command} {period} should parse"
                );
            }
        }
        assert!(
            Cli::try_parse_from(["llmusage", "codex", "blocks"]).is_err(),
            "blocks should stay outside the focused source command tree"
        );
    }

    #[test]
    fn source_injection_accepts_the_same_value_and_rejects_a_conflict() {
        let mut common = ReportCommonArgs::default();
        inject_source(&mut common, SourceKind::Codex).expect("source injection should work");
        inject_source(&mut common, SourceKind::Codex).expect("same source should be accepted");
        let err = inject_source(&mut common, SourceKind::Claude).expect_err("should conflict");
        assert!(err.to_string().contains("--source codex"));
    }
}
