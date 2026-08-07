use std::error::Error as StdError;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use blackflower_ecs::TickDelta;
use blackflower_harness::{
    ClientEvent, ClientHarness, ClientPrediction, ClientTransport, ClientView,
};
use blackflower_networking::SimulationTick;
use blackflower_world_presentation::{FrameIndex, PresentationWorld};

use crate::input::InputSnapshot;

const TARGET_FRAME_SECONDS: f32 = 1.0 / 60.0;
const MINIMUM_FRAME_SECONDS: f32 = 1.0 / 1_000_000.0;
const MAXIMUM_FRAME_SECONDS: f32 = 0.25;

#[derive(Debug, Default)]
pub(crate) struct FrameClock {
    previous: Option<Instant>,
}

impl FrameClock {
    pub(crate) fn resume(&mut self, now: Instant) {
        self.previous = Some(now);
    }

    pub(crate) fn suspend(&mut self) {
        self.previous = None;
    }

    pub(crate) fn delta(&mut self, now: Instant) -> Result<TickDelta> {
        let seconds = self
            .previous
            .replace(now)
            .map_or(TARGET_FRAME_SECONDS, |previous| {
                now.duration_since(previous)
                    .as_secs_f32()
                    .clamp(MINIMUM_FRAME_SECONDS, MAXIMUM_FRAME_SECONDS)
            });
        TickDelta::from_seconds(seconds).context("presentation frame delta is invalid")
    }

    pub(crate) fn next_deadline(now: Instant) -> Instant {
        now + Duration::from_secs_f32(TARGET_FRAME_SECONDS)
    }
}

/// Gameplay-owned conversion from an immutable harness view into client-only state.
pub trait PresentationBridge<S> {
    /// Concrete capture or proxy-update failure.
    type Error: StdError + Send + Sync + 'static;

    /// Consume borrowed harness state without retaining or duplicating its history.
    fn capture(
        &mut self,
        presentation: &mut PresentationWorld,
        view: ClientView<'_, S>,
        events: &[ClientEvent],
    ) -> Result<(), Self::Error>;
}

/// Client composition that advances the shared harness before presentation.
pub struct HarnessPresentationRuntime<T, P, B>
where
    T: ClientTransport,
    P: ClientPrediction,
    B: PresentationBridge<P::State>,
{
    harness: ClientHarness<T, P>,
    presentation: PresentationWorld,
    bridge: B,
}

impl<T, P, B> HarnessPresentationRuntime<T, P, B>
where
    T: ClientTransport,
    P: ClientPrediction,
    B: PresentationBridge<P::State>,
{
    /// Compose an established harness and its gameplay-owned presentation bridge.
    pub fn new(harness: ClientHarness<T, P>, bridge: B) -> Result<Self> {
        let presentation =
            PresentationWorld::new().context("presentation world initialization failed")?;
        Ok(Self {
            harness,
            presentation,
            bridge,
        })
    }

    /// Drain harness work, capture its immutable view, then advance presentation.
    pub fn frame(&mut self, now: Duration, delta: TickDelta) -> Result<bool> {
        let snapshot_tick = latest_authoritative_tick(self.harness.view());
        let clock_tick = self
            .harness
            .estimated_server_tick(now)
            .context("server clock mapping failed")?;
        let authoritative_tick = snapshot_tick.max(clock_tick);
        self.harness
            .update(now, authoritative_tick)
            .context("client harness update failed")?;
        if self.harness.view().session_state() == blackflower_networking::SessionState::Active {
            self.harness
                .advance_prediction_to(authoritative_tick)
                .context("client prediction advance failed")?;
        }
        let events = drain_client_events(&mut self.harness);
        self.bridge
            .capture(&mut self.presentation, self.harness.view(), &events)
            .context("client view capture failed")?;
        self.presentation
            .frame(delta)
            .context("presentation frame failed")
    }

    /// Return the shared client harness.
    #[must_use]
    pub const fn harness(&self) -> &ClientHarness<T, P> {
        &self.harness
    }

    /// Return the shared client harness for input or connection coordination.
    #[must_use]
    pub const fn harness_mut(&mut self) -> &mut ClientHarness<T, P> {
        &mut self.harness
    }

    /// Return the dedicated client presentation world.
    #[must_use]
    pub const fn presentation(&self) -> &PresentationWorld {
        &self.presentation
    }

    /// Return the dedicated client presentation world for gameplay registration.
    #[must_use]
    pub const fn presentation_mut(&mut self) -> &mut PresentationWorld {
        &mut self.presentation
    }

    /// Return the gameplay-owned harness-to-presentation bridge.
    #[must_use]
    pub const fn bridge(&self) -> &B {
        &self.bridge
    }

    /// Return the gameplay-owned bridge for client-only registration or state.
    #[must_use]
    pub const fn bridge_mut(&mut self) -> &mut B {
        &mut self.bridge
    }
}

pub(crate) trait ApplicationRuntime {
    fn frame(&mut self, now: Duration, delta: TickDelta, input: &InputSnapshot) -> Result<bool>;

    fn current_frame(&self) -> FrameIndex;
}

impl<T, P, B> ApplicationRuntime for HarnessPresentationRuntime<T, P, B>
where
    T: ClientTransport + 'static,
    P: ClientPrediction + 'static,
    B: PresentationBridge<P::State> + 'static,
{
    fn frame(&mut self, now: Duration, delta: TickDelta, _input: &InputSnapshot) -> Result<bool> {
        Self::frame(self, now, delta)
    }

    fn current_frame(&self) -> FrameIndex {
        self.presentation.current_frame()
    }
}

fn latest_authoritative_tick<S>(view: ClientView<'_, S>) -> SimulationTick {
    view.authoritative()
        .map_or(SimulationTick::new(0), |snapshot| {
            SimulationTick::new(snapshot.tick().get())
        })
}

fn drain_client_events<T, P>(harness: &mut ClientHarness<T, P>) -> Vec<ClientEvent>
where
    T: ClientTransport,
    P: ClientPrediction,
{
    let mut events = Vec::new();
    while let Some(event) = harness.pop_event() {
        events.push(event);
    }
    events
}

#[cfg(test)]
#[path = "../tests/unit/runtime.rs"]
mod tests;
