use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use blackflower_observability::{ForegroundLogControl, ForegroundLogLevel};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use blackflower_networking::{SessionState, SimulationTick};

use crate::foreground::{AgentCapabilities, ForegroundConfig};
use crate::{
    AgentDescriptor, AgentDiagnosticConfig, AgentDiagnosticError, AgentDiagnosticReceiver,
    AgentDiagnostics, AgentId, DecisionCandidate, DecisionConstraint, DecisionOutcome,
    DecisionRecord, DiagnosticText, MemoryItemSnapshot, MemoryKind, MemoryStatus, PolicySource,
    SensoriumAvailability, SensoriumChannelKind, SensoriumChannelSnapshot, SensoriumSnapshot,
    agent_diagnostic_channel,
};

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
fn overview_identifies_the_unconfigured_runtime_boundary() -> Result<(), Box<dyn Error>> {
    let app = test_app()?;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
    let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
    let rendered = rendered_text(&terminal);

    assert!(rendered.contains("›_ blackflower-agent"));
    assert!(rendered.contains("Agent runtime"));
    assert!(rendered.contains("Diagnostic drops"));
    assert!(rendered.contains("not configured"));
    assert_eq!(
        terminal.backend().buffer().content()[0].bg,
        CODEX_BACKGROUND
    );
    Ok(())
}

#[test]
fn agent_panels_render_only_records_from_the_runtime_stream() -> Result<(), Box<dyn Error>> {
    let capacity = NonZeroUsize::new(8).ok_or_else(|| std::io::Error::other("zero capacity"))?;
    let (sender, receiver) = agent_diagnostic_channel(capacity);
    let agent_id = AgentId::new(
        NonZeroU32::new(7).ok_or_else(|| std::io::Error::other("zero agent identity"))?,
    );
    let descriptor = AgentDescriptor::new(agent_id, text("standard")?, text("classical-v1")?);
    let mut producer = AgentDiagnostics::connected(
        Some(AgentDiagnosticConfig::new(descriptor, sender)),
        SessionState::Active,
    );
    producer.record_sensorium(sensorium(agent_id)?)?;
    producer.record_decision(decision(agent_id)?)?;
    let mut app = test_app_with_diagnostics(Some(receiver))?;
    app.diagnostics.drain();
    let mut terminal = Terminal::new(TestBackend::new(160, 44))?;

    for (page, expected) in [
        (Page::Agents, "standard"),
        (Page::Sensorium, "two visible silhouettes"),
        (Page::Decisions, "move left"),
    ] {
        app.page = page;
        let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
        assert!(rendered_text(&terminal).contains(expected));
    }
    drop(producer);
    Ok(())
}

fn test_app() -> Result<App, Box<dyn Error>> {
    test_app_with_diagnostics(None)
}

fn test_app_with_diagnostics(
    diagnostics: Option<AgentDiagnosticReceiver>,
) -> Result<App, Box<dyn Error>> {
    let (_log_sender, log_receiver) = mpsc::channel();
    Ok(App::new(ForegroundConfig {
        service_name: "blackflower-agent",
        service_version: "0.1.0",
        metrics_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        log_receiver,
        log_control: ForegroundLogControl::new(ForegroundLogLevel::Info),
        capabilities: AgentCapabilities {
            runtime_configured: false,
            policy_configured: false,
            navigation_loaded: false,
            recastnavigation_version: (1, 6, 0),
            detour_navmesh_version: 7,
        },
        diagnostics,
        shutdown_requested: Arc::new(AtomicBool::new(false)),
    })?)
}

fn sensorium(agent_id: AgentId) -> Result<SensoriumSnapshot, AgentDiagnosticError> {
    SensoriumSnapshot::new(
        agent_id,
        1,
        SimulationTick::new(40),
        1,
        text("classical-v1")?,
        2,
        vec![SensoriumChannelSnapshot::new(
            SensoriumChannelKind::Vision,
            SensoriumAvailability::Admitted,
            text("two visible silhouettes")?,
            true,
        )],
        vec![MemoryItemSnapshot::new(
            11,
            MemoryKind::Spatial,
            MemoryStatus::Observed,
            text("cover edge ahead")?,
            0.8,
            0.2,
            std::time::Duration::from_millis(20),
            true,
        )?],
    )
}

fn decision(agent_id: AgentId) -> Result<DecisionRecord, AgentDiagnosticError> {
    DecisionRecord::new(
        agent_id,
        1,
        1,
        SimulationTick::new(40),
        text("take cover")?,
        PolicySource::Classical,
        DecisionOutcome::Completed,
        text("move left")?,
        text("input accepted")?,
        vec![DecisionCandidate::new(
            text("move left")?,
            0.75,
            text("selected")?,
        )?],
        vec![DecisionConstraint::new(
            text("reaction gate")?,
            text("unchanged")?,
        )],
        std::time::Duration::from_micros(80),
        None,
        false,
        None,
    )
}

fn text(value: &str) -> Result<DiagnosticText, AgentDiagnosticError> {
    DiagnosticText::new(value)
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
