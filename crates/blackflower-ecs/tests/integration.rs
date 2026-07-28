use std::error::Error as StdError;
use std::io;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use blackflower_ecs::{
    BuiltinPhase, Component, ComponentId, EntityId, Error, Optional, PairRead, PairWrite, PhaseId,
    ProjectionError, Read, RunError, SystemResult, Tag, TagId, TickDelta, World, WorldBuilder,
    Write,
};
use bytemuck::{Pod, Zeroable};

type TestResult = Result<(), Box<dyn StdError>>;

#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Component)]
#[repr(C)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Component)]
#[repr(C)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Component)]
#[repr(C)]
struct Weight {
    value: u32,
}

#[derive(Tag)]
struct Active;

struct DropMarker(Arc<AtomicBool>);

impl Drop for DropMarker {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

struct PanickingDrop;

impl Drop for PanickingDrop {
    #[allow(
        clippy::panic,
        reason = "the FFI context destructor must be tested with an intentional panic"
    )]
    fn drop(&mut self) {
        panic!("caught context destructor panic");
    }
}

#[test]
fn abi_world_entities_components_and_tags() -> TestResult {
    assert_eq!(blackflower_ecs::FLECS_VERSION, (4, 1, 6));
    assert_eq!(<Position as Component>::NAME, "Position");
    assert_eq!(<Active as Tag>::NAME, "Active");
    assert_eq!(TickDelta::from_seconds(0.0), Err(Error::InvalidTickDelta));
    assert_eq!(
        TickDelta::from_seconds(f32::NAN),
        Err(Error::InvalidTickDelta)
    );

    let mut world = World::new()?;
    let position = world.register_component::<Position>()?;
    let active = world.register_tag::<Active>()?;
    let entity = world.spawn_named("player")?;

    world.insert(entity, position, Position { x: 10.0, y: -2.0 })?;
    world.add_tag(entity, active)?;
    assert_eq!(
        world.get(entity, position)?,
        Some(Position { x: 10.0, y: -2.0 })
    );
    assert!(world.has_tag(entity, active)?);

    let changed = world.with_mut(entity, position, |value| {
        value.x += 5.0;
        value.x
    })?;
    assert_eq!(changed.map(f32::to_bits), Some(15.0_f32.to_bits()));
    world.remove(entity, position)?;
    world.remove_tag(entity, active)?;
    assert!(!world.has(entity, position)?);
    assert!(!world.has_tag(entity, active)?);

    world.despawn(entity)?;
    assert!(!world.is_alive(entity)?);
    assert_eq!(world.get(entity, position), Err(Error::DeadEntity));
    Ok(())
}

#[test]
fn registration_and_world_identity_are_checked() -> TestResult {
    #[derive(Debug, PartialEq, Eq, Tag)]
    #[ecs(name = "Position")]
    struct OtherPosition;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Component)]
    #[repr(transparent)]
    struct Empty([u8; 0]);

    let mut left = World::new()?;
    let mut right = World::new()?;
    let left_position = left.register_component::<Position>()?;
    let right_entity = right.spawn()?;

    assert_eq!(
        left.register_component::<Position>(),
        Err(Error::DuplicateType("Position"))
    );
    assert_eq!(
        left.register_tag::<OtherPosition>(),
        Err(Error::DuplicateName("Position".to_owned()))
    );
    assert_eq!(
        left.register_component::<Empty>(),
        Err(Error::InvalidComponentLayout("Empty"))
    );
    assert_eq!(
        right.insert(right_entity, left_position, Position { x: 0.0, y: 0.0 }),
        Err(Error::WrongWorld)
    );
    let pipeline = left
        .pipeline("LeftPipeline", "flecs.system.System")?
        .build()?;
    assert_eq!(right.set_pipeline(pipeline), Err(Error::WrongWorld));
    assert_eq!(
        right.run_pipeline(pipeline, TickDelta::from_seconds(0.01)?),
        Err(RunError::WrongWorld)
    );
    Ok(())
}

#[test]
fn query_projects_read_write_optional_sparse_and_pairs() -> TestResult {
    let mut world = World::new()?;
    let position = world.register_component::<Position>()?;
    let velocity = world.register_sparse_component::<Velocity>()?;
    let weight = world.register_component::<Weight>()?;
    let active = world.register_tag::<Active>()?;
    let target_a = world.spawn_named("TargetA")?;
    let target_b = world.spawn_named("TargetB")?;
    let first = world.spawn()?;
    let second = world.spawn()?;

    world.insert(first, position, Position { x: 1.0, y: 2.0 })?;
    world.insert(first, velocity, Velocity { x: 3.0, y: 4.0 })?;
    world.insert(second, position, Position { x: 5.0, y: 6.0 })?;
    world.add_tag(first, active)?;
    world.insert_pair(first, weight, target_a, Weight { value: 10 })?;
    world.insert_pair(first, weight, target_b, Weight { value: 20 })?;

    verify_optional_projection(&mut world, position, first, second)?;
    verify_tag_projection(&mut world, active, first)?;
    verify_pair_projection(&mut world, weight, first, target_a, target_b)
}

