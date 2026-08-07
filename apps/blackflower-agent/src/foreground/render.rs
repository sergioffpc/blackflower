use std::time::{Duration, Instant};

use blackflower_networking::SessionState;
use blackflower_observability::{ForegroundLogEvent, ForegroundLogLevel};
use blackflower_observability_tui::MetricStore;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Gauge, Paragraph, Row, Sparkline, Table, Tabs, Wrap,
};

use super::app::{App, Page};
use crate::AgentHealth;

const MIN_WIDTH: u16 = 72;
const MIN_HEIGHT: u16 = 20;

pub(crate) const CODEX_BACKGROUND: Color = Color::Rgb(14, 29, 57);
const CODEX_SURFACE: Color = Color::Rgb(39, 55, 83);
const CODEX_BORDER: Color = Color::Rgb(72, 92, 122);
const CODEX_TEXT: Color = Color::Rgb(219, 224, 232);
const CODEX_MUTED: Color = Color::Rgb(132, 146, 166);
const CODEX_ACCENT: Color = Color::Rgb(79, 195, 247);
const CODEX_SUCCESS: Color = Color::Rgb(101, 214, 173);
const CODEX_WARNING: Color = Color::Rgb(243, 201, 105);
const CODEX_ERROR: Color = Color::Rgb(255, 122, 144);

pub(crate) fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(base_style()), area);
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, area);
        return;
    }

    let content = area.inner(Margin::new(2, 1));
    let regions = Layout::vertical([
        Constraint::Length(7),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .split(content);
    draw_header(frame, regions[0], app);
    match app.page {
        Page::Overview => draw_overview(frame, regions[1], app),
        Page::Logs => draw_logs(frame, regions[1], app),
        Page::Agents => draw_agents(frame, regions[1], app),
        Page::Sensorium => draw_sensorium(frame, regions[1], app),
        Page::Decisions => draw_decisions(frame, regions[1], app),
        Page::Session => draw_session(frame, regions[1], app),
        Page::Prediction => draw_prediction(frame, regions[1], app),
        Page::Navigation => draw_navigation_panel(frame, regions[1], app),
        Page::Host => draw_host(frame, regions[1], app),
    }
    draw_footer(frame, regions[2], app);
    if app.show_help {
        draw_help(frame, area);
    }
    if app.logs.filter_editor.is_some() {
        draw_filter_editor(frame, area, app);
    }
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(4),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("› ", accent_style()),
            Span::styled(app.service_name, text_style().add_modifier(Modifier::BOLD)),
            Span::styled(" --foreground", muted_style()),
        ])),
        regions[0],
    );
    draw_identity(frame, regions[1], app);
    draw_tabs(frame, regions[2], area.width, app.page);
}

fn draw_identity(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let identity = vec![
        Line::from(vec![
            Span::styled("›_ ", accent_style()),
            Span::styled(app.service_name, text_style().add_modifier(Modifier::BOLD)),
            Span::styled(format!("  (v{})", app.service_version), muted_style()),
        ]),
        Line::from(vec![
            Span::styled("metrics: ", muted_style()),
            Span::styled(scrape_status(app), scrape_style(app)),
            Span::styled("    uptime: ", muted_style()),
            Span::styled(format_uptime(app.started.elapsed()), text_style()),
            Span::styled("    policy: ", muted_style()),
            Span::styled(
                configured(app.capabilities.policy_configured),
                capability_style(app.capabilities.policy_configured),
            ),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(identity).style(base_style()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style())
                .style(base_style()),
        ),
        area,
    );
}

fn draw_tabs(frame: &mut Frame<'_>, area: Rect, width: u16, page: Page) {
    let titles = Page::ALL
        .iter()
        .enumerate()
        .map(|(index, page)| {
            let title = if width < 100 {
                page.short_title()
            } else {
                page.title()
            };
            Line::from(format!(" {} {title} ", index + 1))
        })
        .collect::<Vec<_>>();
    let regions = Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).split(area);
    frame.render_widget(Paragraph::new("›").style(accent_style()), regions[0]);
    frame.render_widget(
        Tabs::new(titles)
            .select(page_index(page))
            .style(muted_style())
            .highlight_style(
                Style::default()
                    .fg(CODEX_TEXT)
                    .bg(CODEX_SURFACE)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("  "),
        regions[1],
    );
}

fn draw_overview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Percentage(38),
        Constraint::Percentage(37),
        Constraint::Percentage(25),
    ])
    .split(area);
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[0]);
    draw_agent_summary(frame, columns[0], app);
    draw_process_summary(frame, columns[1], app);

    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
    draw_session_summary(frame, columns[0], app);
    draw_prediction_summary(frame, columns[1], app);
    draw_recent_logs(frame, rows[2], app);
}

fn draw_agent_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Agent runtime",
        vec![
            (
                "Active",
                format_number(app.metrics.value("blackflower_agent_active_agents")),
            ),
            (
                "Healthy / stalled",
                format!(
                    "{} / {}",
                    format_number(app.metrics.value_with_label(
                        "blackflower_agent_agents",
                        "health",
                        AgentHealth::Healthy.as_str(),
                    )),
                    format_number(app.metrics.value_with_label(
                        "blackflower_agent_agents",
                        "health",
                        AgentHealth::Stalled.as_str(),
                    )),
                ),
            ),
            (
                "Decision p95",
                format_millis(
                    app.metrics
                        .histogram_quantile("blackflower_agent_decision_duration_seconds", 0.95),
                ),
            ),
            (
                "Diagnostic drops",
                format_rate(
                    app.metrics
                        .rate("blackflower_agent_diagnostic_records_dropped_total"),
                    "/s",
                ),
            ),
        ],
    );
}

