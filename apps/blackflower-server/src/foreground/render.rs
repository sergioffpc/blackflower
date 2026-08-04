use std::time::{Duration, Instant};

use blackflower_observability::{ForegroundLogEvent, ForegroundLogLevel};
use blackflower_world_simulation::{SIMULATION_TICK_RATE_HZ, SimulationPhase};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, Gauge, Paragraph, Row, Sparkline, Table, Tabs, Wrap,
};

use super::app::{App, Page};
use super::metrics::MetricStore;

const MIN_WIDTH: u16 = 72;
const MIN_HEIGHT: u16 = 20;

const CODEX_BACKGROUND: Color = Color::Rgb(14, 29, 57);
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
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(content);
    draw_header(frame, regions[0], app);
    match app.page {
        Page::Overview => draw_overview(frame, regions[1], app),
        Page::Logs => draw_logs(frame, regions[1], app),
        Page::Simulation => draw_simulation(frame, regions[1], app),
        Page::Network => draw_network(frame, regions[1], app),
        Page::World => draw_world(frame, regions[1], app),
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
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(area);
    draw_command(frame, regions[0], app);
    draw_identity(frame, regions[1], app);
    draw_navigation(frame, regions[2], area.width, app.page);
}

fn draw_command(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("› ", accent_style()),
            Span::styled(app.service_name, text_style().add_modifier(Modifier::BOLD)),
            Span::styled(" --foreground", muted_style()),
        ])),
        area,
    );
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

fn draw_navigation(frame: &mut Frame<'_>, area: Rect, header_width: u16, page: Page) {
    let titles = Page::ALL
        .iter()
        .enumerate()
        .map(|(index, page)| {
            let title = if header_width < 100 {
                page.short_title()
            } else {
                page.title()
            };
            Line::from(format!(" {} {title} ", index + 1))
        })
        .collect::<Vec<_>>();
    let navigation = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);
    frame.render_widget(Paragraph::new("›").style(accent_style()), navigation[0]);
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
        navigation[1],
    );
}

fn draw_overview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_overview_metrics(frame, area, app);
}

#[allow(
    clippy::too_many_lines,
    reason = "overview metric declarations stay adjacent so the operator summary remains auditable"
)]
fn draw_overview_metrics(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(38),
            Constraint::Percentage(37),
            Constraint::Percentage(25),
        ])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);
    draw_key_values(
        frame,
        columns[0],
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
    draw_key_values(
        frame,
        columns[1],
        "Simulation",
        vec![
            (
                "Tick rate",
                format_rate(
                    app.metrics.rate("blackflower_world_simulation_ticks_total"),
                    "Hz",
                ),
            ),
            (
                "p95",
                format_millis(app.metrics.histogram_quantile(
                    "blackflower_world_simulation_tick_duration_seconds",
                    0.95,
                )),
            ),
            (
                "Budget",
                format!("{:.2} ms", tick_budget_seconds() * 1_000.0),
            ),
            (
                "Misses",
                format_rate(
                    app.metrics
                        .rate("blackflower_world_simulation_deadline_misses_total"),
                    "/s",
                ),
            ),
        ],
    );

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);
    draw_key_values(
        frame,
        columns[0],
        "World",
        vec![
            (
                "Worlds",
                format_number(app.metrics.value("blackflower_ecs_active_worlds")),
            ),
            (
                "Entities",
                format_number(app.metrics.value("blackflower_ecs_entities")),
            ),
            (
                "Systems",
                format_number(app.metrics.value("blackflower_ecs_systems")),
            ),
            (
                "Allocations",
                format_number(app.metrics.value("blackflower_ecs_allocations_outstanding")),
            ),
        ],
    );
    draw_key_values(
        frame,
        columns[1],
        "Network",
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
    draw_recent_logs(frame, rows[2], app);
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

#[allow(
    clippy::too_many_lines,
    reason = "log table, filter state, and bounded-buffer health form one visual contract"
)]
fn draw_logs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
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
            Span::styled(
                state,
                Style::default()
                    .fg(CODEX_TEXT)
                    .bg(CODEX_SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(panel("Filters")),
        regions[0],
    );

    let visible_rows = usize::from(regions[1].height.saturating_sub(3));
    let (events, first, total) = app.logs.visible(visible_rows);
    let rows = events.into_iter().map(log_row).collect::<Vec<_>>();
    let table = Table::new(
        rows,
        [
            Constraint::Length(11),
            Constraint::Length(7),
            Constraint::Length(28),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(["Elapsed", "Level", "Target", "Message and fields"]).style(table_header_style()),
    )
    .block(panel("Structured logs"))
    .column_spacing(1);
    frame.render_widget(table, regions[1]);
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
        regions[2],
    );
}

