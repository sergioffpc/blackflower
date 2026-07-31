use crate::Error;

/// The number of Detour polygon area identifiers.
pub const MAX_AREAS: usize = 64;

/// Polygon flags and per-area traversal costs applied to a navigation query.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryFilter {
    pub(crate) include_flags: u16,
    pub(crate) exclude_flags: u16,
    pub(crate) area_costs: [f32; MAX_AREAS],
}

impl QueryFilter {
    /// Construct Detour's default filter: all flags included, none excluded,
    /// and a traversal cost of one for every area.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            include_flags: u16::MAX,
            exclude_flags: 0,
            area_costs: [1.0; MAX_AREAS],
        }
    }

    /// Replace the polygon flags accepted by the filter.
    #[must_use]
    pub const fn with_include_flags(mut self, flags: u16) -> Self {
        self.include_flags = flags;
        self
    }

    /// Replace the polygon flags rejected by the filter.
    #[must_use]
    pub const fn with_exclude_flags(mut self, flags: u16) -> Self {
        self.exclude_flags = flags;
        self
    }

    /// Polygon flags accepted by this filter.
    #[must_use]
    pub const fn include_flags(&self) -> u16 {
        self.include_flags
    }

    /// Polygon flags rejected by this filter.
    #[must_use]
    pub const fn exclude_flags(&self) -> u16 {
        self.exclude_flags
    }

    /// Return the traversal multiplier configured for one Detour area.
    pub fn area_cost(&self, area: u8) -> Result<f32, Error> {
        self.area_costs
            .get(usize::from(area))
            .copied()
            .ok_or(Error::InvalidArea(area))
    }

    /// Set the finite, positive traversal cost for one Detour area.
    pub fn with_area_cost(mut self, area: u8, cost: f32) -> Result<Self, Error> {
        let area_index = usize::from(area);
        let Some(area_cost) = self.area_costs.get_mut(area_index) else {
            return Err(Error::InvalidArea(area));
        };
        if !cost.is_finite() || cost <= 0.0 {
            return Err(Error::InvalidAreaCost);
        }
        *area_cost = cost;
        Ok(self)
    }
}

impl Default for QueryFilter {
    fn default() -> Self {
        Self::new()
    }
}