fn draw_process_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Process",
        vec![
            (
                "CPU",
                format_cores(app.metrics.rate("process_cpu_seconds_total")),
            ),
            (
                "RSS",
                format_bytes(app.metrics.value("process_resident_memory_bytes")),
            ),
            (
                "Virtual",
                format_bytes(app.metrics.value("process_virtual_memory_bytes")),
            ),
            (
                "Open FDs",
                format_number(app.metrics.value("process_open_fds")),
            ),
        ],
    );
}

fn draw_session_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Session",
        vec![
            (
                "Connections",
                format_number(app.metrics.value("blackflower_network_connections")),
            ),
            (
                "RTT p95",
                format_millis(
                    app.metrics
                        .histogram_quantile("blackflower_network_rtt_seconds", 0.95),
                ),
            ),
            (
                "Inputs",
                format_rate(app.metrics.rate("blackflower_network_inputs_total"), "/s"),
            ),
            (
                "Resyncs",
                format_rate(app.metrics.rate("blackflower_network_resync_total"), "/s"),
            ),
        ],
    );
}

fn draw_prediction_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Prediction",
        vec![
            (
                "Forward ticks",
                format_rate(
                    app.metrics.rate_with_label(
                        "blackflower_world_prediction_ticks_total",
                        "pass",
                        "forward",
                    ),
                    "/s",
                ),
            ),
            (
                "Resimulation",
                format_rate(
                    app.metrics.rate_with_label(
                        "blackflower_world_prediction_ticks_total",
                        "pass",
                        "resimulation",
                    ),
                    "/s",
                ),
            ),
            (
                "Tick p95",
                format_millis(app.metrics.histogram_quantile(
                    "blackflower_world_prediction_tick_duration_seconds",
                    0.95,
                )),
            ),
            (
                "Reconciliations",
                format_rate(
                    app.metrics
                        .rate("blackflower_world_prediction_reconciliations_total"),
                    "/s",
                ),
            ),
        ],
    );
}

fn draw_recent_logs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let row_count = usize::from(area.height.saturating_sub(2));
    let lines = app
        .logs
        .recent(row_count)
        .into_iter()
        .map(log_line)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).block(panel("Recent logs")), area);
}

fn draw_logs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .split(area);
    draw_log_filters(frame, regions[0], app);
    draw_log_table(frame, regions[1], regions[2], app);
}

fn draw_log_filters(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let filter = if app.logs.filter_source.is_empty() {
        "—"
    } else {
        app.logs.filter_source.as_str()
    };
    let state = if app.logs.paused {
        "PAUSED"
    } else if app.logs.follow {
        "FOLLOW"
    } else {
        "SCROLL"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" Capture "),
            Span::styled(
                app.logs.control.level().as_str(),
                text_style().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  View "),
            Span::styled(
                app.logs.view_level.as_str(),
                text_style().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Regex "),
            Span::styled(filter, text_style().add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(state, surface_style().add_modifier(Modifier::BOLD)),
        ]))
        .block(panel("Filters")),
        area,
    );
}

