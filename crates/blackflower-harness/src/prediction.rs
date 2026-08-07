use std::collections::BTreeMap;
use std::error::Error as StdError;

use blackflower_networking::{ControlFrame, INPUT_GRACE_TICKS, SimulationTick};
use blackflower_networking_replication::{Snapshot, SnapshotTick};
use blackflower_world_prediction::{
    AuthoritativeSnapshot, HardResyncReason, HistoryError, InputFrame, InputHistory, InputSequence,
    NETWORK_HISTORY_TICKS, PredictionDriver, PredictionHistory, PredictionPass,
    PredictionStateComparison, PredictionTick, ReconciliationCoordinator, ReconciliationError,
    ReconciliationOutcome,
};

/// Source-neutral result of applying one authoritative projection to prediction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredictionUpdate {
    /// A full snapshot established a new prediction timeline.
    Bootstrapped { tick: SimulationTick },
    /// Local prediction satisfied the authoritative comparison policy.
    Converged { tick: SimulationTick },
    /// Prediction was restored and replayed to its previous local tick.
    Reconciled {
        /// Authoritative baseline tick.
        authoritative_tick: SimulationTick,
        /// Number of local ticks replayed.
        resimulated_ticks: u64,
    },
    /// Retained state cannot safely reconcile and a full snapshot is required.
    HardResyncRequired { reason: HardResyncReason },
}

/// Gameplay-owned decoding between canonical replication/control bytes and prediction values.
pub trait PredictionCodec<S, I> {
    /// Concrete gameplay codec failure.
    type Error: StdError + Send + Sync + 'static;

    /// Decode the prediction subset and acknowledged local input from one projection.
    fn decode_snapshot(
        &mut self,
        snapshot: &Snapshot,
    ) -> Result<AuthoritativeSnapshot<S>, Self::Error>;

    /// Decode one canonical control frame for predicted simulation.
    fn decode_input(&mut self, frame: &ControlFrame) -> Result<I, Self::Error>;

    /// Build neutral input after the network grace interval expires.
    fn neutral_input(&self) -> I;

    /// Compare the simulation-defined prediction subset.
    ///
    /// Discrete fields compare exactly. Continuous fields use explicit,
    /// domain-specific tolerances or their canonical quantized representation.
    /// CPU architecture must not affect this decision.
    fn compare_states(&self, predicted: &S, authoritative: &S) -> PredictionStateComparison;
}

/// Prediction operations orchestrated identically for human and bot clients.
pub trait ClientPrediction {
    /// Simulation-defined sealed predicted state exposed through [`crate::ClientView`].
    type State;
    /// Concrete prediction, history, or gameplay-codec failure.
    type Error: StdError + Send + Sync + 'static;

    /// Return the latest prediction tick represented by the simulation driver.
    fn current_tick(&self) -> SimulationTick;

