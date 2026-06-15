use serde::{Deserialize, Serialize};

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(u64);

impl EntityId {
    pub const NONE: Self = Self(0);

    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl From<u64> for EntityId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<EntityId> for u64 {
    fn from(value: EntityId) -> Self {
        value.0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Default)]
pub struct EntityIdAllocator {
    next: u64,
}

impl EntityIdAllocator {
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 0 }
    }

    pub const fn allocate(&mut self) -> EntityId {
        self.next += 1;
        EntityId(self.next)
    }
}
