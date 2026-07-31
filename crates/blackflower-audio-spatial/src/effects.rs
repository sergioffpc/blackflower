use std::collections::BTreeSet;
use std::sync::atomic::{AtomicI32, AtomicU16, AtomicU64, AtomicUsize, Ordering};

use blackflower_acoustics::{AcousticStructureVersion, BandEnergy, PropagationDescriptor};

use crate::{Error, Scene};

/// Allocation-free authoritative broadband gain applied before HRTF.
pub struct DirectEffect {
    frame_len: usize,
}

impl DirectEffect {
    /// Create one fixed-frame effect.
    pub fn new(frame_len: usize) -> Result<Self, Error> {
        if frame_len == 0 {
            Err(Error::InvalidEffectFrame)
        } else {
            Ok(Self { frame_len })
        }
    }

    /// Apply the server-provided direct gain without querying client geometry.
    #[allow(
        clippy::cast_precision_loss,
        reason = "authoritative acoustic gain is bounded Q8 decibels and intentionally processed as f32 audio"
    )]
    pub fn process(
        &mut self,
        propagation: PropagationDescriptor,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<(), Error> {
        validate_frame(self.frame_len, input, output)?;
        let gain = 10.0_f32.powf(propagation.gain_db_q8 as f32 / (20.0 * 256.0));
        for (output, input) in output.iter_mut().zip(input) {
            *output = *input * gain;
        }
        Ok(())
    }
}

/// Allocation-free three-band path coloration applied before HRTF.
pub struct PathEffect {
    frame_len: usize,
    slow: f32,
    fast: f32,
}

impl PathEffect {
    /// Create one fixed-frame effect with preallocated filter state.
    pub fn new(frame_len: usize) -> Result<Self, Error> {
        if frame_len == 0 {
            Err(Error::InvalidEffectFrame)
        } else {
            Ok(Self {
                frame_len,
                slow: 0.0,
                fast: 0.0,
            })
        }
    }

    /// Apply the authoritative low/mid/high path response in-place.
    pub fn process(
        &mut self,
        propagation: PropagationDescriptor,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<(), Error> {
        validate_frame(self.frame_len, input, output)?;
        let gains = propagation
            .band_gain
            .0
            .map(|gain| f32::from(gain) / f32::from(u16::MAX));
        for (output, input) in output.iter_mut().zip(input) {
            self.slow += 0.04 * (*input - self.slow);
            self.fast += 0.35 * (*input - self.fast);
            let low = self.slow;
            let mid = self.fast - self.slow;
            let high = *input - self.fast;
            *output = low * gains[0] + mid * gains[1] + high * gains[2];
        }
        Ok(())
    }

    /// Clear filter history when a voice is recycled.
    pub fn reset(&mut self) {
        self.slow = 0.0;
        self.fast = 0.0;
    }
}

fn validate_frame(frame_len: usize, input: &[f32], output: &[f32]) -> Result<(), Error> {
    if input.len() != frame_len {
        Err(Error::FrameLength {
            buffer: "environmental input",
            expected: frame_len,
            actual: input.len(),
        })
    } else if output.len() != frame_len {
        Err(Error::FrameLength {
            buffer: "environmental output",
            expected: frame_len,
            actual: output.len(),
        })
    } else {
        Ok(())
    }
}

struct AtomicPropagation {
    generation: AtomicU64,
    structure: AtomicU64,
    arrival: AtomicU64,
    path: AtomicU64,
    gain: AtomicI32,
    bands: [AtomicU16; 3],
    directions: [AtomicU16; 3],
    uncertainty: AtomicU16,
    direct: AtomicU16,
}

impl AtomicPropagation {
    fn new(value: PropagationDescriptor) -> Self {
        Self {
            generation: AtomicU64::new(0),
            structure: AtomicU64::new(value.structure_version.0),
            arrival: AtomicU64::new(value.arrival_sample),
            path: AtomicU64::new(value.path_length_mm),
            gain: AtomicI32::new(value.gain_db_q8),
            bands: value.band_gain.0.map(AtomicU16::new),
            directions: value
                .direction_q15
                .map(|value| AtomicU16::new(value.cast_unsigned())),
            uncertainty: AtomicU16::new(value.uncertainty_q16),
            direct: AtomicU16::new(u16::from(value.direct)),
        }
    }

