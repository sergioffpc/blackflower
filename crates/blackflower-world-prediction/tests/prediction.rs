use std::io;
use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::{Arc, Mutex};

use blackflower_ecs::{Component, ComponentId, EntityId, Read, TickDelta, World, Write};
use blackflower_world_prediction::{
    AuthoritativeSnapshot, CaptureTickInputsSystem, CommitStateTransitionsSystem,
    DeriveActorActionsSystem, DeriveStateTransitionsSystem, HardResyncReason, HistoryError,
    InputFrame, InputHistory, InputSequence, PREDICTION_TICK_DELTA_SECONDS, PredictionDriver,
    PredictionError, PredictionExecution, PredictionHistory, PredictionPass, PredictionPhase,
    PredictionPipeline, PredictionStateComparison, PredictionTick, PredictionWorld,
    PrepareTickSystem, ReconciliationCoordinator, ReconciliationOutcome, SealTickSystem,
    SolveRigidBodyDynamicsSystem, SubmitTickOutputsSystem,
};
use bytemuck::{Pod, Zeroable};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct Probe(u8);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct Position(i64);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct Movement(i64);

#[test]
fn phase_names_are_stable() {
    assert_eq!(
        PredictionPhase::ORDER.map(PredictionPhase::name),
        [
            "PrepareTick",
            "CaptureTickInputs",
            "DeriveActorActions",
            "SolveRigidBodyDynamics",
            "DeriveStateTransitions",
            "CommitStateTransitions",
            "SealTick",
            "SubmitTickOutputs",
        ]
    );
}

#[test]
fn prepare_tick_system_names_are_stable() {
    assert_eq!(
        PrepareTickSystem::ORDER.map(PrepareTickSystem::name),
        [
            "OpenTick",
            "ResetTickTransientStorage",
            "ActivateScheduledCommits",
        ]
    );
}

#[test]
fn capture_tick_inputs_system_names_are_stable() {
    assert_eq!(
        CaptureTickInputsSystem::ORDER.map(CaptureTickInputsSystem::name),
        ["CapturePredictionInputFrame"]
    );
}

#[test]
fn derive_actor_actions_system_names_are_stable() {
    assert_eq!(
        DeriveActorActionsSystem::ORDER.map(DeriveActorActionsSystem::name),
        [
            "DeriveLocomotionActions",
            "DeriveWeaponActions",
            "DeriveInteractionActions",
        ]
    );
}

#[test]
fn solve_rigid_body_dynamics_system_names_are_stable() {
    assert_eq!(
        SolveRigidBodyDynamicsSystem::ORDER.map(SolveRigidBodyDynamicsSystem::name),
        [
            "ApplyCharacterControllerInputs",
            "ApplyRigidBodyInputs",
            "AdvanceRigidBodyWorld",
            "RefreshCharacterGroundState",
            "CaptureRigidBodyState",
            "CaptureCharacterState",
            "CaptureContactFacts",
        ]
    );
}

#[test]
fn derive_state_transitions_system_names_are_stable() {
    assert_eq!(
        DeriveStateTransitionsSystem::ORDER.map(DeriveStateTransitionsSystem::name),
        [
            "DeriveActorConditionTransitions",
            "DeriveWeaponStateTransitions",
            "DeriveInventoryStateTransitions",
            "DeriveWorldObjectStateTransitions",
            "CanonicalizeTransitionCandidates",
        ]
    );
}

#[test]
fn commit_state_transitions_system_names_are_stable() {
    assert_eq!(
        CommitStateTransitionsSystem::ORDER.map(CommitStateTransitionsSystem::name),
        [
            "EvaluateTransitionPreconditions",
            "ResolveTransitionConflicts",
            "BuildTransitionCommit",
            "ValidateTransitionCommit",
            "CommitAcceptedTransitions",
            "CaptureCommittedTransitions",
        ]
    );
}

#[test]
fn seal_tick_system_names_are_stable() {
    assert_eq!(
        SealTickSystem::ORDER.map(SealTickSystem::name),
        [
            "ValidatePredictedState",
            "CanonicalizePredictedEvents",
            "ComputePredictedStateHash",
            "SealPredictedState",
        ]
    );
}

