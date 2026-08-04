use std::sync::mpsc;
use std::time::Duration;

use blackflower_observability::{ForegroundLogEvent, ForegroundLogLevel};

use super::LogState;

#[test]
fn filters_level_and_regex() -> Result<(), regex::Error> {
    let (sender, receiver) = mpsc::channel();
    let mut state = state_with_receiver(receiver)?;
    let info = event(ForegroundLogLevel::Info, "service ready");
    let debug = event(ForegroundLogLevel::Debug, "packet ready");
    assert!(sender.send(info).is_ok());
    assert!(sender.send(debug).is_ok());
    state.drain();
    state.begin_filter_edit();
    for character in "service".chars() {
        state.edit_filter_character(character);
    }
    state.commit_filter();

    assert_eq!(state.visible(10).0.len(), 1);
    Ok(())
}

fn state_with_receiver(
    receiver: mpsc::Receiver<ForegroundLogEvent>,
) -> Result<LogState, regex::Error> {
    LogState::new(
        receiver,
        blackflower_observability::ForegroundLogControl::new(ForegroundLogLevel::Info),
        ForegroundLogLevel::Info,
        None,
    )
}

fn event(level: ForegroundLogLevel, message: &str) -> ForegroundLogEvent {
    ForegroundLogEvent {
        elapsed: Duration::ZERO,
        level,
        target: "test".to_owned(),
        message: message.to_owned(),
        fields: Vec::new(),
    }
}
