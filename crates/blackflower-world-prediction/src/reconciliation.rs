use std::error::Error as StdError;
use std::num::NonZeroU64;

use crate::telemetry::{self, ReconciliationObservation};
use crate::{
    HistoryError, InputFrame, InputHistory, InputSequence, PredictionDriver, PredictionHistory,
    PredictionPass, PredictionStateComparison, PredictionTick,
};

/// Network v1 maximum reconciliation rollback.
pub const NETWORK_MAX_RECONCILIATION_TICKS: u64 = 64;

/// Authoritative predicted-state subset sealed by the server at one tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoritativeSnapshot<S> {
    /// Authoritative tick represented by `state`.
    pub tick: PredictionTick,
    /// Latest local control frame included by the server, when known.
    pub acknowledged_input: Option<InputSequence>,
    /// Simulation-defined subset of state needed to restore client prediction.
    pub state: S,
}

/// Reason reconciliation cannot safely re-simulate from the supplied snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardResyncReason {
    /// The snapshot is newer than the local predicted state.
    SnapshotAhead {
        /// Tick in the authoritative snapshot.
        snapshot_tick: PredictionTick,
        /// Latest local prediction tick.
        current_tick: PredictionTick,
    },
    /// The local state used to compare the authoritative snapshot was evicted.
    MissingPredictedState {
        /// Snapshot tick no longer present in prediction history.
        tick: PredictionTick,
    },
    /// Reconciliation would exceed the configured re-simulation work bound.
    ResimulationLimitExceeded {
        /// Number of ticks required to return to the current timeline.
        required: u64,
        /// Configured maximum re-simulation length.
        maximum: u64,
    },
    /// An input needed to re-simulate a tick was evicted or never recorded.
    MissingInput {
        /// First tick for which no input was available.
        tick: PredictionTick,
    },
    /// The tick representation cannot advance far enough to validate re-simulation.
    TickOverflow,
}

/// Result of processing one authoritative snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconciliationOutcome {
    /// Predicted state satisfied its comparison policy at the snapshot tick.
    Converged {
        /// Authoritative tick that was compared.
        authoritative_tick: PredictionTick,
        /// Latest input sequence included by the server.
        acknowledged_input: Option<InputSequence>,
    },
    /// State was restored and zero or more subsequent inputs were re-simulated.
    Reconciled {
        /// Tick at which authoritative state replaced predicted state.
        authoritative_tick: PredictionTick,
        /// Tick reached after re-simulation.
        target_tick: PredictionTick,
        /// Number of prediction pipeline invocations made in re-simulation.
        resimulated_ticks: u64,
        /// Latest input sequence included by the server.
        acknowledged_input: Option<InputSequence>,
    },
    /// Bounded rollback is impossible and the caller must request a full state.
    HardResyncRequired {
        /// Precise reason re-simulation was rejected.
        reason: HardResyncReason,
    },
}