fn draw_log_table(frame: &mut Frame<'_>, table_area: Rect, status_area: Rect, app: &App) {
    let visible_rows = usize::from(table_area.height.saturating_sub(3));
    let (events, first, total) = app.logs.visible(visible_rows);
    let rows = events.into_iter().map(log_row).collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(11),
                Constraint::Length(7),
                Constraint::Length(28),
                Constraint::Min(20),
            ],
        )
        .header(
            Row::new(["Elapsed", "Level", "Target", "Message and fields"])
                .style(table_header_style()),
        )
        .block(panel("Structured logs"))
        .column_spacing(1),
        table_area,
    );
    let disconnected = if app.logs.disconnected() {
        " · source closed"
    } else {
        ""
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{}-{} of {} · dropped {}{}",
            if total == 0 { 0 } else { first + 1 },
            first + visible_rows.min(total.saturating_sub(first)),
            total,
            app.logs.control.dropped_events(),
            disconnected,
        ))
        .style(muted_style()),
        status_area,
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "the agent table keeps its complete bounded operational column mapping together"
)]
fn draw_agents(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([Constraint::Min(6), Constraint::Length(2)]).split(area);
    let now = Instant::now();
    let rows = app
        .diagnostics
        .agents
        .iter()
        .map(|(agent_id, view)| {
            let status = view.status.as_ref();
            let decision = view.decisions.back();
            let sensorium = view.sensorium.as_ref();
            let selected = if app.diagnostics.selected == Some(*agent_id) {
                "›"
            } else {
                " "
            };
            let row = Row::new([
                format!("{selected} {agent_id}"),
                status.map_or_else(
                    || "—".to_owned(),
                    |status| session_state(status.session_state()).to_owned(),
                ),
                status.map_or_else(
                    || "—".to_owned(),
                    |status| status.descriptor().difficulty().to_string(),
                ),
                status.map_or_else(
                    || "—".to_owned(),
                    |status| status.descriptor().policy_version().to_string(),
                ),
                decision.map_or_else(
                    || "—".to_owned(),
                    |decision| decision.current_intent().to_string(),
                ),
                decision.map_or_else(
                    || "—".to_owned(),
                    |decision| decision.source().as_str().to_owned(),
                ),
                decision.map_or_else(
                    || "—".to_owned(),
                    |decision| format_age(now.saturating_duration_since(decision.recorded_at())),
                ),
                sensorium.map_or_else(
                    || "—".to_owned(),
                    |snapshot| format_age(now.saturating_duration_since(snapshot.recorded_at())),
                ),
                status.map_or_else(
                    || "—".to_owned(),
                    |status| status.health().as_str().to_owned(),
                ),
            ]);
            if app.diagnostics.selected == Some(*agent_id) {
                row.style(surface_style().add_modifier(Modifier::BOLD))
            } else {
                row
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(5),
                Constraint::Length(13),
                Constraint::Length(11),
                Constraint::Length(14),
                Constraint::Min(12),
                Constraint::Length(10),
                Constraint::Length(9),
                Constraint::Length(9),
                Constraint::Length(10),
            ],
        )
        .header(
            Row::new([
                "Agent",
                "Session",
                "Difficulty",
                "Policy",
                "Intent",
                "Source",
                "Decision",
                "Observe",
                "Health",
            ])
            .style(table_header_style()),
        )
        .block(panel("Real runtime agents · bounded to 32"))
        .column_spacing(1),
        regions[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            " {} · ↑/↓ select · details persist across Agents, Sensorium, and Decisions",
            diagnostic_stream_status(app),
        ))
        .style(muted_style()),
        regions[1],
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "the sensorium page maps one immutable record into its linked channel and memory views"
)]
fn draw_sensorium(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some((agent_id, view)) = app.diagnostics.selected_view() else {
        draw_no_agent_records(frame, area, app, "Sensorium");
        return;
    };
    let Some(snapshot) = view.sensorium.as_ref() else {
        draw_missing_selected_record(frame, area, app, agent_id, "sensorium");
        return;
    };
    let regions = Layout::vertical([
        Constraint::Length(5),
        Constraint::Percentage(44),
        Constraint::Percentage(56),
    ])
    .split(area);
    draw_key_values(
        frame,
        regions[0],
        &format!("Sensorium · agent {agent_id}"),
        vec![
            ("Observation", snapshot.observation_sequence().to_string()),
            ("Tick", snapshot.observation_tick().get().to_string()),
            (
                "Freshness",
                format_age(Instant::now().saturating_duration_since(snapshot.recorded_at())),
            ),
            (
                "Schema / policy",
                format!(
                    "{} / {}",
                    snapshot.schema_version(),
                    snapshot.policy_version()
                ),
            ),
        ],
    );
    let channel_rows = snapshot
        .channels()
        .iter()
        .map(|channel| {
            Row::new([
                channel.kind().as_str().to_owned(),
                channel.availability().as_str().to_owned(),
                channel.summary().to_string(),
                if channel.affected_decision() {
                    "applied".to_owned()
                } else {
                    "available".to_owned()
                },
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            channel_rows,
            [
                Constraint::Length(13),
                Constraint::Length(16),
                Constraint::Min(24),
                Constraint::Length(11),
            ],
        )
        .header(
            Row::new([
                "Channel",
                "Availability",
                "Exact runtime projection",
                "Decision",
            ])
            .style(table_header_style()),
        )
        .block(panel(&format!(
            "Senses, body, capacity, performance · {} perceived",
            snapshot.perceived_entities()
        )))
        .column_spacing(1),
        regions[1],
    );
    let memory_rows = snapshot
        .memory()
        .iter()
        .map(|item| {
            Row::new([
                item.token().to_string(),
                item.kind().as_str().to_owned(),
                item.status().as_str().to_owned(),
                item.summary().to_string(),
                format!("{:.0}%", item.confidence() * 100.0),
                format!("{:.0}%", item.uncertainty() * 100.0),
                format_age(item.age()),
                if item.consumed_by_decision() {
                    "used".to_owned()
                } else {
                    "unused".to_owned()
                },
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            memory_rows,
            [
                Constraint::Length(7),
                Constraint::Length(10),
                Constraint::Length(11),
                Constraint::Min(24),
                Constraint::Length(7),
                Constraint::Length(7),
                Constraint::Length(8),
                Constraint::Length(7),
            ],
        )
        .header(
            Row::new([
                "Token",
                "Kind",
                "Status",
                "Legal evidence / belief",
                "Conf.",
                "Uncert.",
                "Age",
                "Policy",
            ])
            .style(table_header_style()),
        )
        .block(panel(
            "Actual bounded semantic memory · no authoritative identities",
        ))
        .column_spacing(1),
        regions[2],
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "the decisions page keeps the causal feed and selected record projection together"
)]
fn draw_decisions(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some((agent_id, view)) = app.diagnostics.selected_view() else {
        draw_no_agent_records(frame, area, app, "Decisions");
        return;
    };
    let regions =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area);
    let selected_sequence = app
        .diagnostics
        .selected_decision()
        .map(|decision| decision.decision_sequence());
    let rows = view
        .decisions
        .iter()
        .rev()
        .map(|decision| {
            let row = Row::new([
                decision.decision_sequence().to_string(),
                decision.source().as_str().to_owned(),
                decision.selected_action().to_string(),
                decision.outcome().as_str().to_owned(),
                format_age(Instant::now().saturating_duration_since(decision.recorded_at())),
            ]);
            if selected_sequence == Some(decision.decision_sequence()) {
                row.style(surface_style().add_modifier(Modifier::BOLD))
            } else {
                row
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(7),
                Constraint::Length(10),
                Constraint::Min(14),
                Constraint::Length(10),
                Constraint::Length(8),
            ],
        )
        .header(Row::new(["Seq", "Source", "Action", "Outcome", "Age"]).style(table_header_style()))
        .block(panel(&format!(
            "Decisions · agent {agent_id} · newest first"
        )))
        .column_spacing(1),
        regions[0],
    );
    let Some(decision) = app.diagnostics.selected_decision() else {
        frame.render_widget(
            Paragraph::new("No real controller decision has been published for this agent.")
                .style(muted_style())
                .block(panel("Causal detail"))
                .wrap(Wrap { trim: true }),
            regions[1],
        );
        return;
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Selected  ", muted_style()),
            Span::styled(
                decision.selected_action().to_string(),
                accent_style().add_modifier(Modifier::BOLD),
            ),
            Span::styled("  →  ", muted_style()),
            Span::styled(decision.emission().to_string(), text_style()),
        ]),
        Line::from(format!(
            "Intent {} · source {} · outcome {} · observation {} @ tick {}",
            decision.current_intent(),
            decision.source().as_str(),
            decision.outcome().as_str(),
            decision.observation_sequence(),
            decision.observation_tick().get(),
        )),
        Line::from(format!(
            "Decision {:.3} ms · inference {} · budget {}",
            decision.decision_duration().as_secs_f64() * 1_000.0,
            decision.inference_duration().map_or_else(
                || "not used".to_owned(),
                |duration| format!("{:.3} ms", duration.as_secs_f64() * 1_000.0),
            ),
            if decision.budget_exhausted() {
                "EXHAUSTED"
            } else {
                "within limit"
            },
        )),
        Line::from(""),
        Line::from(Span::styled("Policy candidates", table_header_style())),
    ];
    if decision.candidates().is_empty() {
        lines.push(Line::from("  —"));
    } else {
        lines.extend(decision.candidates().iter().map(|candidate| {
            Line::from(format!(
                "  {:<24} {:>8.3}  {}",
                candidate.action(),
                candidate.score(),
                candidate.disposition(),
            ))
        }));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Downstream constraints",
        table_header_style(),
    )));
    if decision.constraints().is_empty() {
        lines.push(Line::from("  none"));
    } else {
        lines.extend(decision.constraints().iter().map(|constraint| {
            Line::from(format!(
                "  {} → {}",
                constraint.stage(),
                constraint.effect()
            ))
        }));
    }
    if let Some(reason) = decision.fallback_reason() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Fallback: {}", reason.as_str()),
            Style::default()
                .fg(CODEX_WARNING)
                .bg(CODEX_BACKGROUND)
                .add_modifier(Modifier::BOLD),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(text_style())
            .block(panel(
                "Causal detail · policy selection then deterministic overrides",
            ))
            .wrap(Wrap { trim: true }),
        regions[1],
    );
}

