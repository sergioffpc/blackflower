use std::collections::BTreeSet;
use std::time::Duration;

use blackflower_networking::{ProtocolRevision, SimulationTick, SnapshotAppliedAck};
use blackflower_networking_replication::{
    AoiTracker, BaselineError, BaselineTracker, ComponentDescriptor, ComponentId,
    ComponentRegistry, ComponentSampleTick, ComponentState, DeltaOperation, EntityIdAllocator,
    EntityState, Position, ProjectionBundle, ProjectionKind, ProjectionView, QuantizedAngle,
    QuantizedPosition, QuantizedQuaternion, QuantizedVelocity, ReplicatedEntityId,
    ReplicationPriority, ReplicationSource, Snapshot, SnapshotBuilder, SnapshotDelta,
    SnapshotReassembler, SnapshotTick, SourceEntity, build_snapshot_chunks,
};
use bytes::{Bytes, BytesMut};
use glam::{Quat, Vec3};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn entity(value: u64) -> Result<ReplicatedEntityId, Box<dyn std::error::Error>> {
    Ok(ReplicatedEntityId::try_from_u64(value)?)
}

fn component(value: u16) -> Result<ComponentId, Box<dyn std::error::Error>> {
    Ok(ComponentId::try_from_u16(value)?)
}

fn state(
    tick: u64,
    priority: ReplicationPriority,
    bytes: &[u8],
) -> Result<ComponentState, Box<dyn std::error::Error>> {
    Ok(ComponentState::new(
        ComponentSampleTick::new(tick),
        priority,
        Bytes::copy_from_slice(bytes),
    )?)
}

fn entity_state(
    components: impl IntoIterator<Item = (ComponentId, ComponentState)>,
) -> Result<EntityState, Box<dyn std::error::Error>> {
    Ok(EntityState::new(components)?)
}

fn assert_priorities_sorted(delta: &SnapshotDelta) {
    assert!(
        delta
            .operations()
            .windows(2)
            .all(|pair| pair[0].priority() <= pair[1].priority())
    );
}

#[test]
fn identities_are_non_zero_monotonic_and_non_reusing() -> TestResult {
    assert!(ReplicatedEntityId::try_from_u64(0).is_err());
    assert!(ComponentId::try_from_u16(0).is_err());
    let mut allocator = EntityIdAllocator::new();
    assert_eq!(allocator.allocate()?.get(), 1);
    assert_eq!(allocator.allocate()?.get(), 2);
    assert_eq!(allocator.allocate()?.get(), 3);
    Ok(())
}

