use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU32, NonZeroUsize};
use std::time::{Duration, Instant};

use std::sync::Arc;

use super::{EstablishedConnectionCapacity, RetryTokenBucket, ValidatedOriginLimiter};
use crate::AdmissionLimits;

fn limits(capacity: u32, window: Duration, pending_per_origin: usize) -> AdmissionLimits {
    AdmissionLimits {
        attempts_per_window: NonZeroU32::new(capacity).unwrap_or(NonZeroU32::MIN),
        window,
        pending_per_origin: NonZeroUsize::new(pending_per_origin).unwrap_or(NonZeroUsize::MIN),
        pending_global: NonZeroUsize::new(3).unwrap_or(NonZeroUsize::MIN),
        connections_global: NonZeroUsize::new(2).unwrap_or(NonZeroUsize::MIN),
    }
}

#[test]
fn global_retry_bucket_bounds_bursts_and_refills_continuously() {
    let start = Instant::now();
    let mut bucket = RetryTokenBucket::new(limits(4, Duration::from_secs(1), 2), start);

    for _attempt in 0..4 {
        assert!(bucket.try_take(start));
    }
    assert!(!bucket.try_take(start));
    assert!(!bucket.try_take(start + Duration::from_millis(249)));
    assert!(bucket.try_take(start + Duration::from_millis(250)));
    assert!(!bucket.try_take(start + Duration::from_millis(250)));

    let much_later = start + Duration::from_secs(10);
    for _attempt in 0..4 {
        assert!(bucket.try_take(much_later));
    }
    assert!(!bucket.try_take(much_later));
}

#[test]
fn per_origin_state_exists_only_for_validated_pending_handshakes() {
    let first = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
    let second = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1));
    let mut origins = ValidatedOriginLimiter::new(limits(4, Duration::from_secs(1), 2));

    assert!(origins.pending.is_empty());
    assert!(origins.begin_pending(first));
    assert!(origins.begin_pending(first));
    assert!(!origins.begin_pending(first));
    assert!(origins.begin_pending(second));
    assert_eq!(origins.pending.len(), 2);
    assert_eq!(origins.pending_total, 3);

    assert!(!origins.begin_pending(second));

    origins.finish_pending(first);
    origins.finish_pending(first);
    assert!(!origins.pending.contains_key(&first));
    assert_eq!(origins.pending.get(&second), Some(&1));
    assert_eq!(origins.pending_total, 1);
}

#[test]
fn established_connection_capacity_is_global_and_clone_safe() -> Result<(), &'static str> {
    let capacity = Arc::new(EstablishedConnectionCapacity::new(1));
    let permit = capacity.try_acquire().ok_or("first connection rejected")?;
    let cloned = Arc::clone(&permit);

    assert!(capacity.try_acquire().is_none());
    drop(permit);
    assert!(capacity.try_acquire().is_none());
    drop(cloned);
    assert!(capacity.try_acquire().is_some());
    Ok(())
}
