use std::fmt;
use std::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct WorldKey(pub(crate) u64);

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            pub(crate) raw: u64,
            pub(crate) world: WorldKey,
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.raw)
                    .finish()
            }
        }
    };
}

define_id!(EntityId);
define_id!(SystemId);
define_id!(PhaseId);
define_id!(PipelineId);

/// Typed handle for a component registered in one world.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentId<T> {
    pub(crate) raw: u64,
    pub(crate) world: WorldKey,
    pub(crate) marker: PhantomData<fn() -> T>,
}

impl<T> Clone for ComponentId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ComponentId<T> {}

impl<T> fmt::Debug for ComponentId<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ComponentId")
            .field(&self.raw)
            .finish()
    }
}

impl<T> ComponentId<T> {
    /// Return the component's entity handle.
    #[must_use]
    pub fn entity(self) -> EntityId {
        EntityId {
            raw: self.raw,
            world: self.world,
        }
    }
}

/// Typed handle for a tag registered in one world.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagId<T> {
    pub(crate) raw: u64,
    pub(crate) world: WorldKey,
    pub(crate) marker: PhantomData<fn() -> T>,
}

impl<T> Clone for TagId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TagId<T> {}

impl<T> fmt::Debug for TagId<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("TagId").field(&self.raw).finish()
    }
}

impl<T> TagId<T> {
    /// Return the tag's entity handle.
    #[must_use]
    pub fn entity(self) -> EntityId {
        EntityId {
            raw: self.raw,
            world: self.world,
        }
    }
}

/// A validated, explicit fixed-step delta in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickDelta(f32);

impl TickDelta {
    /// Construct a finite, strictly positive tick delta.
    pub fn from_seconds(seconds: f32) -> Result<Self, crate::Error> {
        if seconds.is_finite() && seconds > 0.0 {
            Ok(Self(seconds))
        } else {
            Err(crate::Error::InvalidTickDelta)
        }
    }

    /// Return the validated delta in seconds.
    #[must_use]
    pub const fn as_seconds(self) -> f32 {
        self.0
    }

    #[must_use]
    pub(crate) fn seconds(self) -> f32 {
        self.0
    }

    pub(crate) fn from_flecs(seconds: f32) -> Self {
        debug_assert!(seconds.is_finite() && seconds > 0.0);
        Self(seconds)
    }
}

/// Built-in Flecs pipeline phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinPhase {
    OnStart,
    PreFrame,
    OnLoad,
    PostLoad,
    PreUpdate,
    OnUpdate,
    OnValidate,
    PostUpdate,
    PreStore,
    OnStore,
    PostFrame,
}
