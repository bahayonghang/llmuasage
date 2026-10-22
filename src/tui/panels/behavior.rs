use crate::common::i18n::tui::format as tr_format;
use crate::common::i18n::tui::text as tr;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Color,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::query::{BehaviorSupport, ModelComparePayload, OptimizePayload, ZombieReport};
use std::cell::RefCell;

thread_local! {
    static PRESENTATION: RefCell<Option<Vec<crate::query::OptimizeEvidence>>> = const { RefCell::new(None) };
}

pub(in crate::tui) fn with_presentation<T>(
    payload: Option<&OptimizePayload>,
    hints: Option<&crate::query::OptimizePresentation>,
    render: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<Vec<crate::query::OptimizeEvidence>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            PRESENTATION.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let evidence = payload
        .zip(hints)
        .filter(|(payload, hints)| hints.matches(payload))
        .map(|(_, hints)| hints.evidence.clone());
    let _restore = Restore(PRESENTATION.with(|slot| slot.replace(evidence)));
    render()
}
use crate::tui::{
    app::BehaviorPanelPayload,
    format::{
        cost as format_cost, metric_value as format_metric_value, percent_ratio as format_percent,
        stat_compact,
    },
    theme,
};

/// Render the terminal behavior analytics summary.
pub fn render(frame: &mut Frame, area: Rect, data: &Option<Result<BehaviorPanelPayload, String>>) {
    match data {
        None => {
            let widget = Paragraph::new(tr("loading"))
                .style(theme::muted_style())
                .block(theme::panel_block(tr("behavior")));
            frame.render_widget(widget, area);
        }
        Some(Err(e)) => {
            let widget = Paragraph::new(tr_format!(data_load_failed_e, e = e))
                .style(theme::error_style())
                .block(theme::panel_block(tr("behavior")));
            frame.render_widget(widget, area);
        }
        Some(Ok(payload)) => render_payload(frame, area, payload),
    }
}

fn render_payload(frame: &mut Frame, area: Rect, payload: &BehaviorPanelPayload) {
    let [activity_area, tools_area, optimize_area, compare_area] = Layout::vertical([
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
    ])
    .areas(area);

    render_activity(frame, activity_area, payload);
    render_tools(frame, tools_area, payload);
    render_optimize(frame, optimize_area, &payload.optimize, &payload.zombie);
    render_compare(frame, compare_area, &payload.compare);
}

fn render_activity(frame: &mut Frame, area: Rect, payload: &BehaviorPanelPayload) {
    let mut lines = vec![support_line(&payload.activity.support)];
    if payload.activity.breakdown.is_empty() {
        lines.push(empty_reason_line(
            &payload.activity.support,
            tr("no_activity_behavior_facts"),
        ));
    } else {
        lines.extend(payload.activity.breakdown.iter().take(4).map(|row| {
            Line::from(vec![
                Span::styled(
                    format!("{} ", crate::common::i18n::tui::value(&row.category)),
                    theme::bold_fg_style(theme::accent()),
                ),
                Span::raw(tr_format!(
                    turns_edits_tokens_cost_one_shot_retry,
                    stat_compact(row.turns),
                    stat_compact(row.edit_turns),
                    stat_compact(row.total_tokens),
                    format_cost(row.estimated_cost_usd),
                    format_percent(row.one_shot_rate),
                    format_percent(row.retry_rate)
                )),
            ])
        }));
    }
    render_section(frame, area, tr("behavior_activity_categories"), lines);
}

fn render_tools(frame: &mut Frame, area: Rect, payload: &BehaviorPanelPayload) {
    let mut lines = vec![support_line(&payload.tools.support)];
    if payload.tools.breakdown.is_empty() {
        lines.push(empty_reason_line(
            &payload.tools.support,
            tr("no_tool_behavior_facts"),
        ));
    } else {
        lines.extend(payload.tools.breakdown.iter().take(4).map(|row| {
            let server = row
                .mcp_server
                .as_deref()
                .map(|server| format!(" @{server}"))
                .unwrap_or_default();
            Line::from(vec![
                Span::styled(
                    format!(
                        "{}/{}{} ",
                        crate::common::i18n::tui::value(&row.tool_kind),
                        row.tool_name,
                        server
                    ),
                    theme::bold_fg_style(theme::accent()),
                ),
                Span::raw(tr_format!(
                    calls_turns_sessions_share_cost,
                    stat_compact(row.calls),
                    stat_compact(row.turn_count),
                    stat_compact(row.session_count),
                    format_percent(row.call_share),
                    format_cost(row.estimated_cost_usd)
                )),
            ])
        }));
    }
    render_section(frame, area, tr("behavior_tools"), lines);
}

