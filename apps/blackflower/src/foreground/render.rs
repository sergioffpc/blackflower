use std::time::{Duration, Instant};

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

const MIN_WIDTH: u16 = 72;
const MIN_HEIGHT: u16 = 20;

pub(crate) const BACKGROUND: Color = Color::Rgb(14, 29, 57);
const SURFACE: Color = Color::Rgb(39, 55, 83);
const BORDER: Color = Color::Rgb(72, 92, 122);
const TEXT: Color = Color::Rgb(219, 224, 232);
const MUTED: Color = Color::Rgb(132, 146, 166);
const ACCENT: Color = Color::Rgb(79, 195, 247);
const SUCCESS: Color = Color::Rgb(101, 214, 173);
const WARNING: Color = Color::Rgb(243, 201, 105);
const ERROR: Color = Color::Rgb(255, 122, 144);

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
        Page::Session => draw_session(frame, regions[1], app),
        Page::Prediction => draw_prediction(frame, regions[1], app),
        Page::Runtime => draw_runtime(frame, regions[1], app),
        Page::Presentation => draw_presentation(frame, regions[1], app),
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
            Span::styled("    client: ", muted_style()),
            Span::styled("native + terminal", success_style()),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(identity).block(panel("Client process")),
        regions[1],
    );
    draw_tabs(frame, regions[2], area.width, app.page);
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
                    .fg(TEXT)
                    .bg(SURFACE)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("  "),
        regions[1],
    );
}

fn draw_overview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Percentage(40),
        Constraint::Percentage(35),
        Constraint::Percentage(25),
    ])
    .split(area);
    let columns = halves(rows[0]);
    draw_key_values(
        frame,
        columns[0],
        "Client composition",
        vec![
            (
                "Native application",
                configured(app.capabilities.native_application),
            ),
            (
                "Session / harness",
                configured(app.capabilities.session_configured),
            ),
            (
                "Movement prediction",
                configured(app.capabilities.prediction_configured),
            ),
            (
                "Presentation",
                configured(app.capabilities.presentation_configured),
            ),
            (
                "Renderer backend",
                configured(app.capabilities.renderer_configured),
            ),
        ],
    );
    draw_process_summary(frame, columns[1], app);

    let columns = halves(rows[1]);
    draw_session_summary(frame, columns[0], app);
    draw_frame_summary(frame, columns[1], app);
    draw_recent_logs(frame, rows[2], app);
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
        "Ordinary-client session",
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
                "Controls",
                format_rate(app.metrics.rate("blackflower_network_inputs_total"), "/s"),
            ),
            (
                "Resyncs",
                format_rate(app.metrics.rate("blackflower_network_resync_total"), "/s"),
            ),
        ],
    );
}

fn draw_frame_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Client runtime",
        vec![
            (
                "Presentation",
                format_rate(
                    app.metrics
                        .rate("blackflower_world_presentation_frames_total"),
                    " fps",
                ),
            ),
            (
                "Frame p95",
                format_millis(app.metrics.histogram_quantile(
                    "blackflower_world_presentation_frame_duration_seconds",
                    0.95,
                )),
            ),
            (
                "Prediction",
                configured(app.capabilities.prediction_configured),
            ),
            (
                "Renderer backend",
                configured(app.capabilities.renderer_configured),
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
            Span::styled(app.logs.control.level().as_str(), bold_style()),
            Span::raw("  View "),
            Span::styled(app.logs.view_level.as_str(), bold_style()),
            Span::raw("  Regex "),
            Span::styled(filter, bold_style()),
            Span::raw("  "),
            Span::styled(state, surface_style().add_modifier(Modifier::BOLD)),
        ]))
        .block(panel("Filters")),
        regions[0],
    );
    draw_log_table(frame, regions[1], regions[2], app);
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
    frame.render_widget(
        Paragraph::new(format!(
            "{}-{} of {} · dropped {}{}",
            if total == 0 { 0 } else { first + 1 },
            first + visible_rows.min(total.saturating_sub(first)),
            total,
            app.logs.control.dropped_events(),
            if app.logs.disconnected() {
                " · source closed"
            } else {
                ""
            },
        ))
        .style(muted_style()),
        status_area,
    );
}

fn draw_session(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(area);
    draw_session_identity(frame, rows[0], app);
    draw_session_activity(frame, rows[1], app);
    draw_session_health(frame, rows[2], app);
}

fn draw_session_identity(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Client session",
        vec![
            ("Harness", configured(app.capabilities.session_configured)),
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
        ],
    );
}

fn draw_session_activity(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns = halves(area);
    draw_network_history(frame, columns[0], app);
    draw_series(
        frame,
        columns[1],
        "Bounded queues",
        &app.metrics,
        "blackflower_network_queue_depth",
        &["queue"],
        false,
    );
}

