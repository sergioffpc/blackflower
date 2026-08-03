use crate::{Error, SamplingRatio};

/// Named point on a normalized animation timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationMarker {
    name: String,
    ratio: SamplingRatio,
}

impl AnimationMarker {
    /// Construct a marker at a validated normalized time.
    #[must_use]
    pub fn new(name: impl Into<String>, ratio: SamplingRatio) -> Self {
        Self {
            name: name.into(),
            ratio,
        }
    }

    /// Return the marker name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the normalized marker time.
    #[must_use]
    pub const fn ratio(&self) -> SamplingRatio {
        self.ratio
    }
}

/// Immutable, deterministically ordered markers for one animation timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerTrack {
    markers: Box<[AnimationMarker]>,
}

impl MarkerTrack {
    /// Construct a track ordered by non-decreasing normalized time.
    pub fn new(markers: impl IntoIterator<Item = AnimationMarker>) -> Result<Self, Error> {
        let markers = markers.into_iter().collect::<Vec<_>>();
        if markers
            .windows(2)
            .any(|pair| pair[0].ratio.get() > pair[1].ratio.get())
        {
            return Err(Error::InvalidMarkerOrder);
        }
        Ok(Self {
            markers: markers.into_boxed_slice(),
        })
    }

    /// Return all markers in deterministic timeline order.
    #[must_use]
    pub fn markers(&self) -> &[AnimationMarker] {
        &self.markers
    }

    /// Return markers crossed while moving from `previous` to `current`.
    ///
    /// The starting point is exclusive and the ending point is inclusive.
    /// `wraps` is the number of times the timeline crossed from one back to
    /// zero, allowing a large frame delta to report markers from full loops.
    pub fn crossed(
        &self,
        previous: SamplingRatio,
        current: SamplingRatio,
        wraps: u32,
    ) -> Result<Vec<&AnimationMarker>, Error> {
        if wraps == 0 {
            if current.get() < previous.get() {
                return Err(Error::InvalidMarkerTraversal);
            }
            return Ok(self
                .markers
                .iter()
                .filter(|marker| {
                    marker.ratio.get() > previous.get() && marker.ratio.get() <= current.get()
                })
                .collect());
        }

        let mut crossed = self
            .markers
            .iter()
            .filter(|marker| marker.ratio.get() > previous.get())
            .collect::<Vec<_>>();
        for _ in 1..wraps {
            crossed.extend(self.markers.iter());
        }
        crossed.extend(
            self.markers
                .iter()
                .filter(|marker| marker.ratio.get() <= current.get()),
        );
        Ok(crossed)
    }
}
