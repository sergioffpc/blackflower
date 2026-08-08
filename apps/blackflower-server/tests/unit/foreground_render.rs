use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::mpsc;

use blackflower_observability::{ForegroundLogControl, ForegroundLogLevel};
use blackflower_process::ShutdownToken;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::foreground::ForegroundConfig;

use super::super::app::{App, Page};
use super::{CODEX_BACKGROUND, draw, format_bytes, format_uptime};

#[test]
fn formats_operational_values() {
    assert_eq!(format_bytes(Some(1_073_741_824.0)), "1.00 GiB");
    assert_eq!(
        format_uptime(std::time::Duration::from_secs(3_661)),
        "01:01:01"
    );
}

#[test]
fn every_panel_renders_on_a_standard_terminal() -> Result<(), Box<dyn Error>> {
    let mut app = test_app()?;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;

    for page in Page::ALL {
        app.page = page;
        let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
        let rendered = rendered_text(&terminal);
        assert!(rendered.contains(page.title()));
    }
    Ok(())
}

#[test]
fn overview_uses_codex_shell_layout_without_brand_mark() -> Result<(), Box<dyn Error>> {
    let app = test_app()?;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
    let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
    let rendered = rendered_text(&terminal);

    assert!(rendered.contains("›_ blackflower-server"));
    assert!(rendered.contains("› Process"));
    assert!(!rendered.contains("████"));
    assert_eq!(
        terminal.backend().buffer().content()[0].bg,
        CODEX_BACKGROUND
    );
    Ok(())
}

#[test]
fn simulation_panel_exposes_scheduler_health() -> Result<(), Box<dyn Error>> {
    let mut app = test_app()?;
    app.page = Page::Simulation;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
    let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
    let rendered = rendered_text(&terminal);

    for expected in ["Scheduler", "Lag p95", "Behind", "Pressure p95", "Catch-up"] {
        assert!(rendered.contains(expected), "missing {expected}");
    }
    Ok(())
}

#[test]
fn network_domains_have_dedicated_operational_panels() -> Result<(), Box<dyn Error>> {
    let mut app = test_app()?;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;

    for (page, expected) in [
        (
            Page::Transport,
            [
                "QUIC transport",
                "Application UDP",
                "Queue depth",
                "Transport drops",
            ],
        ),
        (
            Page::Sessions,
            [
                "Application sessions",
                "Input actions",
                "Resync actions",
                "Clock sessions",
            ],
        ),
        (
            Page::Replication,
            [
                "Replication summary",
                "Bootstrap p50 / p95",
                "Snapshot actions",
                "Replication queues",
            ],
        ),
    ] {
        app.page = page;
        let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
        let rendered = rendered_text(&terminal);
        for heading in expected {
            assert!(rendered.contains(heading), "missing {heading} on {page:?}");
        }
        if page == Page::Transport {
            assert!(!rendered.contains("Host throughput"));
        }
    }
    Ok(())
}

fn test_app() -> Result<App, Box<dyn Error>> {
    let (_log_sender, log_receiver) = mpsc::channel();
    Ok(App::new(ForegroundConfig {
        service_name: "blackflower-server",
        service_version: "0.1.0",
        metrics_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        log_receiver,
        log_control: ForegroundLogControl::new(ForegroundLogLevel::Info),
        shutdown_requested: ShutdownToken::new(),
    })?)
}

fn rendered_text(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect()
}