fn draw_session_health(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns = halves(area);
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
        ],
    );
    draw_series(
        frame,
        columns[1],
        "Snapshot actions",
        &app.metrics,
        "blackflower_network_snapshots_total",
        &["action"],
        true,
    );
}

fn draw_network_history(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let inner = area.inner(Margin::new(1, 1));
    let rows =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);
    let receive = app.histories.network_receive_bytes.values();
    let transmit = app.histories.network_transmit_bytes.values();
    frame.render_widget(panel("Host network · RX / TX · 60 s"), area);
    frame.render_widget(
        Sparkline::default().data(&receive).style(accent_style()),
        rows[0],
    );
    frame.render_widget(Sparkline::default().data(&transmit), rows[1]);
}

fn draw_prediction(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Length(9), Constraint::Min(8)]).split(area);
    let columns = halves(rows[0]);
    draw_prediction_contract(frame, columns[0]);
    draw_prediction_activity(frame, columns[1], app);
    let columns = halves(rows[1]);
    draw_series(
        frame,
        columns[0],
        "Reconciliation outcomes",
        &app.metrics,
        "blackflower_world_prediction_reconciliations_total",
        &["result", "reason"],
        true,
    );
    draw_prediction_boundary(frame, columns[1]);
}

fn draw_prediction_contract(frame: &mut Frame<'_>, area: Rect) {
    draw_key_values(
        frame,
        area,
        "Movement prediction",
        vec![
            ("Mode", "FORWARD + RESIMULATION".to_owned()),
            ("PredictionWorld", "configured".to_owned()),
            ("Gameplay state", "movement schema v1".to_owned()),
            ("Controls", "WASD + mouse @ 60 Hz".to_owned()),
            ("Reconciliation", "tolerance based".to_owned()),
        ],
    );
}

fn draw_prediction_activity(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Prediction activity",
        vec![
            (
                "Configured",
                configured(app.capabilities.prediction_configured),
            ),
            (
                "Prediction ticks",
                format_rate(
                    app.metrics.rate("blackflower_world_prediction_ticks_total"),
                    "/s",
                ),
            ),
            (
                "Prediction tick p95",
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
            (
                "Resimulated p95",
                format_number(
                    app.metrics
                        .histogram_quantile("blackflower_world_prediction_resimulated_ticks", 0.95),
                ),
            ),
        ],
    );
}

fn draw_prediction_boundary(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Continuous state reconciles by protocol-v1 margins, not bit identity:"),
            Line::from("position <= 2 cm · velocity <= 5 cm/s · orientation <= 0.5°."),
            Line::from("Controlled entity and grounded state remain exact comparisons."),
            Line::from("Input is neutral outside focused gameplay cursor capture."),
        ])
        .style(text_style())
        .block(panel("Operational boundary"))
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_runtime(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Length(8),
        Constraint::Percentage(45),
        Constraint::Percentage(55),
    ])
    .split(area);
    draw_key_values(
        frame,
        rows[0],
        "Client runtime / world",
        vec![
            (
                "Presentation world",
                configured(app.capabilities.presentation_configured),
            ),
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
                "Systems",
                format_number(app.metrics.value("blackflower_ecs_systems")),
            ),
        ],
    );
    draw_runtime_health(frame, rows[1], app);
    draw_series(
        frame,
        rows[2],
        "ECS tick executions",
        &app.metrics,
        "blackflower_ecs_ticks_total",
        &["operation", "result"],
        true,
    );
}

fn draw_runtime_health(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns = halves(area);
    draw_ecs_health(frame, columns[0], app);
    draw_tick_internals(frame, columns[1], app);
}

fn draw_ecs_health(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "ECS health",
        vec![
            (
                "Queries",
                format_number(app.metrics.value("blackflower_ecs_queries")),
            ),
            (
                "Allocations",
                format_number(app.metrics.value("blackflower_ecs_allocations_outstanding")),
            ),
            (
                "Registrations",
                format_number(app.metrics.sum("blackflower_ecs_registrations_total")),
            ),
            (
                "Callback failures",
                format_number(app.metrics.sum("blackflower_ecs_callback_failures_total")),
            ),
        ],
    );
}

fn draw_tick_internals(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Tick internals p95",
        vec![
            (
                "Duration",
                format_millis(
                    app.metrics
                        .histogram_quantile("blackflower_ecs_tick_duration_seconds", 0.95),
                ),
            ),
            (
                "Systems ran",
                format_number(
                    app.metrics
                        .histogram_quantile("blackflower_ecs_tick_systems_ran", 0.95),
                ),
            ),
            (
                "Merges",
                format_number(
                    app.metrics
                        .histogram_quantile("blackflower_ecs_tick_merges", 0.95),
                ),
            ),
            (
                "Rematches",
                format_number(
                    app.metrics
                        .histogram_quantile("blackflower_ecs_tick_rematches", 0.95),
                ),
            ),
        ],
    );
}