#[test]
fn submit_tick_outputs_system_names_are_stable() {
    assert_eq!(
        SubmitTickOutputsSystem::ORDER.map(SubmitTickOutputsSystem::name),
        [
            "BuildTickOutputBatch",
            "ClassifyPredictionOutputsForPass",
            "ReconcilePredictedEvents",
            "SubmitTickOutputBatch",
        ]
    );
}

#[test]
fn pipeline_orders_systems_by_phase_instead_of_registration_order() -> TestResult {
    let mut world = World::new()?;
    let probe = world.register_component::<Probe>()?;
    let entity = world.spawn()?;
    world.insert(entity, probe, Probe(0))?;

    let pipeline = PredictionPipeline::register(&mut world)?;
    let observed = Arc::new(Mutex::new(Vec::new()));

    for phase in PredictionPhase::ORDER.into_iter().rev() {
        let system_name = format!("Record{}", phase.name());
        let observed_by_system = Arc::clone(&observed);
        world
            .system(&system_name, "Probe")?
            .phase(pipeline.phase(phase))?
            .project(Read::<Probe>::field(0))?
            .each(move |_context, _entity, _probe| {
                let mut observed = observed_by_system
                    .lock()
                    .map_err(|_error| io::Error::other("phase order lock poisoned"))?;
                observed.push(phase);
                Ok(())
            })?;
    }

    assert!(world.progress(TickDelta::from_seconds(PREDICTION_TICK_DELTA_SECONDS)?)?);

    let observed = observed
        .lock()
        .map_err(|_error| io::Error::other("phase order lock poisoned"))?;
    assert_eq!(observed.as_slice(), PredictionPhase::ORDER);
    Ok(())
}

#[test]
fn prediction_world_exposes_forward_and_resimulation_passes() -> TestResult {
    let mut prediction = PredictionWorld::new()?;
    let probe = prediction.ecs_mut().register_component::<Probe>()?;
    let entity = prediction.ecs_mut().spawn()?;
    prediction.ecs_mut().insert(entity, probe, Probe(0))?;

    let execution_context = prediction.execution_context();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_by_system = Arc::clone(&observed);
    let prepare_tick = prediction.phase(PredictionPhase::PrepareTick);
    prediction
        .ecs_mut()
        .system("RecordPredictionExecution", "Probe")?
        .phase(prepare_tick)?
        .project(Read::<Probe>::field(0))?
        .each(move |context, _entity, _probe| {
            let mut observed = observed_by_system
                .lock()
                .map_err(|_error| io::Error::other("execution lock poisoned"))?;
            observed.push((
                execution_context.current(),
                context.delta().as_seconds().to_bits(),
            ));
            Ok(())
        })?;

    assert!(prediction.tick(PredictionPass::Forward)?);
    assert!(prediction.tick(PredictionPass::Resimulation)?);

    assert_eq!(prediction.current_tick(), PredictionTick::new(2));
    assert_eq!(
        observed
            .lock()
            .map_err(|_error| io::Error::other("execution lock poisoned"))?
            .as_slice(),
        [
            (
                PredictionExecution {
                    tick: PredictionTick::new(1),
                    pass: PredictionPass::Forward,
                },
                PREDICTION_TICK_DELTA_SECONDS.to_bits(),
            ),
            (
                PredictionExecution {
                    tick: PredictionTick::new(2),
                    pass: PredictionPass::Resimulation,
                },
                PREDICTION_TICK_DELTA_SECONDS.to_bits(),
            ),
        ]
    );
    Ok(())
}

#[test]
fn failed_prediction_tick_stops_until_a_complete_restore() -> TestResult {
    let mut prediction = PredictionWorld::new()?;
    let probe = prediction.ecs_mut().register_component::<Probe>()?;
    let entity = prediction.ecs_mut().spawn()?;
    prediction.ecs_mut().insert(entity, probe, Probe(0))?;

    let capture_inputs = prediction.phase(PredictionPhase::CaptureTickInputs);
    prediction
        .ecs_mut()
        .system("FailPredictionAfterWrite", "Probe")?
        .phase(capture_inputs)?
        .project(Write::<Probe>::field(0))?
        .each(|_context, _entity, probe| {
            probe.0 = probe.0.saturating_add(1);
            Err(io::Error::other("intentional prediction failure").into())
        })?;

    let error = match prediction.tick(PredictionPass::Forward) {
        Err(error) => error,
        Ok(_) => return Err(io::Error::other("the prediction system must fail").into()),
    };
    assert!(prediction.is_faulted());
    assert_eq!(prediction.fault(), Some(&error));
    assert_eq!(prediction.current_tick(), PredictionTick::ZERO);
    assert_eq!(
        prediction.ecs().get(entity, probe)?.map(|probe| probe.0),
        Some(1)
    );

    assert_eq!(prediction.tick(PredictionPass::Forward), Err(error));
    assert_eq!(
        prediction.ecs().get(entity, probe)?.map(|probe| probe.0),
        Some(1)
    );

    prediction.ecs_mut().insert(entity, probe, Probe(0))?;
    prediction.restore_tick_for_reconciliation(PredictionTick::ZERO);
    assert!(!prediction.is_faulted());
    Ok(())
}