    fn store(&self, value: PropagationDescriptor) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.structure
            .store(value.structure_version.0, Ordering::Relaxed);
        self.arrival.store(value.arrival_sample, Ordering::Relaxed);
        self.path.store(value.path_length_mm, Ordering::Relaxed);
        self.gain.store(value.gain_db_q8, Ordering::Relaxed);
        for (target, value) in self.bands.iter().zip(value.band_gain.0) {
            target.store(value, Ordering::Relaxed);
        }
        for (target, value) in self.directions.iter().zip(value.direction_q15) {
            target.store(value.cast_unsigned(), Ordering::Relaxed);
        }
        self.uncertainty
            .store(value.uncertainty_q16, Ordering::Relaxed);
        self.direct
            .store(u16::from(value.direct), Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    fn load(&self) -> Option<PropagationDescriptor> {
        let before = self.generation.load(Ordering::Acquire);
        if !before.is_multiple_of(2) {
            return None;
        }
        let value = PropagationDescriptor {
            structure_version: AcousticStructureVersion(self.structure.load(Ordering::Relaxed)),
            arrival_sample: self.arrival.load(Ordering::Relaxed),
            path_length_mm: self.path.load(Ordering::Relaxed),
            gain_db_q8: self.gain.load(Ordering::Relaxed),
            band_gain: BandEnergy(
                self.bands
                    .each_ref()
                    .map(|value| value.load(Ordering::Relaxed)),
            ),
            direction_q15: self
                .directions
                .each_ref()
                .map(|value| value.load(Ordering::Relaxed).cast_signed()),
            uncertainty_q16: self.uncertainty.load(Ordering::Relaxed),
            direct: self.direct.load(Ordering::Relaxed) != 0,
        };
        (before == self.generation.load(Ordering::Acquire)).then_some(value)
    }
}

/// Preallocated lock-free triple buffer for callback-visible propagation parameters.
pub struct PropagationExchange {
    slots: [AtomicPropagation; 3],
    published: AtomicUsize,
    next: AtomicUsize,
}

impl PropagationExchange {
    /// Initialize every slot to the same safe descriptor.
    #[must_use]
    pub fn new(initial: PropagationDescriptor) -> Self {
        Self {
            slots: core::array::from_fn(|_index| AtomicPropagation::new(initial)),
            published: AtomicUsize::new(0),
            next: AtomicUsize::new(1),
        }
    }

    /// Publish from a worker/control thread without allocating.
    pub fn publish(&self, value: PropagationDescriptor) {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.slots.len();
        self.slots[index].store(value);
        self.published.store(index, Ordering::Release);
    }

    /// Read the latest coherent descriptor without locks or allocation.
    #[must_use]
    pub fn latest(&self) -> PropagationDescriptor {
        loop {
            let index = self.published.load(Ordering::Acquire);
            if let Some(value) = self.slots[index].load() {
                return value;
            }
            core::hint::spin_loop();
        }
    }
}

/// One coalesced reflection update produced outside the audio callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflectionUpdate {
    /// Only zones adjacent to changed portals or instances.
    pub dirty_zones: Vec<u32>,
    /// Crossfade duration used when publishing the new late response.
    pub crossfade_ms: u16,
}

/// Presentation-only dirty-zone collector and single-commit coordinator.
pub struct ReflectionSimulator {
    dirty_zones: BTreeSet<u32>,
    max_dirty_zones: usize,
    crossfade_ms: u16,
}

impl ReflectionSimulator {
    /// Create a bounded off-callback reflection coordinator.
    pub fn new(max_dirty_zones: usize, crossfade_ms: u16) -> Result<Self, Error> {
        if max_dirty_zones == 0 || crossfade_ms == 0 {
            return Err(Error::InvalidReflectionSettings);
        }
        Ok(Self {
            dirty_zones: BTreeSet::new(),
            max_dirty_zones,
            crossfade_ms,
        })
    }

    /// Mark exactly the zones touched by one committed structure change.
    pub fn mark_dirty(&mut self, zones: impl IntoIterator<Item = u32>) {
        for zone in zones {
            if self.dirty_zones.len() >= self.max_dirty_zones && !self.dirty_zones.contains(&zone) {
                break;
            }
            self.dirty_zones.insert(zone);
        }
    }

    /// Perform at most one Steam Audio scene commit and return the coalesced update.
    pub fn commit(&mut self, scene: &mut Scene) -> Option<ReflectionUpdate> {
        if self.dirty_zones.is_empty() {
            return None;
        }
        scene.commit();
        Some(ReflectionUpdate {
            dirty_zones: core::mem::take(&mut self.dirty_zones).into_iter().collect(),
            crossfade_ms: self.crossfade_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> PropagationDescriptor {
        PropagationDescriptor {
            structure_version: AcousticStructureVersion(2),
            arrival_sample: 480,
            path_length_mm: 3_430,
            gain_db_q8: -6 * 256,
            band_gain: BandEnergy([u16::MAX, 40_000, 20_000]),
            direction_q15: [1, 2, 3],
            uncertainty_q16: 4,
            direct: true,
        }
    }

    #[test]
    fn effects_and_exchange_need_no_per_frame_storage() -> Result<(), Error> {
        let propagation = descriptor();
        let exchange = PropagationExchange::new(propagation);
        exchange.publish(propagation);
        assert_eq!(exchange.latest(), propagation);
        let input = [0.25_f32; 8];
        let mut scratch = [0.0_f32; 8];
        let mut output = [0.0_f32; 8];
        DirectEffect::new(8)?.process(propagation, &input, &mut scratch)?;
        PathEffect::new(8)?.process(propagation, &scratch, &mut output)?;
        assert!(output.iter().any(|sample| *sample != 0.0));
        Ok(())
    }
}
