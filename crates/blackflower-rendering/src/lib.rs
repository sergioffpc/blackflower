//! Backend-facing rendering contracts.
//!
//! Presentation publishes complete immutable [`RenderFrame`] snapshots through
//! a bounded latest-frame mailbox. The renderer owns upload, residency, GPU
//! culling, command encoding, swapchain presentation, and resource retirement.

use std::sync::{Arc, Mutex};

/// Monotonic identity of one presentation-produced render frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderFrameId(u64);

impl RenderFrameId {
    /// Construct a frame identity.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the numeric identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Persistent logical resource identity supplied by presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceHandle(u64);

impl ResourceHandle {
    /// Construct a non-backend resource identity.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the numeric identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Renderer-owned lifecycle of a persistent resource handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceState {
    /// Presentation referenced the resource but no upload has started.
    Requested,
    /// Backend upload or compilation is in progress.
    Uploading,
    /// The resource is safe to use in newly encoded GPU work.
    Resident,
    /// Loading or upload failed and the renderer must use a fallback.
    Failed,
}

/// One immutable render view.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderView {
    /// Stable view identity within the client runtime.
    pub id: u64,
    /// Final world-to-view matrix in column-major order.
    pub view: [f32; 16],
    /// Final projection matrix in column-major order.
    pub projection: [f32; 16],
    /// Output rectangle as x, y, width, and height in pixels.
    pub viewport: [u32; 4],
    /// Semantic layer mask. GPU culling remains renderer-owned.
    pub layer_mask: u64,
}

/// One immutable visual instance referencing persistent logical resources.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderInstance {
    /// Stable presentation proxy identity.
    pub id: u64,
    /// Persistent model or geometry handle.
    pub resource: ResourceHandle,
    /// Final object-to-world matrix in column-major order.
    pub transform: [f32; 16],
    /// Semantic layer membership.
    pub layer_mask: u64,
}

/// Complete immutable visual snapshot consumed by the renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderFrame {
    /// Monotonic frame identity used for idempotent publication.
    pub id: RenderFrameId,
    /// Fully resolved views.
    pub views: Vec<RenderView>,
    /// Fully resolved scene instances.
    pub instances: Vec<RenderInstance>,
}

impl RenderFrame {
    /// Construct an empty frame ready for presentation-owned population.
    #[must_use]
    pub const fn empty(id: RenderFrameId) -> Self {
        Self {
            id,
            views: Vec::new(),
            instances: Vec::new(),
        }
    }
}

/// Result of publishing one frame into a latest-wins mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// The mailbox was empty.
    Published,
    /// An older unconsumed frame was replaced.
    Replaced { dropped: RenderFrameId },
    /// The same or a newer frame was already published.
    IgnoredStale { newest: RenderFrameId },
}

/// Failure while accessing the render-frame mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MailboxError {
    /// A previous panic poisoned the mailbox lock.
    #[error("render-frame mailbox is unavailable")]
    Unavailable,
}

/// Single-slot, latest-wins handoff from presentation to a renderer thread.
#[derive(Debug, Default)]
pub struct LatestFrameMailbox {
    state: Mutex<MailboxState>,
}

#[derive(Debug, Default)]
struct MailboxState {
    pending: Option<Arc<RenderFrame>>,
    newest_published: Option<RenderFrameId>,
}

impl LatestFrameMailbox {
    /// Publish a complete frame without blocking on rendering or presentation.
    pub fn publish(&self, frame: RenderFrame) -> Result<PublishOutcome, MailboxError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_error| MailboxError::Unavailable)?;
        if let Some(newest) = state.newest_published
            && newest >= frame.id
        {
            return Ok(PublishOutcome::IgnoredStale { newest });
        }
        let outcome = match state.pending.as_ref() {
            Some(current) if current.id >= frame.id => {
                return Ok(PublishOutcome::IgnoredStale { newest: current.id });
            }
            Some(current) => PublishOutcome::Replaced {
                dropped: current.id,
            },
            None => PublishOutcome::Published,
        };
        state.newest_published = Some(frame.id);
        state.pending = Some(Arc::new(frame));
        Ok(outcome)
    }

    /// Take the latest pending frame, leaving the mailbox empty.
    pub fn take_latest(&self) -> Result<Option<Arc<RenderFrame>>, MailboxError> {
        self.state
            .lock()
            .map(|mut state| state.pending.take())
            .map_err(|_error| MailboxError::Unavailable)
    }

    /// Inspect the pending frame identity without consuming it.
    pub fn pending_id(&self) -> Result<Option<RenderFrameId>, MailboxError> {
        self.state
            .lock()
            .map(|state| state.pending.as_ref().map(|frame| frame.id))
            .map_err(|_error| MailboxError::Unavailable)
    }
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
