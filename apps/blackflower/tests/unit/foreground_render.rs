use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use blackflower_observability::{ForegroundLogControl, ForegroundLogLevel};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::foreground::{ClientCapabilities, ForegroundConfig};

use super::super::app::{App, Page};
use super::{BACKGROUND, draw, format_bytes, format_uptime};

#[test]
fn formats_operational_values() {
    assert_eq!(format_bytes(Some(1_073_741_824.0)), "1.0 GiB");
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
        assert!(rendered_text(&terminal).contains(page.title()));
    }
    Ok(())
}

#[test]
fn overview_identifies_native_client_boundaries() -> Result<(), Box<dyn Error>> {
    let app = test_app()?;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
    let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
    let rendered = rendered_text(&terminal);

    assert!(rendered.contains("›_ blackflower"));
    assert!(rendered.contains("native + terminal"));
    assert!(rendered.contains("Renderer backend"));
    assert!(rendered.contains("not configured"));
    assert_eq!(terminal.backend().buffer().content()[0].bg, BACKGROUND);
    Ok(())
}

#[test]
fn prediction_panel_reports_the_snapshot_only_contract() -> Result<(), Box<dyn Error>> {
    let mut app = test_app()?;
    app.page = Page::Prediction;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
    let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
    let rendered = rendered_text(&terminal);

    for expected in [
        "Authoritative snapshot state",
        "SNAPSHOT ONLY",
        "PredictionWorld",
        "not instantiated",
        "schema v1 decoded",
        "Operational boundary",
    ] {
        assert!(rendered.contains(expected), "missing {expected}");
    }
    assert!(!rendered.contains("Prediction tick p95"));
    assert!(!rendered.contains("Reconciliation outcomes"));
    Ok(())
}

#[test]
fn runtime_world_panel_exposes_live_ecs_signals() -> Result<(), Box<dyn Error>> {
    let mut app = test_app()?;
    app.page = Page::Runtime;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
    let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
    let rendered = rendered_text(&terminal);

    for expected in [
        "Client runtime / world",
        "Active worlds",
        "ECS health",
        "Tick internals p95",
        "ECS tick executions",
    ] {
        assert!(rendered.contains(expected), "missing {expected}");
    }
    Ok(())
}

fn test_app() -> Result<App, Box<dyn Error>> {
    let (_log_sender, log_receiver) = mpsc::channel();
    Ok(App::new(ForegroundConfig {
        service_name: "blackflower",
        service_version: "0.1.0",
        metrics_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        log_receiver,
        log_control: ForegroundLogControl::new(ForegroundLogLevel::Info),
        capabilities: ClientCapabilities::connected(),
        shutdown_requested: Arc::new(AtomicBool::new(false)),
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