fn verify_optional_projection(
    world: &mut World,
    position: ComponentId<Position>,
    first: EntityId,
    second: EntityId,
) -> TestResult {
    let mut optional_count = 0_u32;
    let mut query = world.query("Position, ?Velocity")?.project((
        Write::<Position>::field(0),
        Optional::new(Read::<Velocity>::field(1)),
    ))?;
    query.each(|_entity, (position, velocity)| {
        optional_count += 1;
        if let Some(velocity) = velocity {
            position.x += velocity.x;
        }
    })?;
    drop(query);
    assert_eq!(optional_count, 2);
    assert_eq!(
        world.get(first, position)?.map(|value| value.x.to_bits()),
        Some(4.0_f32.to_bits())
    );
    assert_eq!(
        world.get(second, position)?.map(|value| value.x.to_bits()),
        Some(5.0_f32.to_bits())
    );
    Ok(())
}

fn verify_tag_projection(world: &mut World, _active: TagId<Active>, first: EntityId) -> TestResult {
    let mut tagged = Vec::new();
    let mut tagged_query = world
        .query("Position, Active")?
        .project(Read::<Position>::field(0))?;
    tagged_query.each(|entity, _position| tagged.push(entity))?;
    drop(tagged_query);
    assert_eq!(tagged, vec![first]);
    Ok(())
}

fn verify_pair_projection(
    world: &mut World,
    weight: ComponentId<Weight>,
    first: EntityId,
    target_a: EntityId,
    target_b: EntityId,
) -> TestResult {
    let mut pair_writes = world
        .query("(Weight, *)")?
        .project(PairWrite::<Weight>::field(0))?;
    pair_writes.each(|_entity, mut pair| {
        pair.value_mut().value += 1;
    })?;
    drop(pair_writes);

    let mut targets = Vec::new();
    let mut pairs = world
        .query("(Weight, *)")?
        .project(PairRead::<Weight>::field(0))?;
    pairs.each(|entity, pair| {
        assert_eq!(entity, first);
        assert_eq!(pair.relation(), weight.entity());
        targets.push((pair.target(), pair.value().value));
    })?;
    targets.sort_by_key(|(_, value)| *value);
    assert_eq!(targets, vec![(target_a, 11), (target_b, 21)]);
    Ok(())
}

#[test]
fn inherited_reads_work_and_shared_writes_are_rejected() -> TestResult {
    let mut world = World::new()?;
    let position = world.register_component::<Position>()?;
    world.make_inheritable(position)?;
    let base = world.spawn()?;
    let instance = world.spawn()?;
    world.insert(base, position, Position { x: 7.0, y: 8.0 })?;
    world.inherit(instance, base)?;

    let mut seen = Vec::new();
    let mut reads = world
        .query("Position(self|up IsA)")?
        .project(Read::<Position>::field(0))?;
    reads.each(|entity, value| seen.push((entity, value.x.to_bits())))?;
    drop(reads);
    assert!(seen.contains(&(instance, 7.0_f32.to_bits())));

    let mut writes = world
        .query("Position(self|up IsA)")?
        .project(Write::<Position>::field(0))?;
    let error = writes.each(|_entity, value| {
        value.x += 1.0;
    });
    assert_eq!(
        error,
        Err(Error::Projection(ProjectionError::SharedWrite(0)))
    );
    Ok(())
}

#[test]
fn mutable_aliases_are_rejected_before_references_escape() -> TestResult {
    let mut world = World::new()?;
    let position = world.register_component::<Position>()?;
    let _velocity = world.register_component::<Velocity>()?;
    let entity = world.spawn()?;
    world.insert(entity, position, Position { x: 1.0, y: 2.0 })?;

    let mut mismatch = world
        .query("Position")?
        .project(Read::<Velocity>::field(0))?;
    assert_eq!(
        mismatch.each(|_entity, _velocity| {}),
        Err(Error::Projection(ProjectionError::ComponentMismatch(0)))
    );
    drop(mismatch);

    let mut read_only = world
        .query("[in] Position")?
        .project(Write::<Position>::field(0))?;
    assert_eq!(
        read_only.each(|_entity, _position| {}),
        Err(Error::Projection(ProjectionError::ReadOnly(0)))
    );
    drop(read_only);

    let mut query = world
        .query("Position, Position")?
        .project((Write::<Position>::field(0), Write::<Position>::field(1)))?;
    let error = query.each(|_entity, (_left, _right)| {});
    assert_eq!(
        error,
        Err(Error::Projection(ProjectionError::AliasedMutableFields(
            0, 1
        )))
    );
    Ok(())
}