fn draw_simulation(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Percentage(45),
            Constraint::Percentage(55),
        ])
        .split(area);
    let p95 = app
        .metrics
        .histogram_quantile("blackflower_world_simulation_tick_duration_seconds", 0.95);
    let ratio = p95.map_or(0.0, |seconds| {
        (seconds / tick_budget_seconds()).clamp(0.0, 1.0)
    });
    frame.render_widget(
        Gauge::default()
            .block(panel(&format!(
                "Authoritative tick · {SIMULATION_TICK_RATE_HZ} Hz"
            )))
            .gauge_style(gauge_style())
            .ratio(ratio)
            .label(format!(
                "p95 {} / {:.2} ms budget",
                format_millis(p95),
                tick_budget_seconds() * 1_000.0
            )),
        regions[0],
    );

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(regions[1]);
    draw_tick_distribution(frame, columns[0], app);
    draw_tick_outcomes(frame, columns[1], app);
    draw_phase_table(frame, regions[2], app);
}

fn draw_tick_distribution(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(4)])
        .split(area);
    let data = app.histories.tick_p95_micros.values();
    frame.render_widget(
        Sparkline::default()
            .block(panel("Tick p95 · last 60 s"))
            .data(&data)
            .style(accent_style().add_modifier(Modifier::BOLD)),
        regions[0],
    );
    draw_key_values(
        frame,
        regions[1],
        "Distribution",
        vec![
            (
                "p50",
                format_millis(app.metrics.histogram_quantile(
                    "blackflower_world_simulation_tick_duration_seconds",
                    0.50,
                )),
            ),
            (
                "p95 / p99",
                format!(
                    "{} / {}",
                    format_millis(app.metrics.histogram_quantile(
                        "blackflower_world_simulation_tick_duration_seconds",
                        0.95,
                    )),
                    format_millis(app.metrics.histogram_quantile(
                        "blackflower_world_simulation_tick_duration_seconds",
                        0.99,
                    )),
                ),
            ),
        ],
    );
}

fn draw_tick_outcomes(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Outcomes",
        vec![
            (
                "Completed",
                format_rate(
                    app.metrics.rate_with_label(
                        "blackflower_world_simulation_ticks_total",
                        "result",
                        "completed",
                    ),
                    "/s",
                ),
            ),
            (
                "Stopped",
                format_rate(
                    app.metrics.rate_with_label(
                        "blackflower_world_simulation_ticks_total",
                        "result",
                        "stopped",
                    ),
                    "/s",
                ),
            ),
            (
                "Failed",
                format_rate(
                    app.metrics.rate_with_label(
                        "blackflower_world_simulation_ticks_total",
                        "result",
                        "failed",
                    ),
                    "/s",
                ),
            ),
            (
                "Deadline misses",
                format_rate(
                    app.metrics
                        .rate("blackflower_world_simulation_deadline_misses_total"),
                    "/s",
                ),
            ),
        ],
    );
}

