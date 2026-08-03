use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::mpsc;

use blackflower_observability::{ForegroundLogControl, ForegroundLogLevel};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::foreground::ForegroundConfig;

use super::super::app::{App, Page};
use super::{draw, format_bytes, format_uptime};

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
    let (_log_sender, log_receiver) = mpsc::channel();
    let mut app = App::new(ForegroundConfig {
        service_name: "blackflower-server",
        service_version: "0.1.0",
        metrics_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        log_receiver,
        log_control: ForegroundLogControl::new(ForegroundLogLevel::Info),
        initial_view_level: ForegroundLogLevel::Info,
        initial_log_regex: None,
    })?;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;

    for page in Page::ALL {
        app.page = page;
        let _completed_frame = terminal.draw(|frame| draw(frame, &app))?;
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(rendered.contains(page.title()));
    }
    Ok(())
}