#[test]
fn single_threaded_systems_can_queue_structural_commands() -> TestResult {
    let mut world = World::new()?;
    let position = world.register_component::<Position>()?;
    let original = world.spawn()?;
    world.insert(original, position, Position { x: 1.0, y: 2.0 })?;

    let phase = world.builtin_phase(BuiltinPhase::OnUpdate);
    world
        .system("Spawner", "Position")?
        .phase(phase)?
        .project(Read::<Position>::field(0))?
        .each(move |mut context, _entity, value| {
            let spawned = context.commands().spawn()?;
            context.commands().insert(
                spawned,
                position,
                Position {
                    x: value.x + 10.0,
                    y: value.y,
                },
            )?;
            Ok(())
        })?;
    let _should_continue = world.progress(TickDelta::from_seconds(0.01)?)?;

    let mut values = Vec::new();
    let mut query = world
        .query("Position")?
        .project(Read::<Position>::field(0))?;
    query.each(|_entity, position| values.push(position.x.to_bits()))?;
    values.sort_unstable();
    assert_eq!(values, vec![1.0_f32.to_bits(), 11.0_f32.to_bits()]);
    Ok(())
}

#[test]
fn systems_use_fixed_delta_and_custom_phase_order() -> TestResult {
    let mut world = World::new()?;
    let position = world.register_component::<Position>()?;
    let velocity = world.register_component::<Velocity>()?;
    let entity = world.spawn()?;
    world.insert(entity, position, Position { x: 0.0, y: 0.0 })?;
    world.insert(entity, velocity, Velocity { x: 2.0, y: 0.0 })?;

    let phase_a = world.create_phase(
        "IntegratePhase",
        Some(world.builtin_phase(BuiltinPhase::OnUpdate)),
    )?;
    let phase_b = world.create_phase("ObservePhase", Some(phase_a))?;
    let order = register_ordered_systems(&mut world, phase_a, phase_b)?;

    let delta = TickDelta::from_seconds(0.25)?;
    assert!(world.progress(delta)?);
    assert_eq!(
        world.get(entity, position)?.map(|value| value.x.to_bits()),
        Some(0.5_f32.to_bits())
    );
    let observed = order
        .lock()
        .map_err(|_error| io::Error::other("order lock poisoned"))?;
    assert_eq!(observed.as_slice(), ["integrate", "observe"]);
    drop(observed);

    verify_custom_pipeline(&mut world, entity, position, delta)
}

fn register_ordered_systems(
    world: &mut World,
    phase_a: PhaseId,
    phase_b: PhaseId,
) -> Result<Arc<Mutex<Vec<&'static str>>>, Box<dyn StdError>> {
    let order = Arc::new(Mutex::new(Vec::new()));
    let integrate_order = Arc::clone(&order);
    world
        .system("Integrate", "Position, Velocity")?
        .phase(phase_a)?
        .project((Write::<Position>::field(0), Read::<Velocity>::field(1)))?
        .each(move |context, _entity, (position, velocity)| {
            position.x += velocity.x * context.delta().as_seconds();
            let mut order = integrate_order
                .lock()
                .map_err(|_error| io::Error::other("order lock poisoned"))?;
            order.push("integrate");
            Ok(())
        })?;

    let observe_order = Arc::clone(&order);
    world
        .system("Observe", "Position")?
        .phase(phase_b)?
        .project(Read::<Position>::field(0))?
        .each(move |_context, _entity, _position| {
            let mut order = observe_order
                .lock()
                .map_err(|_error| io::Error::other("order lock poisoned"))?;
            order.push("observe");
            Ok(())
        })?;

    Ok(order)
}

fn verify_custom_pipeline(
    world: &mut World,
    entity: EntityId,
    position: ComponentId<Position>,
    delta: TickDelta,
) -> TestResult {
    let pipeline = world
        .pipeline("SimulationPipeline", "flecs.system.System")?
        .build()?;
    world.run_pipeline(pipeline, delta)?;
    assert_eq!(
        world.get(entity, position)?.map(|value| value.x.to_bits()),
        Some(1.0_f32.to_bits())
    );
    world.set_pipeline(pipeline)?;
    assert!(world.progress(delta)?);
    assert_eq!(
        world.get(entity, position)?.map(|value| value.x.to_bits()),
        Some(1.5_f32.to_bits())
    );
    Ok(())
}

#[test]
fn parallel_systems_are_deterministic_across_worker_counts() -> TestResult {
    let single = run_parallel_workload(NonZeroU32::MIN)?;
    let four = NonZeroU32::new(4).ok_or_else(|| io::Error::other("four is nonzero"))?;
    let parallel = run_parallel_workload(four)?;
    assert_eq!(single, parallel);
    Ok(())
}