/// Failure after reconciliation has started mutating the prediction world.
#[derive(Debug, thiserror::Error)]
pub enum ReconciliationError<E: StdError + 'static> {
    /// Simulation-specific restore or re-simulation logic failed.
    #[error("reconciliation driver failed")]
    Driver(#[source] E),
    /// Prediction history rejected a replacement or re-simulated state.
    #[error(transparent)]
    History(#[from] HistoryError),
    /// The driver did not move its timeline to the tick requested by the coordinator.
    #[error("reconciliation driver ended at tick {actual}, expected tick {expected}")]
    DriverTickMismatch {
        /// Tick expected after restore or re-simulation.
        expected: PredictionTick,
        /// Tick reported by the driver.
        actual: PredictionTick,
    },
}

/// Bounded controller for authoritative correction and input re-simulation.
#[derive(Debug, Clone, Copy)]
pub struct ReconciliationCoordinator {
    max_resimulation_ticks: NonZeroU64,
}

impl ReconciliationCoordinator {
    /// Construct the normative network v1 64-tick reconciliation bound.
    #[must_use]
    pub fn network_v1() -> Self {
        Self::new(NonZeroU64::new(NETWORK_MAX_RECONCILIATION_TICKS).unwrap_or(NonZeroU64::MIN))
    }

    /// Construct a coordinator with an explicit re-simulation work bound.
    #[must_use]
    pub const fn new(max_resimulation_ticks: NonZeroU64) -> Self {
        Self {
            max_resimulation_ticks,
        }
    }

    /// Return the maximum number of ticks re-simulated for one snapshot.
    #[must_use]
    pub const fn max_resimulation_ticks(self) -> NonZeroU64 {
        self.max_resimulation_ticks
    }

    /// Reconcile one authoritative snapshot against retained prediction state.
    ///
    /// All re-simulation preconditions are validated before the driver or histories
    /// are mutated. A driver failure after restoration leaves the caller
    /// responsible for requesting a hard resync.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "blackflower_world_prediction",
            name = "prediction_reconciliation",
            level = "info",
            skip_all,
            fields(
                authoritative_tick = snapshot.tick.get(),
                target_tick = tracing::field::Empty,
                result = tracing::field::Empty,
                reason = tracing::field::Empty,
            ),
        )
    )]
    pub fn reconcile<D, S, I>(
        self,
        driver: &mut D,
        prediction_history: &mut PredictionHistory<S>,
        input_history: &mut InputHistory<I>,
        snapshot: AuthoritativeSnapshot<S>,
        compare_states: impl FnOnce(&S, &S) -> PredictionStateComparison,
    ) -> Result<ReconciliationOutcome, ReconciliationError<D::Error>>
    where
        D: PredictionDriver<S, InputFrame<I>>,
    {
        telemetry::describe_metrics();
        let target_tick = PredictionTick::new(driver.current_tick());
        #[cfg(feature = "tracing")]
        tracing::Span::current().record("target_tick", target_tick.get());
        let observation = ReconciliationObservation::start();
        let plan = reconciliation_plan(
            prediction_history,
            input_history,
            &snapshot,
            target_tick,
            self.max_resimulation_ticks,
            compare_states,
        );
        let result = match plan {
            Ok(ReconciliationPlan::Converged) => {
                prediction_history.discard_before(snapshot.tick);
                input_history.discard_through(snapshot.tick);
                Ok(ReconciliationOutcome::Converged {
                    authoritative_tick: snapshot.tick,
                    acknowledged_input: snapshot.acknowledged_input,
                })
            }
            Ok(ReconciliationPlan::Resimulate) => resimulate(
                driver,
                prediction_history,
                input_history,
                snapshot,
                target_tick,
            ),
            Err(reason) => Ok(ReconciliationOutcome::HardResyncRequired { reason }),
        };
        observation.finish(&result);
        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconciliationPlan {
    Converged,
    Resimulate,
}

fn reconciliation_plan<S, I>(
    prediction_history: &PredictionHistory<S>,
    input_history: &InputHistory<I>,
    snapshot: &AuthoritativeSnapshot<S>,
    target_tick: PredictionTick,
    max_resimulation_ticks: NonZeroU64,
    compare_states: impl FnOnce(&S, &S) -> PredictionStateComparison,
) -> Result<ReconciliationPlan, HardResyncReason> {
    if snapshot.tick > target_tick {
        return Err(HardResyncReason::SnapshotAhead {
            snapshot_tick: snapshot.tick,
            current_tick: target_tick,
        });
    }
    let frame =
        prediction_history
            .get(snapshot.tick)
            .ok_or(HardResyncReason::MissingPredictedState {
                tick: snapshot.tick,
            })?;
    if compare_states(frame.state(), &snapshot.state).is_within_tolerance() {
        return Ok(ReconciliationPlan::Converged);
    }
    let required = target_tick.get() - snapshot.tick.get();
    if required > max_resimulation_ticks.get() {
        return Err(HardResyncReason::ResimulationLimitExceeded {
            required,
            maximum: max_resimulation_ticks.get(),
        });
    }
    match missing_resimulation_input(input_history, snapshot.tick, target_tick) {
        Some(reason) => Err(reason),
        None => Ok(ReconciliationPlan::Resimulate),
    }
}

fn resimulate<D, S, I>(
    driver: &mut D,
    prediction_history: &mut PredictionHistory<S>,
    input_history: &mut InputHistory<I>,
    snapshot: AuthoritativeSnapshot<S>,
    target_tick: PredictionTick,
) -> Result<ReconciliationOutcome, ReconciliationError<D::Error>>
where
    D: PredictionDriver<S, InputFrame<I>>,
{
    driver
        .restore_authoritative(snapshot.tick.get(), &snapshot.state)
        .map_err(ReconciliationError::Driver)?;
    ensure_driver_tick(driver, snapshot.tick)?;

    prediction_history.truncate_after(snapshot.tick);
    prediction_history.replace(snapshot.tick, snapshot.state)?;

    let mut resimulated_ticks = 0_u64;
    for input in input_history.after(snapshot.tick) {
        if input.tick() > target_tick {
            break;
        }
        let state = driver
            .simulate_tick(PredictionPass::Resimulation, input.tick().get(), input)
            .map_err(ReconciliationError::Driver)?;
        ensure_driver_tick(driver, input.tick())?;
        prediction_history.record(input.tick(), state)?;
        resimulated_ticks += 1;
    }

    prediction_history.discard_before(snapshot.tick);
    input_history.discard_through(snapshot.tick);
    Ok(ReconciliationOutcome::Reconciled {
        authoritative_tick: snapshot.tick,
        target_tick,
        resimulated_ticks,
        acknowledged_input: snapshot.acknowledged_input,
    })
}

fn missing_resimulation_input<I>(
    input_history: &InputHistory<I>,
    snapshot_tick: PredictionTick,
    target_tick: PredictionTick,
) -> Option<HardResyncReason> {
    let mut previous_tick = snapshot_tick;
    for input in input_history.after(snapshot_tick) {
        if input.tick() > target_tick {
            break;
        }
        let Some(expected_tick) = previous_tick.checked_next() else {
            return Some(HardResyncReason::TickOverflow);
        };
        if input.tick() != expected_tick {
            return Some(HardResyncReason::MissingInput {
                tick: expected_tick,
            });
        }
        previous_tick = input.tick();
    }

    if previous_tick == target_tick {
        None
    } else {
        match previous_tick.checked_next() {
            Some(tick) => Some(HardResyncReason::MissingInput { tick }),
            None => Some(HardResyncReason::TickOverflow),
        }
    }
}

fn ensure_driver_tick<D, S, I>(
    driver: &D,
    expected: PredictionTick,
) -> Result<(), ReconciliationError<D::Error>>
where
    D: PredictionDriver<S, InputFrame<I>>,
{
    let actual = PredictionTick::new(driver.current_tick());
    if actual == expected {
        Ok(())
    } else {
        Err(ReconciliationError::DriverTickMismatch { expected, actual })
    }
}