#[test]
fn histories_are_bounded_and_reject_regression() -> TestResult {
    let capacity =
        NonZeroUsize::new(2).ok_or_else(|| io::Error::other("test capacity must be nonzero"))?;
    let mut predictions = PredictionHistory::new(capacity);
    predictions.record(PredictionTick::ZERO, 0_i64)?;
    predictions.record(PredictionTick::new(1), 1_i64)?;
    predictions.record(PredictionTick::new(2), 2_i64)?;

    assert_eq!(
        predictions.oldest().map(|frame| frame.tick()),
        Some(PredictionTick::new(1))
    );
    assert_eq!(
        predictions.record(PredictionTick::new(2), 20_i64),
        Err(HistoryError::TickNotAfterLatest {
            latest: PredictionTick::new(2),
            next: PredictionTick::new(2),
        })
    );

    let mut inputs = InputHistory::new(capacity);
    inputs.record(InputFrame::new(
        PredictionTick::new(1),
        InputSequence::new(2),
        1_i64,
    ))?;
    inputs.record(InputFrame::new(
        PredictionTick::new(2),
        InputSequence::new(2),
        1_i64,
    ))?;
    assert_eq!(
        inputs.record(InputFrame::new(
            PredictionTick::new(3),
            InputSequence::new(1),
            1_i64,
        )),
        Err(HistoryError::InputSequenceRegressed {
            latest: InputSequence::new(2),
            next: InputSequence::new(1),
        })
    );
    Ok(())
}

#[test]
fn reconciliation_restores_and_resimulates_the_prediction_pipeline() -> TestResult {
    let mut driver = TestDriver::new()?;
    let capacity =
        NonZeroUsize::new(8).ok_or_else(|| io::Error::other("test capacity must be nonzero"))?;
    let mut predictions = PredictionHistory::new(capacity);
    let mut inputs = InputHistory::new(capacity);
    predictions.record(PredictionTick::ZERO, 0_i64)?;

    for tick in 1..=3 {
        let frame = InputFrame::new(PredictionTick::new(tick), InputSequence::new(tick), 1_i64);
        let state = driver.advance(PredictionPass::Forward, &frame)?;
        predictions.record(frame.tick(), state)?;
        inputs.record(frame)?;
    }
    driver.clear_observed_execution()?;

    let coordinator = ReconciliationCoordinator::new(
        NonZeroU64::new(8)
            .ok_or_else(|| io::Error::other("test re-simulation limit must be nonzero"))?,
    );
    let outcome = coordinator.reconcile(
        &mut driver,
        &mut predictions,
        &mut inputs,
        AuthoritativeSnapshot {
            tick: PredictionTick::new(1),
            acknowledged_input: Some(InputSequence::new(1)),
            state: 10_i64,
        },
        |predicted, authoritative| {
            PredictionStateComparison::from_within_tolerance(predicted == authoritative)
        },
    )?;

    assert_eq!(
        outcome,
        ReconciliationOutcome::Reconciled {
            authoritative_tick: PredictionTick::new(1),
            target_tick: PredictionTick::new(3),
            resimulated_ticks: 2,
            acknowledged_input: Some(InputSequence::new(1)),
        }
    );
    verify_resimulated_scenario(&driver, &predictions, &inputs)
}

