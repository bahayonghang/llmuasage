use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    query::{self, Dashboard},
    store::Store,
};

pub mod report_table;

pub fn run_terminal(store: &Store) -> Result<()> {
    let dashboard = Dashboard::open(store)?;
    let overview = dashboard.overview()?;
    let sources = dashboard.source_breakdown()?;
    let health = dashboard.health()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let draw_result = draw_loop(&mut terminal, &overview, &sources, &health);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    draw_result
}

fn draw_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    overview: &query::OverviewPayload,
    sources: &[query::SourceBreakdown],
    health: &query::HealthPayload,
) -> Result<()> {
    loop {
        terminal.draw(|frame| {
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(8),
                    Constraint::Length(10),
                    Constraint::Min(8),
                ])
                .split(frame.area());

            let overview_lines = vec![
                Line::from(vec![Span::raw(format!(
                    "累计 tokens: {}",
                    overview.total.total_tokens
                ))]),
                Line::from(vec![Span::raw(format!(
                    "24h tokens: {}",
                    overview.last_24h.total_tokens
                ))]),
                Line::from(vec![Span::raw(format!(
                    "来源数: {}    bucket 数: {}",
                    overview.source_count, overview.bucket_count
                ))]),
                Line::from(vec![Span::raw(format!(
                    "最近同步: {}",
                    overview.last_sync_at.as_deref().unwrap_or("never")
                ))]),
            ];
            frame.render_widget(
                Paragraph::new(overview_lines)
                    .block(Block::default().borders(Borders::ALL).title("总览"))
                    .wrap(Wrap { trim: true }),
                areas[0],
            );

            let source_lines = sources
                .iter()
                .map(|item| {
                    Line::from(vec![Span::raw(format!(
                        "{}  {}  {}",
                        item.source,
                        item.total_tokens,
                        item.last_event_at.as_deref().unwrap_or("never")
                    ))])
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                Paragraph::new(source_lines)
                    .block(Block::default().borders(Borders::ALL).title("来源"))
                    .wrap(Wrap { trim: true }),
                areas[1],
            );

            let health_lines = health
                .integrations
                .iter()
                .map(|item| {
                    Line::from(vec![Span::raw(format!("{}  {}", item.source, item.status))])
                })
                .chain(health.recent_failures.iter().map(|item| {
                    Line::from(vec![Span::raw(format!(
                        "FAIL {}  {}",
                        item.command,
                        item.error.as_deref().unwrap_or("")
                    ))])
                }))
                .collect::<Vec<_>>();
            frame.render_widget(
                Paragraph::new(health_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("健康 / 按 q 退出"),
                    )
                    .wrap(Wrap { trim: true }),
                areas[2],
            );
        })?;

        if let Event::Key(key) = event::read()?
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            break;
        }
    }

    Ok(())
}