fn run_parallel_workload(workers: NonZeroU32) -> Result<Vec<u32>, Box<dyn StdError>> {
    let mut world = WorldBuilder::new().worker_threads(workers).build()?;
    let position = world.register_component::<Position>()?;
    let velocity = world.register_component::<Velocity>()?;
    for index in 0_u16..256 {
        let entity = world.spawn()?;
        world.insert(
            entity,
            position,
            Position {
                x: f32::from(index),
                y: 0.0,
            },
        )?;
        world.insert(entity, velocity, Velocity { x: 2.0, y: 0.0 })?;
    }

    let phase = world.builtin_phase(BuiltinPhase::OnUpdate);
    world
        .system("ParallelIntegrate", "Position, Velocity")?
        .phase(phase)?
        .project((Write::<Position>::field(0), Read::<Velocity>::field(1)))?
        .parallel_each(|_context, _entity, (position, velocity)| {
            position.x += velocity.x;
            Ok(())
        })?;
    let _should_continue = world.progress(TickDelta::from_seconds(1.0 / 60.0)?)?;

    let mut values = Vec::new();
    let mut query = world
        .query("Position")?
        .project(Read::<Position>::field(0))?;
    query.each(|_entity, position| values.push(position.x.to_bits()))?;
    values.sort_unstable();
    Ok(values)
}

#[test]
fn callback_errors_and_context_drop_are_contained() -> TestResult {
    let dropped = Arc::new(AtomicBool::new(false));
    {
        let mut world = World::new()?;
        let position = world.register_component::<Position>()?;
        let entity = world.spawn()?;
        world.insert(entity, position, Position { x: 0.0, y: 0.0 })?;
        for _index in 0..2 {
            let other = world.spawn()?;
            world.insert(other, position, Position { x: 0.0, y: 0.0 })?;
        }
        let marker = DropMarker(Arc::clone(&dropped));
        let callbacks = Arc::new(AtomicUsize::new(0));
        let callback_count = Arc::clone(&callbacks);
        let update_phase = world.builtin_phase(BuiltinPhase::OnUpdate);
        world
            .system("Failure", "Position")?
            .phase(update_phase)?
            .project(Write::<Position>::field(0))?
            .each(move |_context, _entity, position| {
                let _keep_context_alive = &marker;
                callback_count.fetch_add(1, Ordering::Relaxed);
                position.x = 1.0;
                Err(io::Error::other("expected failure").into())
            })?;

        let Err(error) = world.progress(TickDelta::from_seconds(0.01)?) else {
            return Err(io::Error::other("the callback must fail").into());
        };
        assert_eq!(error.system(), Some("Failure"));
        assert!(
            error
                .message()
                .is_some_and(|value| value.contains("expected failure"))
        );
        assert_eq!(
            world.get(entity, position)?.map(|value| value.x.to_bits()),
            Some(1.0_f32.to_bits())
        );
        assert_eq!(callbacks.load(Ordering::Relaxed), 1);
    }
    assert!(dropped.load(Ordering::Acquire));
    Ok(())
}

#[test]
#[allow(
    clippy::panic,
    reason = "the FFI trampoline must be tested with an intentional Rust panic"
)]
fn callback_panics_are_contained() -> TestResult {
    let mut panic_world = World::new()?;
    let position = panic_world.register_component::<Position>()?;
    let entity = panic_world.spawn()?;
    panic_world.insert(entity, position, Position { x: 0.0, y: 0.0 })?;
    let update_phase = panic_world.builtin_phase(BuiltinPhase::OnUpdate);
    panic_world
        .system("RustPanicSystem", "Position")?
        .phase(update_phase)?
        .project(Write::<Position>::field(0))?
        .each(|_context, _entity, _position| -> SystemResult {
            std::panic::panic_any("caught panic")
        })?;
    let Err(panic_error) = panic_world.progress(TickDelta::from_seconds(0.01)?) else {
        return Err(io::Error::other("the callback panic must be reported").into());
    };
    assert_eq!(panic_error.system(), Some("RustPanicSystem"));
    assert!(
        panic_error
            .message()
            .is_some_and(|value| value.contains("caught panic"))
    );
    Ok(())
}

#[test]
fn callback_context_drop_panics_are_contained() -> TestResult {
    let mut drop_world = World::new()?;
    let position = drop_world.register_component::<Position>()?;
    let entity = drop_world.spawn()?;
    drop_world.insert(entity, position, Position { x: 0.0, y: 0.0 })?;
    let marker = PanickingDrop;
    drop_world
        .system("PanickingDropSystem", "Position")?
        .project(Read::<Position>::field(0))?
        .each(move |_context, _entity, _position| {
            let _keep_context_alive = &marker;
            Ok(())
        })?;
    drop(drop_world);
    Ok(())
}