fn render_optimize(
    frame: &mut Frame,
    area: Rect,
    payload: &OptimizePayload,
    zombie: &ZombieReport,
) {
    let mut lines = vec![support_line(&payload.support)];
    lines.push(Line::styled(
        tr("read_only_advice_llmusage_never_deletes_archives_rewrites_or_cleans_content_automatically"),
        theme::muted_style(),
    ));

    if !payload.support.supported {
        if let Some(reason) = &payload.support.reason {
            lines.push(Line::styled(
                crate::common::i18n::tui::support_reason(&payload.support.level, Some(reason))
                    .unwrap_or_default(),
                theme::muted_style(),
            ));
        }
        lines.push(Line::styled(
            tr("no_behavior_facts_score_and_savings_are_not_calculated"),
            theme::muted_style(),
        ));
    } else {
        lines.push(Line::from(vec![
            Span::styled(
                tr_format!(score, payload.score, payload.grade),
                theme::bold_fg_style(theme::positive_fg()),
            ),
            Span::raw(tr_format!(
                estimated_savings,
                stat_compact(payload.estimated_savings_tokens),
                format_cost(payload.estimated_savings_usd)
            )),
        ]));

        if payload.findings.is_empty() {
            lines.push(Line::styled(
                tr("no_obvious_waste_patterns_found_review_the_surrounding_context_manually"),
                theme::muted_style(),
            ));
        } else {
            lines.extend(payload.findings.iter().take(3).map(|finding| {
                let (title, evidence, recommendation) = PRESENTATION.with(|slot| {
                    let hints = slot.borrow();
                    crate::common::i18n::tui::finding(
                        finding,
                        hints
                            .as_ref()
                            .and_then(|hints| hints.iter().find(|item| item.id() == finding.id)),
                    )
                });
                Line::from(vec![
                    Span::styled(
                        format!(
                            "[{}] {} ",
                            crate::common::i18n::tui::value(&finding.severity),
                            title
                        ),
                        theme::bold_fg_style(severity_color(&finding.severity)),
                    ),
                    Span::raw(tr_format!(recommendation, evidence, recommendation)),
                ])
            }));
        }
    }

    if zombie.zombies.is_empty() {
        lines.push(Line::styled(
            tr_format!(
                zombie_skills_mcps_none_installed_items_scanned,
                zombie.installed_total
            ),
            theme::muted_style(),
        ));
    } else {
        let preview = zombie
            .zombies
            .iter()
            .take(3)
            .map(|item| format!("{}:{}/{}", item.source, item.kind, item.name))
            .collect::<Vec<_>>()
            .join(" · ");
        lines.push(Line::from(vec![
            Span::styled(
                tr_format!(zombies, zombie.zombies.len()),
                theme::bold_fg_style(theme::warning_fg()),
            ),
            Span::raw(tr_format!(installed_but_never_called_installed_cleanup_candidates_llmusage_never_deletes_automatically, zombie.installed_total, preview)),
        ]));
    }

    render_section(frame, area, tr("behavior_optimize_read_only"), lines);
}

fn render_compare(frame: &mut Frame, area: Rect, payload: &ModelComparePayload) {
    let mut lines = vec![support_line(&payload.support)];
    if let Some(warning) = &payload.warning {
        lines.push(Line::from(vec![
            Span::styled(tr("warning"), theme::bold_fg_style(theme::warning_fg())),
            Span::raw(
                crate::common::i18n::tui::support_reason(&payload.support.level, Some(warning))
                    .unwrap_or_default(),
            ),
        ]));
    }

    match (&payload.model_a, &payload.model_b) {
        (Some(left), Some(right)) => {
            lines.push(Line::from(vec![
                Span::styled(
                    tr_format!(vs, left.model, right.model),
                    theme::bold_fg_style(theme::accent()),
                ),
                Span::raw(tr_format!(
                    calls_edits_cost,
                    left.model,
                    stat_compact(left.calls),
                    right.model,
                    stat_compact(right.calls),
                    left.model,
                    stat_compact(left.edit_turns),
                    right.model,
                    stat_compact(right.edit_turns),
                    left.model,
                    format_cost(left.estimated_cost_usd),
                    right.model,
                    format_cost(right.estimated_cost_usd)
                )),
            ]));
            lines.extend(payload.metrics.iter().take(3).map(|metric| {
                Line::from(vec![
                    Span::styled(
                        format!(
                            "{} ",
                            crate::common::i18n::tui::metric_label(&metric.id, &metric.label)
                        ),
                        theme::fg_style(theme::accent()),
                    ),
                    Span::raw(format!(
                        "{} → {}{}",
                        format_metric_value(metric.model_a_value),
                        format_metric_value(metric.model_b_value),
                        if metric.higher_is_better {
                            tr("higher_better")
                        } else {
                            ""
                        }
                    )),
                ])
            }));
            if let Some(category) = payload.category_head_to_head.first() {
                lines.push(Line::from(vec![
                    Span::styled(
                        tr_format!(
                            category,
                            crate::common::i18n::tui::value(&category.category)
                        ),
                        theme::fg_style(theme::accent()),
                    ),
                    Span::raw(tr_format!(
                        one_shot_vs,
                        format_percent(category.model_a_one_shot_rate),
                        format_percent(category.model_b_one_shot_rate)
                    )),
                ]));
            }
        }
        _ => {
            lines.push(empty_reason_line(
                &payload.support,
                tr("compare_requires_at_least_two_models_with_local_usage"),
            ));
            lines.push(Line::styled(
                tr_format!(candidate_models, payload.candidates.len()),
                theme::muted_style(),
            ));
        }
    }

    render_section(frame, area, tr("behavior_model_comparison"), lines);
}