#[test]
fn visibility_projection_precedes_serialization() -> TestResult {
    let public = component(1)?;
    let owner = component(2)?;
    let registry = ComponentRegistry::new(
        ProtocolRevision::V1,
        [
            ComponentDescriptor {
                id: public,
                projection: ProjectionKind::Public,
                maximum_bytes: 8,
            },
            ComponentDescriptor {
                id: owner,
                projection: ProjectionKind::Owner,
                maximum_bytes: 8,
            },
        ],
    )?;
    let mut bundle = ProjectionBundle::default();
    bundle.insert(
        ProjectionKind::Public,
        entity_state([(public, state(8, ReplicationPriority::ActiveActor, &[1])?)])?,
    )?;
    bundle.insert(
        ProjectionKind::Owner,
        entity_state([(owner, state(8, ReplicationPriority::OwnerCorrection, &[2])?)])?,
    )?;

    let observer = bundle.project(
        ProjectionView {
            protocol_revision: ProtocolRevision::V1,
            owner: false,
            same_team: false,
            include_global: false,
        },
        &registry,
    )?;
    assert!(observer.get(public).is_some());
    assert!(observer.get(owner).is_none());

    let controlling_player = bundle.project(
        ProjectionView {
            protocol_revision: ProtocolRevision::V1,
            owner: true,
            same_team: false,
            include_global: false,
        },
        &registry,
    )?;
    assert!(controlling_player.get(public).is_some());
    assert!(controlling_player.get(owner).is_some());
    Ok(())
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the test follows one AOI entity through entry, hysteresis, exit, and re-entry"
)]
fn stateful_aoi_uses_fixed_entry_hysteresis_and_same_id_reentry() -> TestResult {
    let id = entity(7)?;
    let component = component(1)?;
    let projected = entity_state([(component, state(1, ReplicationPriority::ActiveActor, &[7])?)])?;
    let center = Position::new(0.0, 0.0, 0.0)?;
    let mut aoi = AoiTracker::new(center, 0.0)?;

    let entered = ReplicationSource::new(
        SnapshotTick::new(1),
        [SourceEntity::new(
            id,
            Position::new(500.0, 0.0, 0.0)?,
            projected.clone(),
        )],
    )?;
    let first = aoi.project(&entered, &BTreeSet::new());
    assert!(first.get(id).is_some());

    let hysteresis = ReplicationSource::new(
        SnapshotTick::new(2),
        [SourceEntity::new(
            id,
            Position::new(520.0, 0.0, 0.0)?,
            projected.clone(),
        )],
    )?;
    assert!(aoi.project(&hysteresis, &BTreeSet::new()).get(id).is_some());

    let left = ReplicationSource::new(
        SnapshotTick::new(3),
        [SourceEntity::new(
            id,
            Position::new(529.0, 0.0, 0.0)?,
            projected.clone(),
        )],
    )?;
    let forgotten = aoi.project(&left, &BTreeSet::new());
    assert!(forgotten.get(id).is_none());
    assert!(matches!(
        SnapshotDelta::between(&forgotten, Some(&first))?.operations(),
        [DeltaOperation::Forget { entity }] if *entity == id
    ));

    let reentered = ReplicationSource::new(
        SnapshotTick::new(4),
        [SourceEntity::new(
            id,
            Position::new(500.0, 0.0, 0.0)?,
            projected,
        )],
    )?;
    let reentry = aoi.project(&reentered, &BTreeSet::new());
    assert!(matches!(
        SnapshotDelta::between(&reentry, Some(&forgotten))?.operations(),
        [DeltaOperation::Spawn { entity, .. }] if *entity == id
    ));
    Ok(())
}