fn draw_no_agent_records(frame: &mut Frame<'_>, area: Rect, app: &App, title: &str) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(diagnostic_stream_status(app)),
            Line::from(""),
            Line::from("This surface consumes only bounded records published by an established AgentRuntime controller."),
            Line::from("The foreground UI never inspects the harness, navigation runtime, policy, or mutable agent state directly."),
        ])
        .style(muted_style())
        .block(panel(title))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_missing_selected_record(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    agent_id: crate::AgentId,
    kind: &str,
) {
    frame.render_widget(
        Paragraph::new(format!(
            "Agent {agent_id} is known, but no real {kind} record has been published yet.\n\n{}",
            diagnostic_stream_status(app),
        ))
        .style(muted_style())
        .block(panel("Waiting for runtime data"))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn diagnostic_stream_status(app: &App) -> &'static str {
    if !app.diagnostics.configured {
        "diagnostic stream unavailable: process-only shell"
    } else if app.diagnostics.disconnected {
        "diagnostic stream closed"
    } else if app.diagnostics.agents.is_empty() {
        "diagnostic stream live: waiting for runtime records"
    } else {
        "diagnostic stream live"
    }
}

fn draw_session(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([
        Constraint::Length(8),
        Constraint::Percentage(45),
        Constraint::Percentage(55),
    ])
    .split(area);
    draw_session_identity(frame, regions[0], app);
    draw_session_activity(frame, regions[1], app);
    draw_session_health(frame, regions[2], app);
}

fn draw_session_identity(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Ordinary client session",
        vec![
            ("Runtime", configured(app.capabilities.runtime_configured)),
            (
                "Connections",
                format_number(app.metrics.value("blackflower_network_connections")),
            ),
            (
                "RTT p50 / p95",
                format!(
                    "{} / {}",
                    format_millis(
                        app.metrics
                            .histogram_quantile("blackflower_network_rtt_seconds", 0.50),
                    ),
                    format_millis(
                        app.metrics
                            .histogram_quantile("blackflower_network_rtt_seconds", 0.95),
                    ),
                ),
            ),
            (
                "Clock uncertainty",
                format_number(
                    app.metrics
                        .value("blackflower_network_clock_uncertainty_ticks"),
                ),
            ),
            (
                "Controls",
                format_rate(app.metrics.rate("blackflower_network_inputs_total"), "/s"),
            ),
        ],
    );
}

