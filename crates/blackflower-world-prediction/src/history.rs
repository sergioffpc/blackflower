use std::collections::VecDeque;
use std::num::NonZeroUsize;

use crate::{InputSequence, PredictionTick};

/// Network v1 prediction and input history length.
pub const NETWORK_HISTORY_TICKS: usize = 512;

/// A sealed predicted state recorded at one tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredictionFrame<S> {
    tick: PredictionTick,
    state: S,
}

impl<S> PredictionFrame<S> {
    /// Return the recorded tick.
    #[must_use]
    pub const fn tick(&self) -> PredictionTick {
        self.tick
    }

    /// Return the recorded predicted state.
    #[must_use]
    pub const fn state(&self) -> &S {
        &self.state
    }
}

/// The input selected for one predicted simulation tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFrame<I> {
    tick: PredictionTick,
    sequence: InputSequence,
    input: I,
}

impl<I> InputFrame<I> {
    /// Construct an input record for one predicted tick.
    #[must_use]
    pub const fn new(tick: PredictionTick, sequence: InputSequence, input: I) -> Self {
        Self {
            tick,
            sequence,
            input,
        }
    }

    /// Return the predicted tick that consumed this input.
    #[must_use]
    pub const fn tick(&self) -> PredictionTick {
        self.tick
    }

    /// Return the originating control-frame sequence.
    #[must_use]
    pub const fn sequence(&self) -> InputSequence {
        self.sequence
    }

    /// Return the recorded input value.
    #[must_use]
    pub const fn input(&self) -> &I {
        &self.input
    }
}

/// Invalid ordering while recording bounded prediction history.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HistoryError {
    /// Prediction ticks must be recorded in strict ascending order.
    #[error("tick {next} must follow latest recorded tick {latest}")]
    TickNotAfterLatest {
        /// Latest tick already present.
        latest: PredictionTick,
        /// Tick rejected by the history.
        next: PredictionTick,
    },
    /// Input sequences may repeat while held, but may not move backwards.
    #[error("input sequence {next} precedes latest recorded sequence {latest}")]
    InputSequenceRegressed {
        /// Latest input sequence already present.
        latest: InputSequence,
        /// Input sequence rejected by the history.
        next: InputSequence,
    },
    /// A requested baseline was no longer present.
    #[error("prediction state for tick {0} is not present")]
    MissingPredictionTick(PredictionTick),
}

/// Bounded chronological history of sealed predicted states.
#[derive(Debug)]
pub struct PredictionHistory<S> {
    capacity: NonZeroUsize,
    frames: VecDeque<PredictionFrame<S>>,
}

impl<S> PredictionHistory<S> {
    /// Create the normative 512-tick network history.
    #[must_use]
    pub fn network_v1() -> Self {
        Self::new(NonZeroUsize::new(NETWORK_HISTORY_TICKS).unwrap_or(NonZeroUsize::MIN))
    }

    /// Create an empty history retaining at most `capacity` states.
    #[must_use]
    pub const fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            frames: VecDeque::new(),
        }
    }

    /// Return the configured maximum number of states.
    #[must_use]
    pub const fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Return the number of currently retained states.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Test whether no states are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Append a newer sealed prediction state.
    pub fn record(&mut self, tick: PredictionTick, state: S) -> Result<(), HistoryError> {
        if let Some(latest) = self.frames.back()
            && tick <= latest.tick
        {
            return Err(HistoryError::TickNotAfterLatest {
                latest: latest.tick,
                next: tick,
            });
        }
        self.frames.push_back(PredictionFrame { tick, state });
        self.enforce_capacity();
        Ok(())
    }

    /// Return the retained state for exactly `tick`.
    #[must_use]
    pub fn get(&self, tick: PredictionTick) -> Option<&PredictionFrame<S>> {
        self.frames.iter().find(|frame| frame.tick == tick)
    }

    /// Return the oldest retained state.
    #[must_use]
    pub fn oldest(&self) -> Option<&PredictionFrame<S>> {
        self.frames.front()
    }

    /// Return the newest retained state.
    #[must_use]
    pub fn newest(&self) -> Option<&PredictionFrame<S>> {
        self.frames.back()
    }

    pub(crate) fn discard_before(&mut self, tick: PredictionTick) {
        while self.frames.front().is_some_and(|frame| frame.tick < tick) {
            drop(self.frames.pop_front());
        }
    }

    pub(crate) fn truncate_after(&mut self, tick: PredictionTick) {
        while self.frames.back().is_some_and(|frame| frame.tick > tick) {
            drop(self.frames.pop_back());
        }
    }

    pub(crate) fn replace(&mut self, tick: PredictionTick, state: S) -> Result<(), HistoryError> {
        let Some(frame) = self.frames.iter_mut().find(|frame| frame.tick == tick) else {
            return Err(HistoryError::MissingPredictionTick(tick));
        };
        frame.state = state;
        Ok(())
    }

    fn enforce_capacity(&mut self) {
        while self.frames.len() > self.capacity.get() {
            drop(self.frames.pop_front());
        }
    }
}

/// Bounded chronological history of the input consumed by every predicted tick.
#[derive(Debug)]
pub struct InputHistory<I> {
    capacity: NonZeroUsize,
    frames: VecDeque<InputFrame<I>>,
}

impl<I> InputHistory<I> {
    /// Create the normative 512-tick network input history.
    #[must_use]
    pub fn network_v1() -> Self {
        Self::new(NonZeroUsize::new(NETWORK_HISTORY_TICKS).unwrap_or(NonZeroUsize::MIN))
    }

    /// Create an empty history retaining at most `capacity` tick inputs.
    #[must_use]
    pub const fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            frames: VecDeque::new(),
        }
    }

    /// Return the configured maximum number of tick inputs.
    #[must_use]
    pub const fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Return the number of currently retained tick inputs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Test whether no tick inputs are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Append the input selected for a newer prediction tick.
    pub fn record(&mut self, frame: InputFrame<I>) -> Result<(), HistoryError> {
        if let Some(latest) = self.frames.back() {
            if frame.tick <= latest.tick {
                return Err(HistoryError::TickNotAfterLatest {
                    latest: latest.tick,
                    next: frame.tick,
                });
            }
            if frame.sequence < latest.sequence {
                return Err(HistoryError::InputSequenceRegressed {
                    latest: latest.sequence,
                    next: frame.sequence,
                });
            }
        }
        self.frames.push_back(frame);
        self.enforce_capacity();
        Ok(())
    }

    /// Return the input recorded for exactly `tick`.
    #[must_use]
    pub fn get(&self, tick: PredictionTick) -> Option<&InputFrame<I>> {
        self.frames.iter().find(|frame| frame.tick == tick)
    }

    /// Return the oldest retained tick input.
    #[must_use]
    pub fn oldest(&self) -> Option<&InputFrame<I>> {
        self.frames.front()
    }

    /// Return the newest retained tick input.
    #[must_use]
    pub fn newest(&self) -> Option<&InputFrame<I>> {
        self.frames.back()
    }

    pub(crate) fn after(&self, tick: PredictionTick) -> impl Iterator<Item = &InputFrame<I>> {
        self.frames.iter().filter(move |frame| frame.tick > tick)
    }

    pub(crate) fn discard_through(&mut self, tick: PredictionTick) {
        while self.frames.front().is_some_and(|frame| frame.tick <= tick) {
            drop(self.frames.pop_front());
        }
    }

    fn enforce_capacity(&mut self) {
        while self.frames.len() > self.capacity.get() {
            drop(self.frames.pop_front());
        }
    }
}
