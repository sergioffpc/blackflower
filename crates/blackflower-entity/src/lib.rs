use serde::{Deserialize, Serialize};

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(u64);

impl EntityId {
    /// Sentinel value used to represent "no entity". Equivalent to
    /// `Option::None` when an explicit sentinel is preferable to nesting.
    pub const NONE: Self = Self(0);

    /// Returns `true` if this id is the [`NONE`](Self::NONE) sentinel.
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Allocator for [`EntityId`].
///
/// Hands out sequential IDs starting at `1`. Not thread-safe by itself —
/// owned by the tick thread and accessed without contention.
#[derive(Debug, Default)]
pub struct EntityIdAllocator {
    next: u64,
}

impl EntityIdAllocator {
    /// Create a new allocator. The first ID returned will be `EntityId(1)`.
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 0 }
    }

    /// Allocate a new unique identifier.
    pub const fn allocate(&mut self) -> EntityId {
        self.next += 1;
        EntityId(self.next)
    }
}
