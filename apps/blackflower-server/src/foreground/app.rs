use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::ForegroundConfig;
use super::logs::LogState;
use super::metrics::{MetricStore, MetricsPoller};
use super::render;

const DRAW_INTERVAL: Duration = Duration::from_millis(100);
const HISTORY_CAPACITY: usize = 60;

/// Failure while starting or running foreground diagnostics.
#[derive(Debug, thiserror::Error)]
pub enum ForegroundError {
    /// The metrics polling worker could not be started.
    #[error("failed to start foreground metrics poller")]
    MetricsPoller(#[source] io::Error),
    /// The initial log regex is invalid.
    #[error("invalid foreground log regex")]
    LogRegex(#[source] regex::Error),
    /// The terminal backend failed.
    #[error("foreground terminal failed")]
    Terminal(#[source] io::Error),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Overview,
    Logs,
    Simulation,
    Network,
    World,
    Host,
}

impl Page {
    pub(crate) const ALL: [Self; 6] = [
        Self::Overview,
        Self::Logs,
        Self::Simulation,
        Self::Network,
        Self::World,
        Self::Host,
    ];

    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Logs => "Logs",
            Self::Simulation => "Simulation",
            Self::Network => "Network",
            Self::World => "World",
            Self::Host => "Host",
        }
    }

    pub(crate) const fn short_title(self) -> &'static str {
        match self {
            Self::Overview => "Ovr",
            Self::Logs => "Logs",
            Self::Simulation => "Sim",
            Self::Network => "Net",
            Self::World => "World",
            Self::Host => "Host",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Overview => 0,
            Self::Logs => 1,
            Self::Simulation => 2,
            Self::Network => 3,
            Self::World => 4,
            Self::Host => 5,
        }
    }

    fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Default)]
pub(crate) struct Histories {
    pub(crate) tick_p95_micros: History,
    pub(crate) network_receive_bytes: History,
    pub(crate) network_transmit_bytes: History,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    values: VecDeque<u64>,
}

impl History {
    fn push(&mut self, value: u64) {
        if self.values.len() == HISTORY_CAPACITY {
            self.values.pop_front();
        }
        self.values.push_back(value);
    }

    pub(crate) fn values(&self) -> Vec<u64> {
        self.values.iter().copied().collect()
    }
}

pub(crate) struct App {
    pub(crate) service_name: &'static str,
    pub(crate) service_version: &'static str,
    pub(crate) metrics_address: std::net::SocketAddr,
    pub(crate) started: Instant,
    pub(crate) page: Page,
    pub(crate) metrics: MetricStore,
    pub(crate) logs: LogState,
    pub(crate) histories: Histories,
    pub(crate) show_help: bool,
    poller: MetricsPoller,
    shutdown_requested: Arc<AtomicBool>,
    should_quit: bool,
}

impl App {
    pub(crate) fn new(config: ForegroundConfig) -> Result<Self, ForegroundError> {
        let poller =
            MetricsPoller::start(config.metrics_address).map_err(ForegroundError::MetricsPoller)?;
        let logs = LogState::new(
            config.log_receiver,
            config.log_control,
            config.initial_view_level,
            config.initial_log_regex.as_deref(),
        )
        .map_err(ForegroundError::LogRegex)?;
        Ok(Self {
            service_name: config.service_name,
            service_version: config.service_version,
            metrics_address: config.metrics_address,
            started: Instant::now(),
            page: Page::Overview,
            metrics: MetricStore::default(),
            logs,
            histories: Histories::default(),
            show_help: false,
            poller,
            shutdown_requested: config.shutdown_requested,
            should_quit: false,
        })
    }

    pub(crate) fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.should_quit && !self.shutdown_requested.load(Ordering::Acquire) {
            self.drain_inputs();
            terminal.draw(|frame| render::draw(frame, self))?;
            if event::poll(DRAW_INTERVAL)? {
                self.handle_event(event::read()?);
            }
        }
        Ok(())
    }

    fn drain_inputs(&mut self) {
        self.logs.drain();
        while let Ok(result) = self.poller.try_recv() {
            if self.metrics.accept(result) {
                self.record_history();
            }
        }
    }

    fn record_history(&mut self) {
        if let Some(seconds) = self
            .metrics
            .histogram_quantile("blackflower_world_simulation_tick_duration_seconds", 0.95)
            && seconds.is_finite()
            && seconds >= 0.0
        {
            let micros = Duration::from_secs_f64(seconds).as_micros();
            self.histories
                .tick_p95_micros
                .push(u64::try_from(micros).unwrap_or(u64::MAX));
        }
        if let Some(rate) = self.metrics.rate("node_network_receive_bytes_total") {
            self.histories.network_receive_bytes.push(rate_to_u64(rate));
        }
        if let Some(rate) = self.metrics.rate("node_network_transmit_bytes_total") {
            self.histories
                .network_transmit_bytes
                .push(rate_to_u64(rate));
        }
    }

    #[allow(
        clippy::wildcard_enum_match_arm,
        reason = "unbound Crossterm keys and future key variants are intentionally harmless"
    )]
    fn handle_event(&mut self, event: Event) {
        let Event::Key(key) = event else {
            return;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        if self.logs.filter_editor.is_some() {
            self.handle_filter_editor(key);
            return;
        }
        if self.show_help {
            self.show_help = false;
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Tab => self.page = self.page.next(),
            KeyCode::BackTab => self.page = self.page.previous(),
            KeyCode::Char('1') => self.page = Page::Overview,
            KeyCode::Char('2') => self.page = Page::Logs,
            KeyCode::Char('3') => self.page = Page::Simulation,
            KeyCode::Char('4') => self.page = Page::Network,
            KeyCode::Char('5') => self.page = Page::World,
            KeyCode::Char('6') => self.page = Page::Host,
            _ if self.page == Page::Logs => self.handle_log_key(key),
            _ => {}
        }
    }

    #[allow(
        clippy::wildcard_enum_match_arm,
        reason = "unbound Crossterm keys and future key variants are intentionally harmless"
    )]
    fn handle_log_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('l') => self.logs.cycle_view_level(),
            KeyCode::Char('L') => self.logs.cycle_capture_level(),
            KeyCode::Char('/') => self.logs.begin_filter_edit(),
            KeyCode::Esc => self.logs.clear_filter(),
            KeyCode::Char('p') => self.logs.toggle_pause(),
            KeyCode::Char('c') => self.logs.clear_events(),
            KeyCode::Up => self.logs.scroll_up(1),
            KeyCode::Down => self.logs.scroll_down(1),
            KeyCode::PageUp => self.logs.scroll_up(10),
            KeyCode::PageDown => self.logs.scroll_down(10),
            KeyCode::End => self.logs.follow(),
            _ => {}
        }
    }

    #[allow(
        clippy::wildcard_enum_match_arm,
        reason = "filter editing accepts text keys and intentionally ignores other input"
    )]
    fn handle_filter_editor(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.logs.commit_filter(),
            KeyCode::Esc => self.logs.cancel_filter_edit(),
            KeyCode::Backspace => self.logs.edit_filter_backspace(),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.logs.edit_filter_character(character);
            }
            _ => {}
        }
    }
}

fn rate_to_u64(rate: f64) -> u64 {
    if !rate.is_finite() || rate <= 0.0 {
        return 0;
    }
    let whole = rate.floor();
    let text = format!("{whole:.0}");
    text.parse::<u64>().unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../../tests/unit/foreground_app.rs"]
mod tests;