fn draw_presentation(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Percentage(40),
        Constraint::Percentage(30),
        Constraint::Percentage(30),
    ])
    .split(area);
    draw_presentation_timing(frame, rows[0], app);
    draw_series(
        frame,
        rows[1],
        "Frame outcomes",
        &app.metrics,
        "blackflower_world_presentation_frames_total",
        &["result"],
        true,
    );
    draw_series(
        frame,
        rows[2],
        "Presentation phases",
        &app.metrics,
        "blackflower_world_presentation_system_executions_total",
        &["phase"],
        true,
    );
}

fn draw_presentation_timing(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns = halves(area);
    let data = app.histories.presentation_p95_micros.values();
    frame.render_widget(
        Sparkline::default()
            .block(panel("Presentation frame p95 · µs · 60 s"))
            .data(&data)
            .style(accent_style()),
        columns[0],
    );
    draw_key_values(
        frame,
        columns[1],
        "Frame timing",
        vec![
            (
                "Frames",
                format_rate(
                    app.metrics
                        .rate("blackflower_world_presentation_frames_total"),
                    " fps",
                ),
            ),
            (
                "Duration p50 / p95",
                format!(
                    "{} / {}",
                    format_millis(app.metrics.histogram_quantile(
                        "blackflower_world_presentation_frame_duration_seconds",
                        0.50,
                    )),
                    format_millis(app.metrics.histogram_quantile(
                        "blackflower_world_presentation_frame_duration_seconds",
                        0.95,
                    )),
                ),
            ),
            (
                "Frame delta p95",
                format_millis(app.metrics.histogram_quantile(
                    "blackflower_world_presentation_frame_delta_seconds",
                    0.95,
                )),
            ),
            (
                "Renderer backend",
                configured(app.capabilities.renderer_configured),
            ),
        ],
    );
}

fn draw_host(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows =
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);
    let columns = halves(rows[0]);
    draw_cpu(frame, columns[0], app);
    draw_memory(frame, columns[1], app);
    let columns = halves(rows[1]);
    draw_host_io(frame, columns[0], app);
    draw_process_detail(frame, columns[1], app);
}

fn draw_cpu(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let usage = app
        .metrics
        .value_with_label("node_cpu_usage_ratio", "cpu", "all");
    let ratio = usage.unwrap_or(0.0).clamp(0.0, 1.0);
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(2)]).split(area);
    frame.render_widget(
        Gauge::default()
            .block(panel("CPU"))
            .ratio(ratio)
            .gauge_style(accent_style())
            .label(format_percent(usage)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Load 1m / 5m / 15m  {} / {} / {}",
            format_decimal(app.metrics.value("node_load1")),
            format_decimal(app.metrics.value("node_load5")),
            format_decimal(app.metrics.value("node_load15")),
        ))
        .block(panel("Load")),
        rows[1],
    );
}

fn draw_memory(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let total = app.metrics.value("node_memory_MemTotal_bytes");
    let available = app.metrics.value("node_memory_MemAvailable_bytes");
    let used = total.zip(available).map(|(total, free)| total - free);
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
                "Process RSS",
                format_bytes(app.metrics.value("process_resident_memory_bytes")),
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
        ],
    );
}

fn draw_process_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_key_values(
        frame,
        area,
        "Client process",
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
                "Host collector",
                up_or_unavailable(
                    app.metrics
                        .value("blackflower_observability_host_collector_up"),
                ),
            ),
        ],
    );
}

fn draw_series(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    metrics: &MetricStore,
    metric: &str,
    labels: &[&str],
    rate: bool,
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
            let value = if rate {
                format_rate(metrics.rate_for_sample(sample), "/s")
            } else {
                format_number(Some(sample.value))
            };
            Row::new([identity, value])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            rows,
            [Constraint::Percentage(65), Constraint::Percentage(35)],
        )
        .header(
            Row::new([
                labels.join(" / "),
                if rate { "Rate" } else { "Value" }.to_owned(),
            ])
            .style(table_header_style()),
        )
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
                Span::styled(value, bold_style()),
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
                    .fg(BACKGROUND)
                    .bg(ERROR)
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
            " Tab next · Shift+Tab previous · 1-7 page · ? help · q stop client{page_help} · http://{}/metrics",
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
        Line::from("1-7             select panel"),
        Line::from("Tab / Shift+Tab next / previous panel"),
        Line::from("q / Ctrl+C      stop client and dashboard"),
        Line::from(""),
        Line::from("Logs"),
        Line::from("l / L            cycle view / capture level"),
        Line::from("/                edit regex"),
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
            "›_ CLIENT FOREGROUND\n\nTerminal too small: {}x{}\nMinimum: {MIN_WIDTH}x{MIN_HEIGHT}\n\nq or Ctrl+C to stop",
            area.width, area.height,
        ))
        .alignment(Alignment::Center)
        .style(base_style())
        .block(panel("Terminal size")),
        area,
    );
}