fn render_section(frame: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'_>>) {
    let widget = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(section_block(title));
    frame.render_widget(widget, area);
}

fn section_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme::fg_style(theme::border_normal()))
        .title(Span::styled(
            format!(" {title} "),
            theme::bold_fg_style(theme::accent()),
        ))
}

fn support_line(support: &BehaviorSupport) -> Line<'_> {
    let level = support_level_label(&support.level);
    let status_style = if support.supported {
        theme::bold_fg_style(theme::positive_fg())
    } else {
        theme::bold_fg_style(theme::warning_fg())
    };
    let mut spans = vec![
        Span::styled(tr("status"), theme::fg_style(theme::accent())),
        Span::styled(level, status_style),
    ];
    if let Some(reason) =
        crate::common::i18n::tui::support_reason(&support.level, support.reason.as_deref())
    {
        spans.push(Span::raw(format!(" — {reason}")));
    }
    Line::from(spans)
}

fn empty_reason_line<'a>(support: &'a BehaviorSupport, fallback: &'a str) -> Line<'a> {
    Line::styled(
        crate::common::i18n::tui::support_reason(&support.level, support.reason.as_deref())
            .unwrap_or_else(|| fallback.to_string()),
        theme::muted_style(),
    )
}

fn support_level_label(level: &str) -> String {
    crate::common::i18n::tui::value(level).replace('_', "-")
}

fn severity_color(severity: &str) -> Color {
    match severity {
        "high" => theme::error_fg(),
        "medium" => theme::warning_fg(),
        _ => theme::positive_fg(),
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use crate::common::i18n::{Language, with_language};
    use crate::query::{OptimizeEvidence, OptimizeFinding, OptimizePresentation};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn frame_hints_match_the_report_and_restore_after_nested_scope_and_unwind() {
        let payload = OptimizePayload {
            support: BehaviorSupport {
                supported: true,
                level: "normalized".into(),
                reason: None,
            },
            score: 85,
            grade: "B".into(),
            estimated_savings_tokens: 7,
            estimated_savings_usd: 0.25,
            findings: vec![OptimizeFinding {
                id: "duplicate_reads".into(),
                title: "Repeated reads in one session".into(),
                severity: "medium".into(),
                evidence: "CANONICAL_EVIDENCE".into(),
                recommendation: "CANONICAL_ADVICE".into(),
                estimated_savings_tokens: 7,
                estimated_savings_usd: 0.25,
            }],
        };
        let hints = OptimizePresentation::new(
            &payload,
            vec![OptimizeEvidence::DuplicateReads {
                session: "SESSION_SENTINEL".into(),
                target: "TARGET_SENTINEL".into(),
                calls: 7,
            }],
        );
        let draw = || {
            let mut terminal = Terminal::new(TestBackend::new(180, 15)).unwrap();
            terminal
                .draw(|frame| {
                    render_optimize(
                        frame,
                        frame.area(),
                        &payload,
                        &ZombieReport {
                            installed_total: 0,
                            zombies: Vec::new(),
                        },
                    )
                })
                .unwrap();
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
        };
        with_language(Language::Chinese, || {
            assert!(draw().contains("CANONICAL_EVIDENCE"));
            with_presentation(Some(&payload), Some(&hints), || {
                let text = draw();
                assert!(text.contains("SESSION_SENTINEL") && text.contains("TARGET_SENTINEL"));
                assert!(!text.contains("CANONICAL_EVIDENCE"));
                let mut mismatch = payload.clone();
                mismatch.score += 1;
                with_presentation(Some(&mismatch), Some(&hints), || {
                    assert!(draw().contains("CANONICAL_EVIDENCE"))
                });
                assert!(draw().contains("SESSION_SENTINEL"));
                let panic = std::panic::catch_unwind(|| {
                    with_presentation(None, None, || panic!("hint-scope fixture"))
                });
                assert!(panic.is_err());
                assert!(draw().contains("SESSION_SENTINEL"));
            });
            assert!(draw().contains("CANONICAL_EVIDENCE"));
        });
        assert_eq!(payload.findings[0].evidence, "CANONICAL_EVIDENCE");
    }
}
