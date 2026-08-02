use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::{Duration, Instant};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;

/// Severity used by the foreground log capture and view.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ForegroundLogLevel {
    /// Disable foreground log capture.
    Off,
    /// Capture errors only.
    Error,
    /// Capture warnings and errors.
    Warn,
    /// Capture informational records and more severe records.
    #[default]
    Info,
    /// Capture diagnostic records and more severe records.
    Debug,
    /// Capture every statically enabled record.
    Trace,
}

impl ForegroundLogLevel {
    /// Return the stable uppercase display name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }

    /// Return the next less restrictive capture level.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Error,
            Self::Error => Self::Warn,
            Self::Warn => Self::Info,
            Self::Info => Self::Debug,
            Self::Debug => Self::Trace,
            Self::Trace => Self::Off,
        }
    }

    const fn encoded(self) -> u8 {
        self as u8
    }

    fn allows(self, level: &tracing::Level) -> bool {
        let required = if level == &tracing::Level::ERROR {
            Self::Error
        } else if level == &tracing::Level::WARN {
            Self::Warn
        } else if level == &tracing::Level::INFO {
            Self::Info
        } else if level == &tracing::Level::DEBUG {
            Self::Debug
        } else {
            Self::Trace
        };
        self.encoded() >= required.encoded()
    }

    /// Return whether a view at this threshold includes an event level.
    #[must_use]
    pub fn includes(self, event_level: Self) -> bool {
        self != Self::Off && self.encoded() >= event_level.encoded()
    }

    fn from_encoded(value: u8) -> Self {
        match value {
            0 => Self::Off,
            1 => Self::Error,
            2 => Self::Warn,
            3 => Self::Info,
            4 => Self::Debug,
            _ => Self::Trace,
        }
    }

    fn from_tracing(level: &tracing::Level) -> Self {
        if level == &tracing::Level::ERROR {
            Self::Error
        } else if level == &tracing::Level::WARN {
            Self::Warn
        } else if level == &tracing::Level::INFO {
            Self::Info
        } else if level == &tracing::Level::DEBUG {
            Self::Debug
        } else {
            Self::Trace
        }
    }
}

/// One structured tracing event captured for the foreground UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundLogEvent {
    /// Monotonic elapsed time since foreground capture started.
    pub elapsed: Duration,
    /// Event severity.
    pub level: ForegroundLogLevel,
    /// Stable tracing target.
    pub target: String,
    /// Human-readable event message.
    pub message: String,
    /// Remaining structured fields in recording order.
    pub fields: Vec<(String, String)>,
}

impl ForegroundLogEvent {
    /// Build the text searched by foreground regex filters.
    #[must_use]
    pub fn searchable_text(&self) -> String {
        let mut text = String::with_capacity(
            self.target.len() + self.message.len() + self.fields.len().saturating_mul(16),
        );
        text.push_str(&self.target);
        text.push(' ');
        text.push_str(&self.message);
        for (name, value) in &self.fields {
            text.push(' ');
            text.push_str(name);
            text.push('=');
            text.push_str(value);
        }
        text
    }
}

/// Thread-safe controls and health for foreground log capture.
#[derive(Debug, Clone)]
pub struct ForegroundLogControl {
    level: Arc<AtomicU8>,
    dropped: Arc<AtomicU64>,
}

impl ForegroundLogControl {
    /// Create a detached control, primarily for foreground state composition.
    #[must_use]
    pub fn new(level: ForegroundLogLevel) -> Self {
        Self {
            level: Arc::new(AtomicU8::new(level.encoded())),
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Change the capture threshold without rebuilding the tracing subscriber.
    pub fn set_level(&self, level: ForegroundLogLevel) {
        self.level.store(level.encoded(), Ordering::Relaxed);
    }

    /// Return the active capture threshold.
    #[must_use]
    pub fn level(&self) -> ForegroundLogLevel {
        ForegroundLogLevel::from_encoded(self.level.load(Ordering::Relaxed))
    }

    /// Return the number of records discarded because the queue was full.
    #[must_use]
    pub fn dropped_events(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

pub(crate) struct ForegroundLogs {
    pub(crate) receiver: Receiver<ForegroundLogEvent>,
    pub(crate) control: ForegroundLogControl,
}

pub(crate) struct ForegroundLogLayer {
    sender: SyncSender<ForegroundLogEvent>,
    control: ForegroundLogControl,
    started: Instant,
}

pub(crate) fn channel(
    capacity: usize,
    level: ForegroundLogLevel,
) -> (ForegroundLogLayer, ForegroundLogs) {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let control = ForegroundLogControl::new(level);
    let layer = ForegroundLogLayer {
        sender,
        control: control.clone(),
        started: Instant::now(),
    };
    (layer, ForegroundLogs { receiver, control })
}

impl<S> Layer<S> for ForegroundLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _context: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();
        let capture_level = self.control.level();
        if !capture_level.allows(metadata.level()) {
            return;
        }

        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);
        let record = ForegroundLogEvent {
            elapsed: self.started.elapsed(),
            level: ForegroundLogLevel::from_tracing(metadata.level()),
            target: metadata.target().to_owned(),
            message: visitor.message.unwrap_or_default(),
            fields: visitor.fields,
        };
        if let Err(error) = self.sender.try_send(record)
            && matches!(error, TrySendError::Full(_))
        {
            self.control.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
struct EventVisitor {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl EventVisitor {
    fn record_value(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.push((field.name().to_owned(), value));
        }
    }
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_value(field, format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field, value.to_owned());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_value(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_value(field, value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_value(field, value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record_value(field, value.to_string());
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.record_value(field, value.to_string());
    }
}

#[cfg(test)]
#[path = "../tests/unit/foreground_logs.rs"]
mod tests;