fn draw_session_activity(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    draw_network_sparklines(frame, columns[0], app);
    draw_metric_series(
        frame,
        columns[1],
        "Bounded queues",
        &app.metrics,
        "blackflower_network_queue_depth",
        "queue",
        false,
    );
}

fn draw_session_health(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    draw_key_values(
        frame,
        columns[0],
        "Session health",
        vec![
            (
                "Drops",
                format_rate(app.metrics.rate("blackflower_network_drops_total"), "/s"),
            ),
            (
                "Violations",
                format_rate(
                    app.metrics
                        .rate("blackflower_network_protocol_violations_total"),
                    "/s",
                ),
            ),
            (
                "Resyncs",
                format_rate(app.metrics.rate("blackflower_network_resync_total"), "/s"),
            ),
            (
                "Upstream",
                format_byte_rate(app.metrics.rate_with_label(
                    "blackflower_network_udp_bytes_total",
                    "direction",
                    "upstream",
                )),
            ),
            (
                "Downstream",
                format_byte_rate(app.metrics.rate_with_label(
                    "blackflower_network_udp_bytes_total",
                    "direction",
                    "downstream",
                )),
            ),
        ],
    );
    draw_metric_series(
        frame,
        columns[1],
        "Snapshot actions",
        &app.metrics,
        "blackflower_network_snapshots_total",
        "action",
        true,
    );
}

fn draw_network_sparklines(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area.inner(Margin::new(1, 1)));
    frame.render_widget(panel("Host throughput · 60 s"), area);
    let receive = app.histories.network_receive_bytes.values();
    let transmit = app.histories.network_transmit_bytes.values();
    frame.render_widget(
        Sparkline::default()
            .data(&receive)
            .style(accent_style().add_modifier(Modifier::BOLD)),
        regions[0],
    );
    frame.render_widget(Sparkline::default().data(&transmit), regions[1]);
}

fn draw_prediction(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([
        Constraint::Percentage(38),
        Constraint::Percentage(31),
        Constraint::Percentage(31),
    ])
    .split(area);
    let columns = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(regions[0]);
    draw_prediction_history(frame, columns[0], app);
    draw_prediction_timing(frame, columns[1], app);
    draw_prediction_metric_series(
        frame,
        regions[1],
        "Tick executions by pass and result",
        &app.metrics,
        "blackflower_world_prediction_ticks_total",
        &["pass", "result"],
    );
    draw_prediction_metric_series(
        frame,
        regions[2],
        "Reconciliation outcomes",
        &app.metrics,
        "blackflower_world_prediction_reconciliations_total",
        &["result", "reason"],
    );
}

fn draw_prediction_timing(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Prediction timing",
        vec![
            (
                "Tick p50",
                format_millis(app.metrics.histogram_quantile(
                    "blackflower_world_prediction_tick_duration_seconds",
                    0.50,
                )),
            ),
            (
                "Tick p95 / p99",
                format!(
                    "{} / {}",
                    format_millis(app.metrics.histogram_quantile(
                        "blackflower_world_prediction_tick_duration_seconds",
                        0.95,
                    )),
                    format_millis(app.metrics.histogram_quantile(
                        "blackflower_world_prediction_tick_duration_seconds",
                        0.99,
                    )),
                ),
            ),
            (
                "Reconcile p95",
                format_millis(app.metrics.histogram_quantile(
                    "blackflower_world_prediction_reconciliation_duration_seconds",
                    0.95,
                )),
            ),
            (
                "Resim ticks p95",
                format_number(
                    app.metrics
                        .histogram_quantile("blackflower_world_prediction_resimulated_ticks", 0.95),
                ),
            ),
        ],
    );
}

fn draw_prediction_history(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let data = app.histories.prediction_p95_micros.values();
    frame.render_widget(
        Sparkline::default()
            .block(panel("Prediction tick p95 · µs · 60 s"))
            .data(&data)
            .style(accent_style().add_modifier(Modifier::BOLD)),
        area,
    );
}

fn draw_navigation_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([
        Constraint::Length(8),
        Constraint::Percentage(45),
        Constraint::Percentage(55),
    ])
    .split(area);
    draw_navigation_identity(frame, regions[0], app);
    draw_navigation_capabilities(frame, regions[1]);
    draw_controller_boundary(frame, regions[2]);
}

fn draw_navigation_identity(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let (major, minor, patch) = app.capabilities.recastnavigation_version;
    draw_key_values(
        frame,
        area,
        "Detour runtime",
        vec![
            (
                "Navigation asset",
                loaded(app.capabilities.navigation_loaded),
            ),
            ("RecastNavigation", format!("{major}.{minor}.{patch}")),
            (
                "Navmesh data version",
                app.capabilities.detour_navmesh_version.to_string(),
            ),
            ("Ownership", "agent main thread · !Send · !Sync".to_owned()),
            ("Dynamic avoidance", "not available".to_owned()),
        ],
    );
}

fn draw_navigation_capabilities(frame: &mut Frame<'_>, area: Rect) {
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    draw_key_values(
        frame,
        columns[0],
        "Available now",
        vec![
            ("Static navmesh", "cooked .bfnav".to_owned()),
            ("Pathfinding", "bounded Detour query".to_owned()),
            ("Area policy", "cooked query filter".to_owned()),
            ("Partial paths", "reported explicitly".to_owned()),
        ],
    );
    draw_key_values(
        frame,
        columns[1],
        "Deliberately absent",
        vec![
            ("Observation encoder", "not configured".to_owned()),
            ("Policy / model", "not configured".to_owned()),
            ("Steering", "not implemented".to_owned()),
            ("Background worker", "not started".to_owned()),
        ],
    );
}

