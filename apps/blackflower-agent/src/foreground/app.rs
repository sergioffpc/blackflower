use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use blackflower_observability_tui::{LogState, MetricStore, MetricsPoller};

use super::{AgentCapabilities, ForegroundConfig, render};
use crate::{
    AgentDiagnosticReceiver, AgentDiagnosticRecord, AgentId, AgentStatusSnapshot, DecisionRecord,
    SensoriumSnapshot,
};

const DRAW_INTERVAL: Duration = Duration::from_millis(100);
const HISTORY_CAPACITY: usize = 60;
const MAX_VISIBLE_AGENTS: usize = 32;
const DECISION_HISTORY_CAPACITY: usize = 128;

/// Failure while starting or running foreground diagnostics.
#[derive(Debug, thiserror::Error)]
pub enum ForegroundError {
    /// The metrics polling worker could not be started.
    #[error("failed to start foreground metrics poller")]
    MetricsPoller(#[source] io::Error),
    /// The terminal backend failed.
    #[error("foreground terminal failed")]
    Terminal(#[source] io::Error),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Overview,
    Logs,
    Agents,
    Sensorium,
    Decisions,
    Session,
    Prediction,
    Navigation,
    Host,
}

impl Page {
    pub(crate) const ALL: [Self; 9] = [
        Self::Overview,
        Self::Logs,
        Self::Agents,
        Self::Sensorium,
        Self::Decisions,
        Self::Session,
        Self::Prediction,
        Self::Navigation,
        Self::Host,
    ];

    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Logs => "Logs",
            Self::Agents => "Agents",
            Self::Sensorium => "Sensorium",
            Self::Decisions => "Decisions",
            Self::Session => "Session",
            Self::Prediction => "Prediction",
            Self::Navigation => "Navigation",
            Self::Host => "Host",
        }
    }

    pub(crate) const fn short_title(self) -> &'static str {
        match self {
            Self::Overview => "Ovr",
            Self::Logs => "Logs",
            Self::Agents => "Agents",
            Self::Sensorium => "Sense",
            Self::Decisions => "Dec",
            Self::Session => "Sess",
            Self::Prediction => "Pred",
            Self::Navigation => "Nav",
            Self::Host => "Host",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Overview => 0,
            Self::Logs => 1,
            Self::Agents => 2,
            Self::Sensorium => 3,
            Self::Decisions => 4,
            Self::Session => 5,
            Self::Prediction => 6,
            Self::Navigation => 7,
            Self::Host => 8,
        }
    }

    pub(crate) fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub(crate) fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Default)]
pub(crate) struct AgentView {
    pub(crate) status: Option<AgentStatusSnapshot>,
    pub(crate) sensorium: Option<SensoriumSnapshot>,
    pub(crate) decisions: VecDeque<DecisionRecord>,
}

#[derive(Debug)]
pub(crate) struct AgentDiagnosticState {
    receiver: Option<AgentDiagnosticReceiver>,
    pub(crate) configured: bool,
    pub(crate) agents: BTreeMap<AgentId, AgentView>,
    pub(crate) selected: Option<AgentId>,
    pub(crate) selected_decision_offset: usize,
    pub(crate) disconnected: bool,
}

impl AgentDiagnosticState {
    fn new(receiver: Option<AgentDiagnosticReceiver>) -> Self {
        let configured = receiver.is_some();
        Self {
            receiver,
            configured,
            agents: BTreeMap::new(),
            selected: None,
            selected_decision_offset: 0,
            disconnected: false,
        }
    }

    pub(crate) fn drain(&mut self) {
        loop {
            let result = self
                .receiver
                .as_ref()
                .map(AgentDiagnosticReceiver::try_recv);
            match result {
                Some(Ok(record)) => self.accept(record),
                Some(Err(TryRecvError::Empty)) | None => break,
                Some(Err(TryRecvError::Disconnected)) => {
                    self.receiver = None;
                    self.disconnected = true;
                    break;
                }
            }
        }
    }

    fn accept(&mut self, record: AgentDiagnosticRecord) {
        let agent_id = match &record {
            AgentDiagnosticRecord::Status(status) => status.descriptor().id(),
            AgentDiagnosticRecord::Sensorium(snapshot) => snapshot.agent_id(),
            AgentDiagnosticRecord::Decision(decision) => decision.agent_id(),
        };
        if !self.agents.contains_key(&agent_id) && self.agents.len() == MAX_VISIBLE_AGENTS {
            return;
        }
        let agent = self.agents.entry(agent_id).or_default();
        match record {
            AgentDiagnosticRecord::Status(status) => agent.status = Some(status),
            AgentDiagnosticRecord::Sensorium(snapshot) => agent.sensorium = Some(snapshot),
            AgentDiagnosticRecord::Decision(decision) => {
                if agent.decisions.len() == DECISION_HISTORY_CAPACITY {
                    agent.decisions.pop_front();
                }
                agent.decisions.push_back(decision);
            }
        }
        if self.selected.is_none() {
            self.selected = Some(agent_id);
        }
    }

