use std::collections::BTreeSet;
use std::error::Error as StdError;
use std::num::NonZeroUsize;

use blackflower_replication::{
    AoiError, BaselineError, BaselineTracker, DeltaError, Position, PositionQuantizer,
    QuantizationError, QuantizedScalar, ReplicatedEntityId, ReplicationSource, ScalarQuantizer,
    Snapshot, SnapshotError, SnapshotTick, SourceEntity, SphericalAoi,
};

type TestResult = Result<(), Box<dyn StdError>>;

fn entity(value: u64) -> ReplicatedEntityId {
    ReplicatedEntityId::new(value)
}

fn tick(value: u64) -> SnapshotTick {
    SnapshotTick::new(value)
}

#[test]
fn area_of_interest_projects_stable_client_state() -> TestResult {
    let source = ReplicationSource::new(
        tick(8),
        [
            SourceEntity::new(entity(3), Position::new(50.0, 0.0, 0.0)?, 30_u32),
            SourceEntity::new(entity(1), Position::new(3.0, 4.0, 0.0)?, 10_u32),
            SourceEntity::new(entity(2), Position::new(11.0, 0.0, 0.0)?, 20_u32),
        ],
    )?;
    let area = SphericalAoi::new(Position::new(0.0, 0.0, 0.0)?, 10.0)?;
    let always_relevant = BTreeSet::from([entity(3)]);

    let snapshot = area.project(&source, &always_relevant);
    assert_eq!(snapshot.tick(), tick(8));
    assert_eq!(
        snapshot
            .entities()
            .map(|(entity, _state)| entity)
            .collect::<Vec<_>>(),
        [entity(1), entity(3)]
    );
    assert_eq!(snapshot.get(entity(1)), Some(&10));
    assert_eq!(snapshot.get(entity(2)), None);
    Ok(())
}

#[test]
fn area_of_interest_rejects_invalid_geometry_and_duplicate_ids() -> TestResult {
    assert_eq!(
        Position::new(f64::NAN, 0.0, 0.0),
        Err(AoiError::NonFinitePosition)
    );
    let center = Position::new(0.0, 0.0, 0.0)?;
    assert!(matches!(
        SphericalAoi::new(center, -1.0),
        Err(AoiError::InvalidRadius { .. })
    ));
    let duplicate = ReplicationSource::new(
        tick(1),
        [
            SourceEntity::new(entity(7), center, 1_u8),
            SourceEntity::new(entity(7), center, 2_u8),
        ],
    );
    assert_eq!(
        duplicate,
        Err(AoiError::DuplicateEntity { entity: entity(7) })
    );
    Ok(())
}

#[test]
fn scalar_and_position_quantization_are_explicit() -> TestResult {
    let scalar = ScalarQuantizer::new(-1.0, 1.0, 8)?;
    assert_eq!(scalar.bits(), 8);
    assert_eq!(scalar.quantize(-1.0)?, QuantizedScalar::new(0));
    assert_eq!(scalar.quantize(1.0)?, QuantizedScalar::new(255));
    assert_eq!(scalar.quantize_clamped(2.0)?, QuantizedScalar::new(255));
    assert!(matches!(
        scalar.quantize(2.0),
        Err(QuantizationError::ValueOutOfRange { .. })
    ));

    let position = PositionQuantizer::new([
        ScalarQuantizer::new(-100.0, 100.0, 16)?,
        ScalarQuantizer::new(-10.0, 10.0, 12)?,
        ScalarQuantizer::new(0.0, 50.0, 10)?,
    ]);
    let quantized = position.quantize([12.5, -2.0, 25.0])?;
    let reconstructed = position.dequantize(quantized)?;
    let ranges = [200.0 / 65_535.0, 20.0 / 4_095.0, 50.0 / 1_023.0];
    for (actual, (expected, resolution)) in reconstructed
        .into_iter()
        .zip([12.5, -2.0, 25.0].into_iter().zip(ranges))
    {
        assert!((actual - expected).abs() <= resolution / 2.0);
    }
    Ok(())
}

#[test]
fn projected_snapshots_transform_into_quantized_protocol_state() -> TestResult {
    let source = ReplicationSource::new(
        tick(8),
        [
            SourceEntity::new(entity(1), Position::new(5.0, 0.0, 0.0)?, [5.0, 0.0, 0.0]),
            SourceEntity::new(entity(2), Position::new(50.0, 0.0, 0.0)?, [50.0, 0.0, 0.0]),
        ],
    )?;
    let area = SphericalAoi::new(Position::new(0.0, 0.0, 0.0)?, 10.0)?;
    let projected = area.project(&source, &BTreeSet::new());
    let axis = ScalarQuantizer::new(-100.0, 100.0, 16)?;
    let quantizer = PositionQuantizer::new([axis; 3]);
    let quantized = projected.try_map(|_entity, position| quantizer.quantize(*position))?;

    assert_eq!(quantized.tick(), tick(8));
    assert_eq!(quantized.len(), 1);
    assert!(quantized.get(entity(1)).is_some());
    assert_eq!(quantized.get(entity(2)), None);
    Ok(())
}