fn verify_resimulated_scenario(
    driver: &TestDriver,
    predictions: &PredictionHistory<i64>,
    inputs: &InputHistory<i64>,
) -> TestResult {
    assert_eq!(driver.position()?, 12);
    assert_eq!(
        predictions
            .get(PredictionTick::new(1))
            .map(|frame| *frame.state()),
        Some(10)
    );
    assert_eq!(
        predictions
            .get(PredictionTick::new(2))
            .map(|frame| *frame.state()),
        Some(11)
    );
    assert_eq!(
        predictions
            .get(PredictionTick::new(3))
            .map(|frame| *frame.state()),
        Some(12)
    );
    assert_eq!(
        inputs.oldest().map(InputFrame::tick),
        Some(PredictionTick::new(2))
    );
    assert_eq!(
        driver.observed_execution()?,
        vec![
            PredictionExecution {
                tick: PredictionTick::new(2),
                pass: PredictionPass::Resimulation,
            },
            PredictionExecution {
                tick: PredictionTick::new(3),
                pass: PredictionPass::Resimulation,
            },
        ]
    );
    Ok(())
}

#[test]
fn reconciliation_skips_resimulation_when_state_already_converged() -> TestResult {
    let mut driver = TestDriver::new()?;
    let capacity =
        NonZeroUsize::new(8).ok_or_else(|| io::Error::other("test capacity must be nonzero"))?;
    let mut predictions = PredictionHistory::new(capacity);
    let mut inputs = InputHistory::new(capacity);
    predictions.record(PredictionTick::ZERO, 0_i64)?;

    for tick in 1..=3 {
        let frame = InputFrame::new(PredictionTick::new(tick), InputSequence::new(tick), 1_i64);
        let state = driver.advance(PredictionPass::Forward, &frame)?;
        predictions.record(frame.tick(), state)?;
        inputs.record(frame)?;
    }
    driver.clear_observed_execution()?;

    let coordinator = ReconciliationCoordinator::new(
        NonZeroU64::new(8)
            .ok_or_else(|| io::Error::other("test re-simulation limit must be nonzero"))?,
    );
    let outcome = coordinator.reconcile(
        &mut driver,
        &mut predictions,
        &mut inputs,
        AuthoritativeSnapshot {
            tick: PredictionTick::new(2),
            acknowledged_input: Some(InputSequence::new(2)),
            state: 2_i64,
        },
        |predicted, authoritative| {
            PredictionStateComparison::from_within_tolerance(predicted == authoritative)
        },
    )?;

    assert_eq!(
        outcome,
        ReconciliationOutcome::Converged {
            authoritative_tick: PredictionTick::new(2),
            acknowledged_input: Some(InputSequence::new(2)),
        }
    );
    assert_eq!(driver.position()?, 3);
    assert!(driver.observed_execution()?.is_empty());
    assert_eq!(
        predictions.oldest().map(|frame| frame.tick()),
        Some(PredictionTick::new(2))
    );
    assert_eq!(
        inputs.oldest().map(InputFrame::tick),
        Some(PredictionTick::new(3))
    );
    Ok(())
}

