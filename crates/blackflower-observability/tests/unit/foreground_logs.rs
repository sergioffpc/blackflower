use tracing_subscriber::layer::SubscriberExt as _;

use super::{ForegroundLogLevel, channel};

#[test]
fn capture_level_changes_without_rebuilding_subscriber() -> Result<(), std::sync::mpsc::TryRecvError>
{
    let (layer, logs) = channel(4, ForegroundLogLevel::Warn);
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "test", answer = 42, "hidden");
        tracing::warn!(target: "test", answer = 42, "visible");
    });

    let event = logs.receiver.try_recv()?;
    assert_eq!(event.level, ForegroundLogLevel::Warn);
    assert_eq!(event.target, "test");
    assert_eq!(event.message, "visible");
    assert_eq!(event.fields, vec![("answer".to_owned(), "42".to_owned())]);
    assert!(logs.receiver.try_recv().is_err());

    logs.control.set_level(ForegroundLogLevel::Trace);
    assert_eq!(logs.control.level(), ForegroundLogLevel::Trace);
    Ok(())
}

#[test]
fn full_queue_is_lossy_and_counted() {
    let (layer, logs) = channel(1, ForegroundLogLevel::Info);
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("first");
        tracing::info!("second");
    });

    assert_eq!(logs.control.dropped_events(), 1);
}