#[test]
fn quantization_rejects_invalid_policies_values_and_codes() -> TestResult {
    assert!(matches!(
        ScalarQuantizer::new(1.0, 1.0, 8),
        Err(QuantizationError::InvalidRange { .. })
    ));
    assert_eq!(
        ScalarQuantizer::new(0.0, 1.0, 0),
        Err(QuantizationError::InvalidBitCount { bits: 0 })
    );
    let scalar = ScalarQuantizer::new(0.0, 1.0, 4)?;
    assert!(matches!(
        scalar.quantize(f64::INFINITY),
        Err(QuantizationError::NonFiniteValue { .. })
    ));
    assert_eq!(
        scalar.dequantize(QuantizedScalar::new(16)),
        Err(QuantizationError::CodeOutOfRange {
            code: 16,
            maximum: 15,
        })
    );
    Ok(())
}

#[test]
fn delta_tracks_aoi_entries_changes_and_departures() -> TestResult {
    let baseline = Snapshot::new(
        tick(8),
        [
            (entity(1), QuantizedScalar::new(10)),
            (entity(2), QuantizedScalar::new(20)),
        ],
    )?;
    let current = Snapshot::new(
        tick(16),
        [
            (entity(2), QuantizedScalar::new(21)),
            (entity(3), QuantizedScalar::new(30)),
        ],
    )?;

    let delta = blackflower_replication::SnapshotDelta::between(&current, Some(&baseline))?;
    assert_eq!(delta.baseline(), Some(tick(8)));
    assert_eq!(delta.removed(), [entity(1)]);
    assert_eq!(
        delta
            .updates()
            .iter()
            .map(|update| (update.entity(), update.state().code()))
            .collect::<Vec<_>>(),
        [(entity(2), 21), (entity(3), 30)]
    );
    assert_eq!(delta.apply(Some(&baseline))?, current);
    Ok(())
}

#[test]
fn full_delta_requires_no_baseline() -> TestResult {
    let current = Snapshot::new(tick(8), [(entity(1), 10_u8)])?;
    let delta = blackflower_replication::SnapshotDelta::between(&current, None)?;
    assert_eq!(delta.baseline(), None);
    assert_eq!(delta.apply(None)?, current);
    assert_eq!(
        delta.apply(Some(&current)),
        Err(DeltaError::UnexpectedBaseline { actual: tick(8) })
    );
    Ok(())
}

#[test]
fn baseline_tracker_uses_only_acknowledged_snapshots() -> TestResult {
    let mut tracker = BaselineTracker::new(NonZeroUsize::new(3).ok_or("invalid test bound")?);
    let first = Snapshot::new(tick(8), [(entity(1), 10_u8)])?;
    let second = Snapshot::new(tick(16), [(entity(1), 20_u8)])?;
    let third = Snapshot::new(tick(24), [(entity(1), 30_u8)])?;

    assert_eq!(tracker.build_delta(&first)?.baseline(), None);
    assert_eq!(tracker.record_sent(first.clone())?, None);
    tracker.acknowledge(tick(8))?;
    assert_eq!(tracker.acknowledged_tick(), Some(tick(8)));

    assert_eq!(tracker.record_sent(second)?, None);
    assert_eq!(tracker.build_delta(&third)?.baseline(), Some(tick(8)));
    assert_eq!(tracker.record_sent(third.clone())?, None);
    tracker.acknowledge(tick(24))?;
    assert_eq!(tracker.baseline(), Some(&third));
    assert_eq!(tracker.pending_len(), 0);
    Ok(())
}

#[test]
fn bounded_pending_history_reports_eviction_and_unknown_acknowledgement() -> TestResult {
    let mut tracker = BaselineTracker::new(NonZeroUsize::MIN);
    let first = Snapshot::new(tick(8), [(entity(1), 10_u8)])?;
    let second = Snapshot::new(tick(16), [(entity(1), 20_u8)])?;

    assert_eq!(tracker.record_sent(first)?, None);
    assert_eq!(tracker.record_sent(second)?, Some(tick(8)));
    assert_eq!(
        tracker.acknowledge(tick(8)),
        Err(BaselineError::UnknownAcknowledgement { tick: tick(8) })
    );
    tracker.acknowledge(tick(16))?;
    assert_eq!(
        tracker.record_sent(Snapshot::new(tick(16), [(entity(1), 20_u8)])?),
        Err(BaselineError::NonIncreasingSnapshot {
            tick: tick(16),
            latest: tick(16),
        })
    );
    Ok(())
}

#[test]
fn snapshots_reject_duplicate_entities_and_regressing_baselines() -> TestResult {
    assert_eq!(
        Snapshot::new(tick(8), [(entity(1), 10_u8), (entity(1), 11_u8)]),
        Err(SnapshotError::DuplicateEntity { entity: entity(1) })
    );
    let current = Snapshot::new(tick(8), [(entity(1), 10_u8)])?;
    let baseline = Snapshot::new(tick(8), [(entity(1), 9_u8)])?;
    assert_eq!(
        blackflower_replication::SnapshotDelta::between(&current, Some(&baseline)),
        Err(DeltaError::BaselineNotOlder {
            baseline: tick(8),
            current: tick(8),
        })
    );
    Ok(())
}