fn draw_controller_boundary(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("The agent remains an ordinary client."),
            Line::from("Navigation reads only the cooked local mesh; snapshots and prediction arrive through ClientHarness."),
            Line::from("A future gameplay controller must translate semantic actions into validated ControlSubmission values."),
        ])
        .style(text_style())
        .block(panel("Controller boundary"))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_host(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows =
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[0]);
    draw_cpu(frame, columns[0], app);
    draw_memory(frame, columns[1], app);
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
    draw_host_io(frame, columns[0], app);
    draw_process(frame, columns[1], app);
}

fn draw_cpu(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let usage = app
        .metrics
        .value_with_label("node_cpu_usage_ratio", "cpu", "all");
    let ratio = usage.unwrap_or(0.0).clamp(0.0, 1.0);
    let regions = Layout::vertical([Constraint::Length(3), Constraint::Min(2)]).split(area);
    frame.render_widget(
        Gauge::default()
            .block(panel("CPU"))
            .ratio(ratio)
            .gauge_style(gauge_style())
            .label(format_percent(usage)),
        regions[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Load 1m / 5m / 15m  {} / {} / {}",
            format_decimal(app.metrics.value("node_load1")),
            format_decimal(app.metrics.value("node_load5")),
            format_decimal(app.metrics.value("node_load15")),
        ))
        .style(text_style())
        .block(panel("Load")),
        regions[1],
    );
}

fn draw_memory(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let total = app.metrics.value("node_memory_MemTotal_bytes");
    let available = app.metrics.value("node_memory_MemAvailable_bytes");
    let used = total.zip(available).map(|(total, free)| total - free);
    let swap_total = app.metrics.value("node_memory_SwapTotal_bytes");
    let swap_free = app.metrics.value("node_memory_SwapFree_bytes");
    let swap_used = swap_total.zip(swap_free).map(|(total, free)| total - free);
    draw_key_values(
        frame,
        area,
        "Memory",
        vec![
            (
                "Used / total",
                format!("{} / {}", format_bytes(used), format_bytes(total)),
            ),
            ("Available", format_bytes(available)),
            (
                "Swap used / total",
                format!("{} / {}", format_bytes(swap_used), format_bytes(swap_total)),
            ),
        ],
    );
}

fn draw_host_io(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Host I/O",
        vec![
            (
                "Disk read",
                format_byte_rate(app.metrics.rate("node_disk_read_bytes_total")),
            ),
            (
                "Disk write",
                format_byte_rate(app.metrics.rate("node_disk_written_bytes_total")),
            ),
            (
                "Network RX",
                format_byte_rate(app.metrics.rate("node_network_receive_bytes_total")),
            ),
            (
                "Network TX",
                format_byte_rate(app.metrics.rate("node_network_transmit_bytes_total")),
            ),
            (
                "Network errors",
                format_rate(
                    combine_rates(
                        app.metrics.rate("node_network_receive_errs_total"),
                        app.metrics.rate("node_network_transmit_errs_total"),
                    ),
                    "/s",
                ),
            ),
        ],
    );
}

fn draw_process(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Agent process",
        vec![
            (
                "CPU",
                format_cores(app.metrics.rate("process_cpu_seconds_total")),
            ),
            (
                "RSS / virtual",
                format!(
                    "{} / {}",
                    format_bytes(app.metrics.value("process_resident_memory_bytes")),
                    format_bytes(app.metrics.value("process_virtual_memory_bytes")),
                ),
            ),
            (
                "Open / max FDs",
                format!(
                    "{} / {}",
                    format_number(app.metrics.value("process_open_fds")),
                    format_number(app.metrics.value("process_max_fds")),
                ),
            ),
            (
                "Collector",
                if app
                    .metrics
                    .value("blackflower_observability_host_collector_up")
                    .is_some_and(|value| value > 0.5)
                {
                    "UP".to_owned()
                } else {
                    "—".to_owned()
                },
            ),
        ],
    );
}

fn draw_metric_series(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    metrics: &MetricStore,
    metric: &str,
    label: &str,
    rate: bool,
) {
    let rows = metrics
        .series(metric)
        .into_iter()
        .map(|sample| {
            let value = if rate {
                sample.label(label).map_or_else(
                    || "—".to_owned(),
                    |label_value| {
                        format_rate(metrics.rate_with_label(metric, label, label_value), "/s")
                    },
                )
            } else {
                format_number(Some(sample.value))
            };
            Row::new([sample.label(label).unwrap_or("—").to_owned(), value])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            rows,
            [Constraint::Percentage(65), Constraint::Percentage(35)],
        )
        .header(Row::new([label, if rate { "Rate" } else { "Value" }]).style(table_header_style()))
        .block(panel(title)),
        area,
    );
}

fn draw_prediction_metric_series(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    metrics: &MetricStore,
    metric: &str,
    labels: &[&str],
) {
    let rows = metrics
        .series(metric)
        .into_iter()
        .map(|sample| {
            let identity = labels
                .iter()
                .map(|label| sample.label(label).unwrap_or("—"))
                .collect::<Vec<_>>()
                .join(" / ");
            Row::new([identity, format_rate(metrics.rate_for_sample(sample), "/s")])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            rows,
            [Constraint::Percentage(65), Constraint::Percentage(35)],
        )
        .header(Row::new([labels.join(" / "), "Rate".to_owned()]).style(table_header_style()))
        .block(panel(title)),
        area,
    );
}

