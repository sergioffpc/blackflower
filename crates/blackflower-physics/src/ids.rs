use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct WorldKey(pub(crate) u64);

/// Stable Jolt body handle tied to the world that created it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyId {
    pub(crate) raw: u32,
    pub(crate) world: WorldKey,
}

impl fmt::Debug for BodyId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("BodyId").field(&self.raw).finish()
    }
}

/// Stable character-controller handle tied to the world that created it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CharacterId {
    pub(crate) raw: u32,
    pub(crate) world: WorldKey,
}

impl CharacterId {
    /// Return the rigid body driven by this character controller.
    #[must_use]
    pub const fn body(self) -> BodyId {
        BodyId {
            raw: self.raw,
            world: self.world,
        }
    }
}

impl fmt::Debug for CharacterId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CharacterId")
            .field(&self.raw)
            .finish()
    }
}

/// Stable identifier for a child shape within a collision shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubShapeId(pub(crate) u32);

impl SubShapeId {
    /// Return the opaque numeric identifier used in captured contact facts.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}