fn log_row(event: &ForegroundLogEvent) -> Row<'static> {
    let fields = event
        .fields
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(" ");
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

fn log_style(level: ForegroundLogLevel) -> Style {
    match level {
        ForegroundLogLevel::Off | ForegroundLogLevel::Trace | ForegroundLogLevel::Debug => {
            muted_style()
        }
        ForegroundLogLevel::Info => text_style(),
        ForegroundLogLevel::Warn => Style::default().fg(WARNING).bg(BACKGROUND),
        ForegroundLogLevel::Error => Style::default()
            .fg(ERROR)
            .bg(BACKGROUND)
            .add_modifier(Modifier::BOLD),
    }
}

fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER).bg(BACKGROUND))
        .style(base_style())
        .title(format!(" {title} "))
}

fn popup_panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT).bg(SURFACE))
        .style(surface_style())
        .title(format!(" {title} "))
}

const fn base_style() -> Style {
    Style::new().fg(TEXT).bg(BACKGROUND)
}

const fn text_style() -> Style {
    Style::new().fg(TEXT).bg(BACKGROUND)
}

const fn muted_style() -> Style {
    Style::new().fg(MUTED).bg(BACKGROUND)
}

const fn surface_style() -> Style {
    Style::new().fg(TEXT).bg(SURFACE)
}

const fn muted_surface_style() -> Style {
    Style::new().fg(MUTED).bg(SURFACE)
}

const fn accent_style() -> Style {
    Style::new().fg(ACCENT).bg(BACKGROUND)
}

const fn success_style() -> Style {
    Style::new().fg(SUCCESS).bg(BACKGROUND)
}

fn bold_style() -> Style {
    text_style().add_modifier(Modifier::BOLD)
}

fn table_header_style() -> Style {
    Style::default()
        .fg(ACCENT)
        .bg(BACKGROUND)
        .add_modifier(Modifier::BOLD)
}

fn halves(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area)
}

const fn page_index(page: Page) -> usize {
    match page {
        Page::Overview => 0,
        Page::Logs => 1,
        Page::Session => 2,
        Page::Prediction => 3,
        Page::Runtime => 4,
        Page::Presentation => 5,
        Page::Host => 6,
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn scrape_status(app: &App) -> String {
    match app.metrics.scrape_age(Instant::now()) {
        Some(age) if age <= Duration::from_secs(3) => "LIVE".to_owned(),
        Some(age) => format!("STALE {}s", age.as_secs()),
        None if app.metrics.last_error.is_some() => "ERROR".to_owned(),
        None => "STARTING".to_owned(),
    }
}

fn scrape_style(app: &App) -> Style {
    match app.metrics.scrape_age(Instant::now()) {
        Some(age) if age <= Duration::from_secs(3) => success_style(),
        Some(_) => Style::default().fg(WARNING).bg(BACKGROUND),
        None if app.metrics.last_error.is_some() => Style::default().fg(ERROR).bg(BACKGROUND),
        None => muted_style(),
    }
}

fn configured(value: bool) -> String {
    if value {
        "configured".to_owned()
    } else {
        "not configured".to_owned()
    }
}

fn up_or_unavailable(value: Option<f64>) -> String {
    if value.is_some_and(|value| value > 0.5) {
        "UP".to_owned()
    } else {
        "—".to_owned()
    }
}

fn format_uptime(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60,
    )
}

fn format_number(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.0}"))
}

fn format_decimal(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2}"))
}

fn format_percent(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{:.1}%", value * 100.0))
}

fn format_millis(seconds: Option<f64>) -> String {
    seconds.map_or_else(
        || "—".to_owned(),
        |value| format!("{:.3} ms", value * 1_000.0),
    )
}

fn format_rate(value: Option<f64>, suffix: &str) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2}{suffix}"))
}

fn format_cores(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2} cores"))
}

fn format_byte_rate(value: Option<f64>) -> String {
    value.map_or_else(
        || "—".to_owned(),
        |value| format!("{}/s", human_bytes(value)),
    )
}

fn format_bytes(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), human_bytes)
}

fn human_bytes(value: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if !value.is_finite() || value < 0.0 {
        return "—".to_owned();
    }
    let mut scaled = value;
    let mut unit = 0;
    while scaled >= 1_024.0 && unit + 1 < UNITS.len() {
        scaled /= 1_024.0;
        unit += 1;
    }
    format!("{scaled:.1} {}", UNITS[unit])
}

#[cfg(test)]
#[path = "../../tests/unit/foreground_render.rs"]
mod tests;