    pub(crate) fn selected_view(&self) -> Option<(AgentId, &AgentView)> {
        let id = self.selected?;
        Some((id, self.agents.get(&id)?))
    }

    pub(crate) fn selected_decision(&self) -> Option<&DecisionRecord> {
        let (_id, view) = self.selected_view()?;
        let index = view
            .decisions
            .len()
            .checked_sub(self.selected_decision_offset + 1)?;
        view.decisions.get(index)
    }

    fn select_next_agent(&mut self) {
        let ids = self.agents.keys().copied().collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }
        let next = self
            .selected
            .and_then(|selected| ids.iter().position(|candidate| *candidate == selected))
            .map_or(0, |index| (index + 1) % ids.len());
        self.selected = Some(ids[next]);
        self.selected_decision_offset = 0;
    }

    fn select_previous_agent(&mut self) {
        let ids = self.agents.keys().copied().collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }
        let previous = self
            .selected
            .and_then(|selected| ids.iter().position(|candidate| *candidate == selected))
            .map_or(0, |index| (index + ids.len() - 1) % ids.len());
        self.selected = Some(ids[previous]);
        self.selected_decision_offset = 0;
    }

    fn select_older_decision(&mut self) {
        let Some((_id, view)) = self.selected_view() else {
            return;
        };
        self.selected_decision_offset = self
            .selected_decision_offset
            .saturating_add(1)
            .min(view.decisions.len().saturating_sub(1));
    }

    fn select_newer_decision(&mut self) {
        self.selected_decision_offset = self.selected_decision_offset.saturating_sub(1);
    }
}

#[derive(Debug, Default)]
pub(crate) struct Histories {
    pub(crate) prediction_p95_micros: History,
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
    pub(crate) capabilities: AgentCapabilities,
    pub(crate) started: Instant,
    pub(crate) page: Page,
    pub(crate) metrics: MetricStore,
    pub(crate) logs: LogState,
    pub(crate) histories: Histories,
    pub(crate) diagnostics: AgentDiagnosticState,
    pub(crate) show_help: bool,
    poller: MetricsPoller,
    shutdown_requested: Arc<AtomicBool>,
    should_quit: bool,
}

impl App {
    pub(crate) fn new(config: ForegroundConfig) -> Result<Self, ForegroundError> {
        let poller =
            MetricsPoller::start(config.metrics_address).map_err(ForegroundError::MetricsPoller)?;
        let logs = LogState::new(config.log_receiver, config.log_control);
        Ok(Self {
            service_name: config.service_name,
            service_version: config.service_version,
            metrics_address: config.metrics_address,
            capabilities: config.capabilities,
            started: Instant::now(),
            page: Page::Overview,
            metrics: MetricStore::default(),
            logs,
            histories: Histories::default(),
            diagnostics: AgentDiagnosticState::new(config.diagnostics),
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
        self.diagnostics.drain();
        while let Ok(result) = self.poller.try_recv() {
            if self.metrics.accept(result) {
                self.record_history();
            }
        }
    }

    fn record_history(&mut self) {
        if let Some(seconds) = self
            .metrics
            .histogram_quantile("blackflower_world_prediction_tick_duration_seconds", 0.95)
            && seconds.is_finite()
            && seconds >= 0.0
        {
            let micros = Duration::from_secs_f64(seconds).as_micros();
            self.histories
                .prediction_p95_micros
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
        clippy::too_many_lines,
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
            KeyCode::Char('3') => self.page = Page::Agents,
            KeyCode::Char('4') => self.page = Page::Sensorium,
            KeyCode::Char('5') => self.page = Page::Decisions,
            KeyCode::Char('6') => self.page = Page::Session,
            KeyCode::Char('7') => self.page = Page::Prediction,
            KeyCode::Char('8') => self.page = Page::Navigation,
            KeyCode::Char('9') => self.page = Page::Host,
            _ if self.page == Page::Logs => self.handle_log_key(key),
            KeyCode::Down
                if matches!(self.page, Page::Agents | Page::Sensorium | Page::Decisions) =>
            {
                self.diagnostics.select_next_agent();
            }
            KeyCode::Up
                if matches!(self.page, Page::Agents | Page::Sensorium | Page::Decisions) =>
            {
                self.diagnostics.select_previous_agent();
            }
            KeyCode::Left if self.page == Page::Decisions => {
                self.diagnostics.select_older_decision();
            }
            KeyCode::Right if self.page == Page::Decisions => {
                self.diagnostics.select_newer_decision();
            }
            KeyCode::End if self.page == Page::Decisions => {
                self.diagnostics.selected_decision_offset = 0;
            }
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
        reason = "unbound Crossterm keys and future key variants are intentionally harmless"
    )]
    fn handle_filter_editor(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.logs.commit_filter(),
            KeyCode::Esc => self.logs.cancel_filter_edit(),
            KeyCode::Backspace => self.logs.edit_filter_backspace(),
            KeyCode::Char(character)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
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