#[test]
fn reconciliation_requires_hard_resync_before_mutation_when_input_is_missing() -> TestResult {
    let mut driver = TestDriver::new()?;
    let capacity =
        NonZeroUsize::new(8).ok_or_else(|| io::Error::other("test capacity must be nonzero"))?;
    let mut predictions = PredictionHistory::new(capacity);
    let mut inputs = InputHistory::new(capacity);
    predictions.record(PredictionTick::ZERO, 0_i64)?;

    for tick in 1..=3 {
        let frame = InputFrame::new(PredictionTick::new(tick), InputSequence::new(tick), 1_i64);
        let state = driver.advance(PredictionPass::Forward, &frame)?;
        predictions.record(frame.tick(), state)?;
        if tick != 2 {
            inputs.record(frame)?;
        }
    }
    driver.clear_observed_execution()?;

    let coordinator = ReconciliationCoordinator::new(
        NonZeroU64::new(8)
            .ok_or_else(|| io::Error::other("test re-simulation limit must be nonzero"))?,
    );
    let outcome = coordinator.reconcile(
        &mut driver,
        &mut predictions,
        &mut inputs,
        AuthoritativeSnapshot {
            tick: PredictionTick::new(1),
            acknowledged_input: Some(InputSequence::new(1)),
            state: 10_i64,
        },
        |predicted, authoritative| {
            PredictionStateComparison::from_within_tolerance(predicted == authoritative)
        },
    )?;

    assert_eq!(
        outcome,
        ReconciliationOutcome::HardResyncRequired {
            reason: HardResyncReason::MissingInput {
                tick: PredictionTick::new(2),
            },
        }
    );
    assert_eq!(driver.position()?, 3);
    assert!(driver.observed_execution()?.is_empty());
    assert_eq!(
        predictions
            .get(PredictionTick::new(1))
            .map(|frame| *frame.state()),
        Some(1)
    );
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum DriverError {
    #[error(transparent)]
    Ecs(#[from] blackflower_ecs::Error),
    #[error(transparent)]
    Prediction(#[from] PredictionError),
    #[error("predicted position component is missing")]
    MissingPosition,
    #[error("execution observation lock poisoned")]
    ObservationLock,
    #[error("gameplay tick does not match input frame")]
    TickMismatch,
}

struct TestDriver {
    prediction: PredictionWorld,
    entity: EntityId,
    position: ComponentId<Position>,
    movement: ComponentId<Movement>,
    observed: Arc<Mutex<Vec<PredictionExecution>>>,
}

impl TestDriver {
    fn new() -> Result<Self, DriverError> {
        let mut prediction = PredictionWorld::new()?;
        let position = prediction.ecs_mut().register_component::<Position>()?;
        let movement = prediction.ecs_mut().register_component::<Movement>()?;
        let entity = prediction.ecs_mut().spawn()?;
        prediction.ecs_mut().insert(entity, position, Position(0))?;
        prediction.ecs_mut().insert(entity, movement, Movement(0))?;

        let execution = prediction.execution_context();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_by_system = Arc::clone(&observed);
        let solve_rigid_body_dynamics = prediction.phase(PredictionPhase::SolveRigidBodyDynamics);
        prediction
            .ecs_mut()
            .system("IntegratePredictedMovement", "Position, Movement")?
            .phase(solve_rigid_body_dynamics)?
            .project((Write::<Position>::field(0), Read::<Movement>::field(1)))?
            .each(move |_context, _entity, (position, movement)| {
                position.0 += movement.0;
                let mut observed = observed_by_system
                    .lock()
                    .map_err(|_error| io::Error::other("execution observation lock poisoned"))?;
                observed.push(execution.current());
                Ok(())
            })?;

        Ok(Self {
            prediction,
            entity,
            position,
            movement,
            observed,
        })
    }

    fn advance(
        &mut self,
        pass: PredictionPass,
        input: &InputFrame<i64>,
    ) -> Result<i64, DriverError> {
        self.prediction
            .ecs_mut()
            .insert(self.entity, self.movement, Movement(*input.input()))?;
        let _should_continue = self.prediction.tick(pass)?;
        self.position()
    }

    fn position(&self) -> Result<i64, DriverError> {
        self.prediction
            .ecs()
            .get(self.entity, self.position)?
            .map(|position| position.0)
            .ok_or(DriverError::MissingPosition)
    }

    fn observed_execution(&self) -> Result<Vec<PredictionExecution>, DriverError> {
        self.observed
            .lock()
            .map(|observed| observed.clone())
            .map_err(|_error| DriverError::ObservationLock)
    }

    fn clear_observed_execution(&self) -> Result<(), DriverError> {
        self.observed
            .lock()
            .map(|mut observed| observed.clear())
            .map_err(|_error| DriverError::ObservationLock)
    }
}

impl PredictionDriver<i64, InputFrame<i64>> for TestDriver {
    type Error = DriverError;

    fn current_tick(&self) -> u64 {
        self.prediction.current_tick().get()
    }

    fn restore_authoritative(&mut self, tick: u64, state: &i64) -> Result<(), Self::Error> {
        self.prediction
            .ecs_mut()
            .insert(self.entity, self.position, Position(*state))?;
        self.prediction
            .restore_tick_for_reconciliation(PredictionTick::new(tick));
        Ok(())
    }

    fn simulate_tick(
        &mut self,
        pass: PredictionPass,
        tick: u64,
        input: &InputFrame<i64>,
    ) -> Result<i64, Self::Error> {
        if tick != input.tick().get() {
            return Err(DriverError::TickMismatch);
        }
        self.advance(pass, input)
    }
}