fn draw_key_values(frame: &mut Frame<'_>, area: Rect, title: &str, values: Vec<(&str, String)>) {
    let lines = values
        .into_iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(format!("{label:<20}"), muted_style()),
                Span::styled(value, text_style().add_modifier(Modifier::BOLD)),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).block(panel(title)), area);
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if let Some(error) = &app.metrics.last_error {
        frame.render_widget(
            Paragraph::new(format!(" Metrics error: {error}")).style(
                Style::default()
                    .fg(CODEX_BACKGROUND)
                    .bg(CODEX_ERROR)
                    .add_modifier(Modifier::BOLD),
            ),
            area,
        );
        return;
    }
    let page_help = match app.page {
        Page::Logs => " · / regex · l view · L capture · p pause · c clear",
        Page::Agents | Page::Sensorium => " · ↑/↓ agent",
        Page::Decisions => " · ↑/↓ agent · ←/→ decision · End live",
        Page::Overview | Page::Session | Page::Prediction | Page::Navigation | Page::Host => "",
    };
    frame.render_widget(
        Paragraph::new(format!(
            " Tab next · Shift+Tab previous · 1-9 page · ? help · q quit{page_help} · http://{}/metrics",
            app.metrics_address,
        ))
        .style(muted_style()),
        area,
    );
}

fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let popup = centered_rect(64, 21, area);
    frame.render_widget(Clear, popup);
    let text = vec![
        Line::from("1-9             select panel"),
        Line::from("Tab / Shift+Tab next / previous panel"),
        Line::from("q / Ctrl+C      stop foreground mode"),
        Line::from(""),
        Line::from("Logs"),
        Line::from("l / L            cycle view / capture level"),
        Line::from("/                edit regex (target + message + fields)"),
        Line::from("Esc              cancel edit or clear regex"),
        Line::from("p / End          pause / resume follow"),
        Line::from("↑ ↓ PgUp PgDn    scroll"),
        Line::from("c                clear local log buffer"),
        Line::from(""),
        Line::from("Agents / Sensorium / Decisions"),
        Line::from("↑ / ↓            select real runtime agent"),
        Line::from("← / → / End      browse decisions / return live"),
        Line::from(""),
        Line::from("Press any key to close help."),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .style(surface_style())
            .block(popup_panel("Keyboard shortcuts"))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn draw_filter_editor(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(editor) = &app.logs.filter_editor else {
        return;
    };
    let popup = centered_rect(72, 7, area);
    frame.render_widget(Clear, popup);
    let error = editor
        .error
        .as_deref()
        .unwrap_or("Enter apply · Esc cancel");
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!("/{}", editor.draft)),
            Line::from(""),
            Line::from(Span::styled(error, muted_surface_style())),
        ])
        .style(surface_style())
        .block(popup_panel("Log filter")),
        popup,
    );
}

fn draw_too_small(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(format!(
            "›_ AGENT FOREGROUND\n\nTerminal too small: {}x{}\nMinimum: {MIN_WIDTH}x{MIN_HEIGHT}\n\nq or Ctrl+C to stop",
            area.width, area.height,
        ))
        .alignment(Alignment::Center)
        .style(base_style())
        .block(panel("Terminal size")),
        area,
    );
}

fn log_row(event: &ForegroundLogEvent) -> Row<'static> {
    let fields = format_fields(event);
    Row::new([
        format!("+{:>8.3}s", event.elapsed.as_secs_f64()),
        event.level.as_str().to_owned(),
        event.target.clone(),
        if fields.is_empty() {
            event.message.clone()
        } else {
            format!("{}  {fields}", event.message)
        },
    ])
    .style(log_style(event.level))
}

fn log_line(event: &ForegroundLogEvent) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{:>5} ", event.level.as_str()),
            log_style(event.level),
        ),
        Span::styled(format!("{:<28} ", event.target), muted_style()),
        Span::styled(event.message.clone(), text_style()),
    ])
}

fn format_fields(event: &ForegroundLogEvent) -> String {
    event
        .fields
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn log_style(level: ForegroundLogLevel) -> Style {
    match level {
        ForegroundLogLevel::Off | ForegroundLogLevel::Trace | ForegroundLogLevel::Debug => {
            muted_style()
        }
        ForegroundLogLevel::Info => text_style(),
        ForegroundLogLevel::Warn => Style::default()
            .fg(CODEX_WARNING)
            .bg(CODEX_BACKGROUND)
            .add_modifier(Modifier::BOLD),
        ForegroundLogLevel::Error => Style::default()
            .fg(CODEX_ERROR)
            .bg(CODEX_BACKGROUND)
            .add_modifier(Modifier::BOLD),
    }
}

fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style())
        .style(base_style())
        .title(Line::from(vec![
            Span::styled(" › ", accent_style().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{title} "),
                text_style().add_modifier(Modifier::BOLD),
            ),
        ]))
}

fn popup_panel(title: &str) -> Block<'static> {
    panel(title)
        .style(surface_style())
        .border_style(Style::default().fg(CODEX_ACCENT).bg(CODEX_SURFACE))
}