#[test]
fn indexed_aoi_preserves_order_and_includes_distant_global_entities() -> TestResult {
    let mut entities = (3_u64..=1_024)
        .map(|id| {
            Ok(SourceEntity::new(
                entity(id)?,
                Position::new(f32::from(u16::try_from(id)?) * 2_048.0, 0.0, 0.0)?,
                EntityState::default(),
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    entities.push(SourceEntity::new(
        entity(2)?,
        Position::new(-10.0, 0.0, 0.0)?,
        EntityState::default(),
    ));
    entities.push(SourceEntity::new(
        entity(1)?,
        Position::new(10.0, 0.0, 0.0)?,
        EntityState::default(),
    ));
    let source = ReplicationSource::new(SnapshotTick::new(1), entities)?;
    let always_relevant = BTreeSet::from([entity(1_024)?]);
    let mut aoi = AoiTracker::new(Position::new(0.0, 0.0, 0.0)?, 0.0)?;

    let projected = aoi.project(&source, &always_relevant);
    assert_eq!(
        projected
            .entities()
            .map(|(entity, _state)| entity.get())
            .collect::<Vec<_>>(),
        [1, 2, 1_024]
    );
    Ok(())
}

#[test]
fn normative_quantizers_round_trip_with_bounded_error() -> TestResult {
    let position = QuantizedPosition::quantize(Vec3::new(12.345, -6.789, 0.001))?;
    assert_eq!(position.codes(), [1_235, -679, 0]);
    let position_round_trip = position.dequantize();
    assert!((position_round_trip[0] - 12.35).abs() < 0.000_001);

    let velocity = QuantizedVelocity::quantize(Vec3::new(3.25, -2.5, 0.0))?;
    assert_eq!(velocity.codes(), [325, -250, 0]);
    assert!(
        velocity
            .dequantize()
            .abs_diff_eq(Vec3::new(3.25, -2.5, 0.0), 0.000_001)
    );

    let angle = QuantizedAngle::quantize(std::f32::consts::PI)?;
    assert_eq!(angle.code(), 32_768);
    assert!((angle.dequantize() - std::f32::consts::PI).abs() < 0.000_001);

    let quaternion = QuantizedQuaternion::quantize(Quat::IDENTITY)?;
    assert_eq!(quaternion.largest_index(), 3);
    assert_eq!(quaternion.components(), [0, 0, 0]);
    assert!(
        quaternion
            .dequantize()?
            .abs_diff_eq(Quat::IDENTITY, 0.000_001)
    );
    Ok(())
}

#[test]
fn component_delta_is_atomic_and_reconstructs_projection() -> TestResult {
    let first = component(1)?;
    let second = component(2)?;
    let first_entity = entity(1)?;
    let second_entity = entity(2)?;
    let third_entity = entity(3)?;
    let baseline = Snapshot::new(
        SnapshotTick::new(8),
        [
            (
                first_entity,
                entity_state([
                    (first, state(8, ReplicationPriority::ActiveActor, &[10])?),
                    (second, state(8, ReplicationPriority::Remaining, &[20])?),
                ])?,
            ),
            (
                third_entity,
                entity_state([(first, state(8, ReplicationPriority::Remaining, &[30])?)])?,
            ),
        ],
    )?;
    let current = Snapshot::new(
        SnapshotTick::new(16),
        [
            (
                first_entity,
                entity_state([(first, state(16, ReplicationPriority::ActiveActor, &[11])?)])?,
            ),
            (
                second_entity,
                entity_state([(first, state(16, ReplicationPriority::Lifecycle, &[40])?)])?,
            ),
        ],
    )?;

    let delta = SnapshotDelta::between(&current, Some(&baseline))?;
    assert_eq!(delta.baseline(), Some(SnapshotTick::new(8)));
    assert_eq!(delta.operations().len(), 4);
    assert_priorities_sorted(&delta);
    assert!(delta.operations().iter().any(|operation| matches!(
        operation,
        DeltaOperation::Update { entity: id, component, .. }
            if *id == first_entity && *component == first
    )));
    assert!(delta
        .operations()
        .iter()
        .any(|operation| matches!(operation, DeltaOperation::RemoveComponent { component, .. } if *component == second)));
    assert_eq!(delta.apply(Some(&baseline))?, current);
    Ok(())
}

#[test]
fn builder_preserves_sample_tick_for_unchanged_components() -> TestResult {
    let id = entity(1)?;
    let unchanged = component(1)?;
    let changed = component(2)?;
    let previous = Snapshot::new(
        SnapshotTick::new(8),
        [(
            id,
            entity_state([
                (unchanged, state(4, ReplicationPriority::Remaining, &[1])?),
                (changed, state(8, ReplicationPriority::ActiveActor, &[2])?),
            ])?,
        )],
    )?;
    let mut builder = SnapshotBuilder::from_previous(SnapshotTick::new(16), &previous);
    builder.update_component(
        id,
        changed,
        state(16, ReplicationPriority::ActiveActor, &[3])?,
    )?;
    let current = builder.build()?;
    let entity = current.get(id).ok_or("missing entity")?;
    assert_eq!(
        entity
            .get(unchanged)
            .ok_or("missing component")?
            .sample_tick(),
        ComponentSampleTick::new(4)
    );
    assert_eq!(
        entity
            .get(changed)
            .ok_or("missing component")?
            .sample_tick(),
        ComponentSampleTick::new(16)
    );
    Ok(())
}

#[test]
fn canonical_codec_chunks_and_exact_applied_ack_share_digest() -> TestResult {
    let id = entity(1)?;
    let component = component(1)?;
    let snapshot = Snapshot::new(
        SnapshotTick::new(8),
        [(
            id,
            entity_state([(
                component,
                state(7, ReplicationPriority::ActiveActor, &[0xaa, 0xbb])?,
            )])?,
        )],
    )?;
    let canonical = snapshot.encode()?;
    assert_eq!(Snapshot::decode(&canonical)?, snapshot);
    assert_eq!(
        canonical,
        vec![
            8, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 7, 0, 0, 0, 0,
            0, 0, 0, 3, 0, 2, 0, 0xaa, 0xbb,
        ]
    );

    let delta = SnapshotDelta::between(&snapshot, None)?;
    assert_eq!(SnapshotDelta::decode(&delta.encode()?)?, delta);
    let chunks = build_snapshot_chunks(&delta, &snapshot, ProtocolRevision::V1, 20)?;
    assert_eq!(chunks.len(), 3);
    let mut reassembler = SnapshotReassembler::new(chunks[0].clone(), Duration::ZERO)?;
    let mut reconstructed = None;
    for chunk in &chunks[1..] {
        reconstructed = reassembler.push(chunk.clone(), Duration::from_millis(10))?;
    }
    let reconstructed = reconstructed.ok_or("snapshot did not complete")?;
    assert_eq!(reconstructed, delta.encode()?);

    let mut baselines = BaselineTracker::new(ProtocolRevision::V1);
    baselines.record_sent(snapshot.clone())?;
    let digest = snapshot.digest(ProtocolRevision::V1)?;
    assert!(matches!(
        baselines.acknowledge(SnapshotAppliedAck {
            snapshot_tick: SimulationTick::new(8),
            projection_digest: blackflower_networking::ProjectionDigest::from_bytes([0; 32]),
        }),
        Err(BaselineError::DigestMismatch { .. })
    ));
    baselines.acknowledge(SnapshotAppliedAck {
        snapshot_tick: SimulationTick::new(8),
        projection_digest: digest,
    })?;
    assert_eq!(baselines.baseline(), Some(&snapshot));
    Ok(())
}

#[test]
fn late_ack_reconstructs_a_demoted_pending_snapshot() -> TestResult {
    let id = entity(1)?;
    let component = component(1)?;
    let snapshot = |tick, value| -> Result<Snapshot, Box<dyn std::error::Error>> {
        Ok(Snapshot::new(
            SnapshotTick::new(tick),
            [(
                id,
                entity_state([(
                    component,
                    state(tick, ReplicationPriority::ActiveActor, &[value])?,
                )])?,
            )],
        )?)
    };
    let first = snapshot(8, 1)?;
    let second = snapshot(16, 2)?;
    let third = snapshot(24, 3)?;
    let mut baselines = BaselineTracker::new(ProtocolRevision::V1);
    baselines.record_sent(first.clone())?;
    baselines.acknowledge(SnapshotAppliedAck {
        snapshot_tick: SimulationTick::new(8),
        projection_digest: first.digest(ProtocolRevision::V1)?,
    })?;
    baselines.record_sent_delta(
        second.clone(),
        SnapshotDelta::between(&second, Some(&first))?,
    )?;
    baselines.record_sent_delta(third.clone(), SnapshotDelta::between(&third, Some(&first))?)?;

    baselines.acknowledge(SnapshotAppliedAck {
        snapshot_tick: SimulationTick::new(16),
        projection_digest: second.digest(ProtocolRevision::V1)?,
    })?;
    assert_eq!(baselines.baseline(), Some(&second));
    baselines.acknowledge(SnapshotAppliedAck {
        snapshot_tick: SimulationTick::new(24),
        projection_digest: third.digest(ProtocolRevision::V1)?,
    })?;
    assert_eq!(baselines.baseline(), Some(&third));
    Ok(())
}

#[test]
fn delta_rejects_an_operation_count_larger_than_the_remaining_bytes() {
    let mut bytes = BytesMut::zeroed(24);
    bytes[20..24].copy_from_slice(&65_536_u32.to_le_bytes());

    assert!(SnapshotDelta::decode(&bytes).is_err());
}