fn draw_phase_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = SimulationPhase::ORDER
        .iter()
        .map(|phase| {
            Row::new([
                Cell::from(phase.name()),
                Cell::from(format_rate(
                    app.metrics.rate_with_label(
                        "blackflower_world_simulation_system_executions_total",
                        "phase",
                        phase.name(),
                    ),
                    "/s",
                )),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            rows,
            [Constraint::Percentage(70), Constraint::Percentage(30)],
        )
        .header(Row::new(["Phase", "System executions"]).style(table_header_style()))
        .block(panel("Pipeline phases")),
        area,
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "network layout keeps transport, queue, and replication sections visibly coordinated"
)]
fn draw_network(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Percentage(45),
            Constraint::Percentage(55),
        ])
        .split(area);
    draw_key_values(
        frame,
        regions[0],
        "QUIC transport",
        vec![
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
                "Inputs",
                format_rate(app.metrics.rate("blackflower_network_inputs_total"), "/s"),
            ),
        ],
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(regions[1]);
    draw_network_sparklines(frame, columns[0], app);
    draw_metric_series(
        frame,
        columns[1],
        "Queue depth",
        &app.metrics,
        "blackflower_network_queue_depth",
        "queue",
        false,
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(regions[2]);
    draw_network_health(frame, columns[0], app);
    draw_replication(frame, columns[1], app);
}

fn draw_network_sparklines(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
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

fn draw_network_health(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Network health",
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
                "Voice",
                format_rate(
                    app.metrics.rate("blackflower_network_voice_packets_total"),
                    "/s",
                ),
            ),
        ],
    );
}

fn draw_replication(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_metric_series(
        frame,
        area,
        "Snapshot actions",
        &app.metrics,
        "blackflower_network_snapshots_total",
        "action",
        true,
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "world layout keeps ECS and acoustics metric mappings adjacent and auditable"
)]
fn draw_world(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Percentage(45),
            Constraint::Percentage(55),
        ])
        .split(area);
    draw_key_values(
        frame,
        regions[0],
        "World · ECS",
        vec![
            (
                "Active worlds",
                format_number(app.metrics.value("blackflower_ecs_active_worlds")),
            ),
            (
                "Entities",
                format_number(app.metrics.value("blackflower_ecs_entities")),
            ),
            (
                "Tables",
                format_number(app.metrics.value("blackflower_ecs_tables")),
            ),
            (
                "Queries",
                format_number(app.metrics.value("blackflower_ecs_queries")),
            ),
            (
                "Systems",
                format_number(app.metrics.value("blackflower_ecs_systems")),
            ),
        ],
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(regions[1]);
    draw_key_values(
        frame,
        columns[0],
        "ECS health",
        vec![
            (
                "Allocations",
                format_number(app.metrics.value("blackflower_ecs_allocations_outstanding")),
            ),
            (
                "Callback failures",
                format_number(app.metrics.sum("blackflower_ecs_callback_failures_total")),
            ),
            (
                "Registrations",
                format_number(app.metrics.sum("blackflower_ecs_registrations_total")),
            ),
            (
                "Tick p95",
                format_millis(
                    app.metrics
                        .histogram_quantile("blackflower_ecs_tick_duration_seconds", 0.95),
                ),
            ),
        ],
    );
    draw_key_values(
        frame,
        columns[1],
        "Acoustics",
        vec![
            (
                "Candidate pairs p95",
                format_number(
                    app.metrics
                        .histogram_quantile("blackflower_acoustic_candidate_pairs", 0.95),
                ),
            ),
            (
                "Direct pairs p95",
                format_number(
                    app.metrics
                        .histogram_quantile("blackflower_acoustic_direct_pairs", 0.95),
                ),
            ),
            (
                "Observations",
                format_rate(
                    app.metrics.rate("blackflower_acoustic_observations_total"),
                    "/s",
                ),
            ),
            (
                "Deferred",
                format_rate(
                    app.metrics
                        .rate("blackflower_acoustic_deferred_indirect_pairs_total"),
                    "/s",
                ),
            ),
        ],
    );
    draw_metric_series(
        frame,
        regions[2],
        "ECS tick operations",
        &app.metrics,
        "blackflower_ecs_ticks_total",
        "operation",
        true,
    );
}

