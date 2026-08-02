use super::*;

#[test]
fn mailbox_keeps_only_the_latest_frame() -> Result<(), MailboxError> {
    let mailbox = LatestFrameMailbox::default();
    assert_eq!(
        mailbox.publish(RenderFrame::empty(RenderFrameId::new(1)))?,
        PublishOutcome::Published
    );
    assert_eq!(
        mailbox.publish(RenderFrame::empty(RenderFrameId::new(3)))?,
        PublishOutcome::Replaced {
            dropped: RenderFrameId::new(1)
        }
    );
    assert_eq!(
        mailbox.publish(RenderFrame::empty(RenderFrameId::new(2)))?,
        PublishOutcome::IgnoredStale {
            newest: RenderFrameId::new(3)
        }
    );
    assert_eq!(
        mailbox.take_latest()?.map(|frame| frame.id),
        Some(RenderFrameId::new(3))
    );
    assert_eq!(mailbox.pending_id()?, None);
    assert_eq!(
        mailbox.publish(RenderFrame::empty(RenderFrameId::new(3)))?,
        PublishOutcome::IgnoredStale {
            newest: RenderFrameId::new(3)
        }
    );
    Ok(())
}