fn scrape_status(app: &App) -> String {
    if app.metrics.last_success.is_none() {
        return "waiting".to_owned();
    }
    if app.metrics.last_error.is_some()
        || app
            .metrics
            .scrape_age(Instant::now())
            .is_some_and(|age| age > Duration::from_secs(3))
    {
        return "stale".to_owned();
    }
    let age = app
        .metrics
        .scrape_age(Instant::now())
        .unwrap_or(Duration::ZERO);
    format!("live · age {:.1}s", age.as_secs_f64())
}

fn scrape_style(app: &App) -> Style {
    if app.metrics.last_success.is_none() {
        muted_style()
    } else if app.metrics.last_error.is_some()
        || app
            .metrics
            .scrape_age(Instant::now())
            .is_some_and(|age| age > Duration::from_secs(3))
    {
        Style::default()
            .fg(CODEX_WARNING)
            .bg(CODEX_BACKGROUND)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(CODEX_SUCCESS)
            .bg(CODEX_BACKGROUND)
            .add_modifier(Modifier::BOLD)
    }
}

fn capability_style(configured: bool) -> Style {
    if configured {
        Style::default()
            .fg(CODEX_SUCCESS)
            .bg(CODEX_BACKGROUND)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(CODEX_WARNING)
            .bg(CODEX_BACKGROUND)
            .add_modifier(Modifier::BOLD)
    }
}

fn configured(value: bool) -> String {
    if value {
        "configured".to_owned()
    } else {
        "not configured".to_owned()
    }
}

fn loaded(value: bool) -> String {
    if value {
        "loaded".to_owned()
    } else {
        "not loaded".to_owned()
    }
}

const fn base_style() -> Style {
    Style::new().fg(CODEX_TEXT).bg(CODEX_BACKGROUND)
}

const fn text_style() -> Style {
    base_style()
}

const fn muted_style() -> Style {
    Style::new().fg(CODEX_MUTED).bg(CODEX_BACKGROUND)
}

const fn accent_style() -> Style {
    Style::new().fg(CODEX_ACCENT).bg(CODEX_BACKGROUND)
}

const fn border_style() -> Style {
    Style::new().fg(CODEX_BORDER).bg(CODEX_BACKGROUND)
}

const fn surface_style() -> Style {
    Style::new().fg(CODEX_TEXT).bg(CODEX_SURFACE)
}

const fn muted_surface_style() -> Style {
    Style::new().fg(CODEX_MUTED).bg(CODEX_SURFACE)
}

fn table_header_style() -> Style {
    accent_style().add_modifier(Modifier::BOLD)
}

fn gauge_style() -> Style {
    Style::default()
        .fg(CODEX_ACCENT)
        .bg(CODEX_SURFACE)
        .add_modifier(Modifier::BOLD)
}

const fn page_index(page: Page) -> usize {
    match page {
        Page::Overview => 0,
        Page::Logs => 1,
        Page::Agents => 2,
        Page::Sensorium => 3,
        Page::Decisions => 4,
        Page::Session => 5,
        Page::Prediction => 6,
        Page::Navigation => 7,
        Page::Host => 8,
    }
}

const fn session_state(state: SessionState) -> &'static str {
    match state {
        SessionState::Connecting => "connecting",
        SessionState::Secure => "secure",
        SessionState::Negotiating => "negotiating",
        SessionState::ContentChecking => "content",
        SessionState::Synchronizing => "syncing",
        SessionState::Active => "active",
        SessionState::Resynchronizing => "resyncing",
        SessionState::Closing => "closing",
    }
}

fn format_age(age: Duration) -> String {
    if age < Duration::from_secs(1) {
        format!("{} ms", age.as_millis())
    } else {
        format!("{:.1} s", age.as_secs_f64())
    }
}

fn format_number(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.0}"))
}

fn format_decimal(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2}"))
}

fn format_rate(value: Option<f64>, unit: &str) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2} {unit}"))
}

fn format_millis(seconds: Option<f64>) -> String {
    seconds.map_or_else(
        || "—".to_owned(),
        |seconds| format!("{:.3} ms", seconds * 1_000.0),
    )
}

fn format_cores(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |cores| format!("{cores:.2} cores"))
}

pub(crate) fn format_bytes(value: Option<f64>) -> String {
    let Some(bytes) = value else {
        return "—".to_owned();
    };
    const KIB: f64 = 1_024.0;
    const MIB: f64 = KIB * 1_024.0;
    const GIB: f64 = MIB * 1_024.0;
    const TIB: f64 = GIB * 1_024.0;
    if bytes >= TIB {
        format!("{:.2} TiB", bytes / TIB)
    } else if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn format_byte_rate(value: Option<f64>) -> String {
    value.map_or_else(
        || "—".to_owned(),
        |value| format!("{}/s", format_bytes(Some(value))),
    )
}

fn format_percent(ratio: Option<f64>) -> String {
    ratio.map_or_else(|| "—".to_owned(), |ratio| format!("{:.1}%", ratio * 100.0))
}

fn combine_rates(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + right),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

pub(crate) fn format_uptime(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

fn centered_rect(width_percent: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height.min(area.height)),
        Constraint::Fill(1),
    ])
    .split(area);
    let horizontal = Layout::horizontal([
        Constraint::Percentage((100 - width_percent) / 2),
        Constraint::Percentage(width_percent),
        Constraint::Percentage((100 - width_percent) / 2),
    ])
    .split(vertical[1]);
    horizontal[1]
}

#[cfg(test)]
#[path = "../../tests/unit/foreground_render.rs"]
mod tests;