fn draw_host(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Percentage(45),
            Constraint::Percentage(55),
        ])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(regions[0]);
    draw_cpu(frame, columns[0], app);
    draw_memory(frame, columns[1], app);
    draw_filesystems(frame, regions[1], app);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(regions[2]);
    draw_host_io(frame, columns[0], app);
    draw_process(frame, columns[1], app);
}

fn draw_cpu(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let usage = app
        .metrics
        .value_with_label("node_cpu_usage_ratio", "cpu", "all");
    let ratio = usage.unwrap_or(0.0).clamp(0.0, 1.0);
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(2)])
        .split(area);
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
    let used = total
        .zip(available)
        .map(|(total, available)| total - available);
    let ratio = used
        .zip(total)
        .map_or(0.0, |(used, total)| safe_ratio(used, total));
    let swap_total = app.metrics.value("node_memory_SwapTotal_bytes");
    let swap_free = app.metrics.value("node_memory_SwapFree_bytes");
    let swap_used = swap_total.zip(swap_free).map(|(total, free)| total - free);
    draw_key_values(
        frame,
        area,
        &format!("Memory · {:.0}%", ratio * 100.0),
        vec![
            (
                "Used / total",
                format!("{} / {}", format_bytes(used), format_bytes(total)),
            ),
            (
                "Available",
                format_bytes(app.metrics.value("node_memory_MemAvailable_bytes")),
            ),
            (
                "Swap used / total",
                format!("{} / {}", format_bytes(swap_used), format_bytes(swap_total)),
            ),
        ],
    );
}

fn draw_filesystems(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app
        .metrics
        .series("node_filesystem_size_bytes")
        .into_iter()
        .map(|size| {
            let mount = size.label("mountpoint").unwrap_or("—");
            let filesystem = size.label("fstype").unwrap_or("—");
            let available = app
                .metrics
                .series("node_filesystem_avail_bytes")
                .into_iter()
                .find(|sample| sample.label("mountpoint") == Some(mount))
                .map(|sample| sample.value);
            let used = available.map(|available| size.value - available);
            Row::new([
                mount.to_owned(),
                filesystem.to_owned(),
                format_bytes(used),
                format_bytes(available),
                format_percent(used.map(|used| safe_ratio(used, size.value))),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Percentage(28),
                Constraint::Percentage(18),
                Constraint::Percentage(18),
                Constraint::Percentage(18),
                Constraint::Percentage(18),
            ],
        )
        .header(
            Row::new(["Mount", "Filesystem", "Used", "Available", "Usage"])
                .style(table_header_style()),
        )
        .block(panel("Filesystems")),
        area,
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
        "Server process",
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
    let page_help = if app.page == Page::Logs {
        " · / regex · l view · L capture · p pause · c clear"
    } else {
        ""
    };
    frame.render_widget(
        Paragraph::new(format!(
            " Tab next · Shift+Tab previous · 1-6 page · ? help · q quit{page_help} · http://{}/metrics",
            app.metrics_address,
        ))
        .style(muted_style()),
        area,
    );
}

fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let popup = centered_rect(64, 17, area);
    frame.render_widget(Clear, popup);
    let text = vec![
        Line::from("1-6             select panel"),
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
            "›_ SERVER FOREGROUND\n\nTerminal too small: {}x{}\nMinimum: {MIN_WIDTH}x{MIN_HEIGHT}\n\nq or Ctrl+C to stop",
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
        Page::Simulation => 2,
        Page::Network => 3,
        Page::World => 4,
        Page::Host => 5,
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

fn format_bytes(value: Option<f64>) -> String {
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

fn safe_ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator <= 0.0 {
        0.0
    } else {
        (numerator / denominator).clamp(0.0, 1.0)
    }
}

fn combine_rates(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + right),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn tick_budget_seconds() -> f64 {
    let rate = u32::try_from(SIMULATION_TICK_RATE_HZ).unwrap_or(u32::MAX);
    1.0 / f64::from(rate)
}

fn format_uptime(duration: Duration) -> String {
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
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height.min(area.height)),
            Constraint::Fill(1),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
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
