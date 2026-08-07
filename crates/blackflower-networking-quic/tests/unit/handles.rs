use std::error::Error as StdError;

use blackflower_networking::VoiceStreamId;

use super::{
    HOST_EVENT_CAPACITY, NetworkEvent, SharedVoiceQueue, control_channel, event_channel, publish,
    try_receive,
};

type TestResult = Result<(), Box<dyn StdError>>;

#[test]
fn host_event_depth_tracks_successful_publication_and_receive() -> TestResult {
    let (sender, receiver) = event_channel();

    assert_eq!(receiver.depth(), 0);
    assert!(publish(&sender, NetworkEvent::TransportStopped));
    assert_eq!(receiver.depth(), 1);
    assert_eq!(
        try_receive(&receiver)?,
        Some(NetworkEvent::TransportStopped)
    );
    assert_eq!(receiver.depth(), 0);
    Ok(())
}

#[test]
fn rejected_host_event_does_not_inflate_queue_depth() {
    let (sender, receiver) = event_channel();
    for _index in 0..HOST_EVENT_CAPACITY {
        assert!(publish(&sender, NetworkEvent::TransportStopped));
    }

    assert!(!publish(&sender, NetworkEvent::TransportStopped));
    assert_eq!(receiver.depth(), HOST_EVENT_CAPACITY);
}

#[test]
fn control_and_voice_depths_follow_the_actual_bounded_queues() -> TestResult {
    let (control, mut control_receiver) = control_channel();
    assert_eq!(control.depth(), 0);
    control.try_send(vec![1])?;
    assert_eq!(control.depth(), 1);
    let _frame = control_receiver.try_recv()?;
    assert_eq!(control.depth(), 0);

    let voice = SharedVoiceQueue::default();
    let stream = VoiceStreamId(7);
    for packet in 0..4 {
        voice.push(stream, vec![packet])?;
    }
    assert_eq!(voice.depth(), 3);
    assert_eq!(voice.pop()?, Some(vec![1]));
    assert_eq!(voice.depth(), 2);
    Ok(())
}
