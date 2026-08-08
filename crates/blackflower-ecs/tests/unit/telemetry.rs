use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

use crate::{TickDelta, World};

#[derive(Clone)]
struct CountingSubscriber {
    events: Arc<AtomicUsize>,
    spans: Arc<AtomicUsize>,
}

impl Subscriber for CountingSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _attributes: &Attributes<'_>) -> Id {
        let id = self.spans.fetch_add(1, Ordering::Relaxed) + 1;
        Id::from_u64(u64::try_from(id).unwrap_or(u64::MAX))
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, _event: &Event<'_>) {
        self.events.fetch_add(1, Ordering::Relaxed);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

#[test]
fn tracing_feature_emits_lifecycle_and_tick_signals() -> Result<(), Box<dyn std::error::Error>> {
    let events = Arc::new(AtomicUsize::new(0));
    let spans = Arc::new(AtomicUsize::new(0));
    let subscriber = CountingSubscriber {
        events: Arc::clone(&events),
        spans: Arc::clone(&spans),
    };

    tracing::subscriber::with_default(subscriber, || -> Result<(), Box<dyn std::error::Error>> {
        let mut world = World::new()?;
        let _should_continue = world.progress(TickDelta::from_seconds(1.0 / 60.0)?)?;
        Ok(())
    })?;

    assert!(events.load(Ordering::Relaxed) >= 2);
    assert_eq!(spans.load(Ordering::Relaxed), 1);
    Ok(())
}
