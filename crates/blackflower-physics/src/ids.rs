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