    /// Establish prediction from one validated full-state projection.
    fn bootstrap(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error>;

    /// Reconcile one validated incremental authoritative projection.
    fn apply_snapshot(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error>;

    /// Queue a canonical four-tick control frame for later forward prediction.
    fn queue_control(&mut self, frame: &ControlFrame) -> Result<(), Self::Error>;

    /// Advance forward prediction through `target`, consuming queued or held input.
    fn advance_to(&mut self, target: SimulationTick) -> Result<(), Self::Error>;

    /// Return the newest locally sealed prediction state.
    fn predicted_state(&self) -> Option<&Self::State>;
}

/// Shared prediction/history coordinator backed by `blackflower-world-prediction`.
pub struct PredictionSession<D, C, S, I> {
    driver: D,
    codec: C,
    coordinator: ReconciliationCoordinator,
    prediction_history: PredictionHistory<S>,
    input_history: InputHistory<I>,
    queued_inputs: BTreeMap<PredictionTick, QueuedInput<I>>,
    held_input: Option<HeldInput<I>>,
    bootstrapped: bool,
}

impl<D, C, S, I> PredictionSession<D, C, S, I> {
    /// Compose a simulation driver and its revision-specific prediction codec.
    #[must_use]
    pub fn new(driver: D, codec: C) -> Self {
        Self {
            driver,
            codec,
            coordinator: ReconciliationCoordinator::network_v1(),
            prediction_history: PredictionHistory::network_v1(),
            input_history: InputHistory::network_v1(),
            queued_inputs: BTreeMap::new(),
            held_input: None,
            bootstrapped: false,
        }
    }

    /// Return the simulation-specific prediction driver.
    #[must_use]
    pub const fn driver(&self) -> &D {
        &self.driver
    }

    /// Return the simulation-specific prediction driver for initial registration.
    #[must_use]
    pub const fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }
}

impl<D, C, S, I> ClientPrediction for PredictionSession<D, C, S, I>
where
    D: PredictionDriver<S, InputFrame<I>>,
    C: PredictionCodec<S, I>,
    S: Clone,
    I: Clone,
{
    type State = S;
    type Error = PredictionSessionError<D::Error, C::Error>;

    fn current_tick(&self) -> SimulationTick {
        SimulationTick::new(self.driver.current_tick())
    }

    fn bootstrap(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        let authoritative = self.decode_authoritative(snapshot)?;
        self.driver
            .restore_authoritative(authoritative.tick.get(), &authoritative.state)
            .map_err(PredictionSessionError::Driver)?;
        self.ensure_driver_tick(authoritative.tick)?;
        self.reset_histories(authoritative.tick, authoritative.state)?;
        Ok(PredictionUpdate::Bootstrapped {
            tick: SimulationTick::new(authoritative.tick.get()),
        })
    }

    fn apply_snapshot(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        if !self.bootstrapped {
            return Err(PredictionSessionError::NotBootstrapped);
        }
        let authoritative = self.decode_authoritative(snapshot)?;
        let codec = &self.codec;
        let outcome = self
            .coordinator
            .reconcile(
                &mut self.driver,
                &mut self.prediction_history,
                &mut self.input_history,
                authoritative,
                |predicted, actual| codec.compare_states(predicted, actual),
            )
            .map_err(PredictionSessionError::Reconciliation)?;
        Ok(map_reconciliation(outcome))
    }

    fn queue_control(&mut self, frame: &ControlFrame) -> Result<(), Self::Error> {
        let input = self
            .codec
            .decode_input(frame)
            .map_err(PredictionSessionError::Codec)?;
        self.queue_frame_ticks(frame, input)
    }

    fn advance_to(&mut self, target: SimulationTick) -> Result<(), Self::Error> {
        if !self.bootstrapped {
            return Err(PredictionSessionError::NotBootstrapped);
        }
        self.validate_advance(target)?;
        while self.driver.current_tick() < target.get() {
            self.advance_one_tick()?;
        }
        Ok(())
    }

    fn predicted_state(&self) -> Option<&Self::State> {
        self.prediction_history
            .newest()
            .map(blackflower_world_prediction::PredictionFrame::state)
    }
}

impl<D, C, S, I> PredictionSession<D, C, S, I>
where
    D: PredictionDriver<S, InputFrame<I>>,
    C: PredictionCodec<S, I>,
    S: Clone,
    I: Clone,
{
    fn decode_authoritative(
        &mut self,
        snapshot: &Snapshot,
    ) -> Result<AuthoritativeSnapshot<S>, PredictionSessionError<D::Error, C::Error>> {
        let authoritative = self
            .codec
            .decode_snapshot(snapshot)
            .map_err(PredictionSessionError::Codec)?;
        if authoritative.tick.get() != snapshot.tick().get() {
            return Err(PredictionSessionError::SnapshotTickMismatch {
                projection: snapshot.tick(),
                prediction: authoritative.tick,
            });
        }
        Ok(authoritative)
    }

    fn reset_histories(
        &mut self,
        tick: PredictionTick,
        state: S,
    ) -> Result<(), PredictionSessionError<D::Error, C::Error>> {
        self.prediction_history = PredictionHistory::network_v1();
        self.input_history = InputHistory::network_v1();
        self.prediction_history.record(tick, state)?;
        self.queued_inputs.clear();
        self.held_input = None;
        self.bootstrapped = true;
        Ok(())
    }

    fn queue_frame_ticks(
        &mut self,
        frame: &ControlFrame,
        input: I,
    ) -> Result<(), PredictionSessionError<D::Error, C::Error>> {
        if frame.execute_tick.get() <= self.driver.current_tick() {
            return Err(PredictionSessionError::InputAlreadyPassed {
                current: PredictionTick::new(self.driver.current_tick()),
                execute_tick: PredictionTick::new(frame.execute_tick.get()),
            });
        }
        let maximum = self.driver.current_tick().saturating_add(
            u64::try_from(NETWORK_HISTORY_TICKS)
                .unwrap_or(u64::MAX)
                .saturating_sub(3),
        );
        if frame.execute_tick.get() > maximum {
            return Err(PredictionSessionError::InputTooFarAhead {
                current: PredictionTick::new(self.driver.current_tick()),
                execute_tick: PredictionTick::new(frame.execute_tick.get()),
                maximum: PredictionTick::new(maximum),
            });
        }
        let mut ticks = Vec::with_capacity(4);
        for offset in 0_u64..4 {
            let value = frame
                .execute_tick
                .get()
                .checked_add(offset)
                .ok_or(PredictionSessionError::TickOverflow)?;
            let tick = PredictionTick::new(value);
            if self.queued_inputs.contains_key(&tick) {
                return Err(PredictionSessionError::InputAlreadyQueued { tick });
            }
            ticks.push(tick);
        }
        for tick in ticks {
            self.queued_inputs.insert(
                tick,
                QueuedInput {
                    sequence: InputSequence::new(frame.sequence.get()),
                    input: input.clone(),
                },
            );
        }
        Ok(())
    }

    fn validate_advance(
        &self,
        target: SimulationTick,
    ) -> Result<(), PredictionSessionError<D::Error, C::Error>> {
        let current = self.driver.current_tick();
        if target.get() < current {
            return Err(PredictionSessionError::PredictionRegressed {
                current: PredictionTick::new(current),
                target: PredictionTick::new(target.get()),
            });
        }
        let required = target.get() - current;
        if required > u64::try_from(NETWORK_HISTORY_TICKS).unwrap_or(u64::MAX) {
            return Err(PredictionSessionError::AdvanceLimitExceeded { required });
        }
        Ok(())
    }

    fn advance_one_tick(&mut self) -> Result<(), PredictionSessionError<D::Error, C::Error>> {
        let next_value = self
            .driver
            .current_tick()
            .checked_add(1)
            .ok_or(PredictionSessionError::TickOverflow)?;
        let tick = PredictionTick::new(next_value);
        let queued = self.input_for_tick(tick);
        let frame = InputFrame::new(tick, queued.sequence, queued.input);
        let state = self
            .driver
            .simulate_tick(PredictionPass::Forward, tick.get(), &frame)
            .map_err(PredictionSessionError::Driver)?;
        self.ensure_driver_tick(tick)?;
        self.input_history.record(frame)?;
        self.prediction_history.record(tick, state)?;
        Ok(())
    }

    fn input_for_tick(&mut self, tick: PredictionTick) -> QueuedInput<I> {
        if let Some(queued) = self.queued_inputs.remove(&tick) {
            self.held_input = Some(HeldInput {
                sequence: queued.sequence,
                input: queued.input.clone(),
                last_tick: tick,
            });
            return queued;
        }
        self.held_input(tick).unwrap_or_else(|| QueuedInput {
            sequence: self
                .held_input
                .as_ref()
                .map_or(InputSequence::new(0), |held| held.sequence),
            input: self.codec.neutral_input(),
        })
    }

    fn held_input(&self, tick: PredictionTick) -> Option<QueuedInput<I>> {
        let held = self.held_input.as_ref()?;
        let age = tick.get().saturating_sub(held.last_tick.get());
        (age <= INPUT_GRACE_TICKS).then(|| QueuedInput {
            sequence: held.sequence,
            input: held.input.clone(),
        })
    }

    fn ensure_driver_tick(
        &self,
        expected: PredictionTick,
    ) -> Result<(), PredictionSessionError<D::Error, C::Error>> {
        let actual = PredictionTick::new(self.driver.current_tick());
        if actual == expected {
            Ok(())
        } else {
            Err(PredictionSessionError::DriverTickMismatch { expected, actual })
        }
    }
}

#[derive(Debug, Clone)]
struct QueuedInput<I> {
    sequence: InputSequence,
    input: I,
}

#[derive(Debug, Clone)]
struct HeldInput<I> {
    sequence: InputSequence,
    input: I,
    last_tick: PredictionTick,
}

/// Failure while coordinating prediction, bounded histories, and gameplay codecs.
#[derive(Debug, thiserror::Error)]
pub enum PredictionSessionError<DE, CE>
where
    DE: StdError + 'static,
    CE: StdError + 'static,
{
    /// Simulation-specific prediction driver failed.
    #[error("prediction driver failed")]
    Driver(#[source] DE),
    /// Gameplay-owned canonical codec failed.
    #[error("prediction codec failed")]
    Codec(#[source] CE),
    /// Existing reconciliation coordination failed after mutation began.
    #[error(transparent)]
    Reconciliation(#[from] ReconciliationError<DE>),
    /// Bounded prediction or input history rejected an ordering invariant.
    #[error(transparent)]
    History(#[from] HistoryError),
    /// A full authoritative state has not established the prediction timeline.
    #[error("prediction has not been bootstrapped")]
    NotBootstrapped,
    /// Gameplay decoding produced a tick different from the projection tick.
    #[error("prediction snapshot tick {prediction} differs from projection tick {projection}")]
    SnapshotTickMismatch {
        /// Canonical replication tick.
        projection: SnapshotTick,
        /// Simulation-defined prediction tick.
        prediction: PredictionTick,
    },
    /// The driver did not advance or restore to the expected tick.
    #[error("prediction driver ended at tick {actual}, expected {expected}")]
    DriverTickMismatch {
        /// Required driver tick.
        expected: PredictionTick,
        /// Actual driver tick.
        actual: PredictionTick,
    },
    /// A prediction target attempted to move backwards.
    #[error("prediction target {target} precedes current tick {current}")]
    PredictionRegressed {
        /// Current driver tick.
        current: PredictionTick,
        /// Rejected target tick.
        target: PredictionTick,
    },
    /// One update requested more work than the retained network history.
    #[error("prediction advance requires {required} ticks, exceeding retained history")]
    AdvanceLimitExceeded {
        /// Requested forward ticks.
        required: u64,
    },
    /// Two local controls attempted to define the same predicted tick.
    #[error("prediction input is already queued for tick {tick}")]
    InputAlreadyQueued {
        /// Conflicting predicted tick.
        tick: PredictionTick,
    },
    /// A newly produced control targeted an already predicted tick.
    #[error("control tick {execute_tick} does not follow current prediction tick {current}")]
    InputAlreadyPassed {
        /// Latest sealed local tick.
        current: PredictionTick,
        /// Rejected first control tick.
        execute_tick: PredictionTick,
    },
    /// A queued control would exceed the bounded future prediction window.
    #[error("control tick {execute_tick} is too far ahead of {current}; maximum is {maximum}")]
    InputTooFarAhead {
        /// Latest sealed local tick.
        current: PredictionTick,
        /// Rejected first control tick.
        execute_tick: PredictionTick,
        /// Latest first tick accepted by the bounded queue.
        maximum: PredictionTick,
    },
    /// Tick arithmetic exhausted its protocol representation.
    #[error("prediction tick overflow")]
    TickOverflow,
}

fn map_reconciliation(outcome: ReconciliationOutcome) -> PredictionUpdate {
    match outcome {
        ReconciliationOutcome::Converged {
            authoritative_tick, ..
        } => PredictionUpdate::Converged {
            tick: SimulationTick::new(authoritative_tick.get()),
        },
        ReconciliationOutcome::Reconciled {
            authoritative_tick,
            resimulated_ticks,
            ..
        } => PredictionUpdate::Reconciled {
            authoritative_tick: SimulationTick::new(authoritative_tick.get()),
            resimulated_ticks,
        },
        ReconciliationOutcome::HardResyncRequired { reason } => {
            PredictionUpdate::HardResyncRequired { reason }
        }
    }
}
