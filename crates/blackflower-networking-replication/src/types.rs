use std::collections::BTreeMap;
use std::fmt;
use std::num::{NonZeroU16, NonZeroU64};

use blackflower_networking::ProtocolRevision;

/// Stable non-zero identity allocated monotonically within one game session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplicatedEntityId(NonZeroU64);

impl ReplicatedEntityId {
    /// Construct an identity from an already validated non-zero value.
    #[must_use]
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    /// Validate a raw protocol value.
    pub const fn try_from_u64(value: u64) -> Result<Self, IdentityError> {
        match NonZeroU64::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(IdentityError::ZeroEntity),
        }
    }

    /// Return the non-zero protocol value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for ReplicatedEntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Monotonic non-reusing entity identity allocator for one session.
#[derive(Debug, Clone)]
pub struct EntityIdAllocator {
    next: Option<NonZeroU64>,
}

impl Default for EntityIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityIdAllocator {
    /// Start a new session identity domain at one.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next: Some(NonZeroU64::MIN),
        }
    }

    /// Allocate the next identity and never make it available again.
    pub fn allocate(&mut self) -> Result<ReplicatedEntityId, IdentityError> {
        let next = self.next.ok_or(IdentityError::Exhausted)?;
        let allocated = ReplicatedEntityId::new(next);
        self.next = next.get().checked_add(1).and_then(NonZeroU64::new);
        Ok(allocated)
    }
}

/// Stable non-zero component identity registered per protocol revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentId(NonZeroU16);

impl ComponentId {
    /// Construct an identity from an already validated non-zero value.
    #[must_use]
    pub const fn new(value: NonZeroU16) -> Self {
        Self(value)
    }

    /// Validate a raw protocol value.
    pub const fn try_from_u16(value: u16) -> Result<Self, IdentityError> {
        match NonZeroU16::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(IdentityError::ZeroComponent),
        }
    }

    /// Return the non-zero protocol value.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

/// Authoritative simulation tick represented by a replication snapshot.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnapshotTick(u64);

impl SnapshotTick {
    /// Construct a snapshot tick from its protocol value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the protocol value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for SnapshotTick {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Tick at which one component sample was last authoritatively updated.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSampleTick(u64);

impl ComponentSampleTick {
    /// Construct a component sample tick.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the protocol value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Visibility projection applied before canonical component serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProjectionKind {
    /// State visible to every observer with spatial interest.
    Public,
    /// State visible only to the controlling player.
    Owner,
    /// State visible only to members of the same team.
    Team,
    /// Explicit non-spatial match-global state.
    Global,
}

/// Normative ordering used when snapshot capacity is constrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReplicationPriority {
    /// Spawn, forget, and component-removal lifecycle facts.
    Lifecycle,
    /// Correction state required by the owning client's prediction.
    OwnerCorrection,
    /// State of active actors relevant to immediate play.
    ActiveActor,
    /// Remaining non-essential replicated state.
    Remaining,
}

/// Stable component registry entry for one protocol revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentDescriptor {
    /// Stable component identity.
    pub id: ComponentId,
    /// Projection in which the component may be serialized.
    pub projection: ProjectionKind,
    /// Maximum canonical component byte length.
    pub maximum_bytes: u16,
}

/// Immutable stable component registry keyed by protocol revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentRegistry {
    revision: ProtocolRevision,
    components: BTreeMap<ComponentId, ComponentDescriptor>,
}

impl ComponentRegistry {
    /// Build a revision registry, rejecting duplicate IDs and zero bounds.
    pub fn new(
        revision: ProtocolRevision,
        descriptors: impl IntoIterator<Item = ComponentDescriptor>,
    ) -> Result<Self, RegistryError> {
        let mut components = BTreeMap::new();
        for descriptor in descriptors {
            if descriptor.maximum_bytes == 0 {
                return Err(RegistryError::ZeroMaximum { id: descriptor.id });
            }
            let id = descriptor.id;
            if components.insert(id, descriptor).is_some() {
                return Err(RegistryError::DuplicateComponent { id });
            }
        }
        Ok(Self {
            revision,
            components,
        })
    }

    /// Return the exact protocol revision owning this stable mapping.
    #[must_use]
    pub const fn revision(&self) -> ProtocolRevision {
        self.revision
    }

    /// Resolve one component descriptor.
    #[must_use]
    pub fn descriptor(&self, id: ComponentId) -> Option<&ComponentDescriptor> {
        self.components.get(&id)
    }
}

/// Invalid non-zero identity or exhausted session identity domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    /// Replicated entity wire identity zero is reserved.
    #[error("replicated entity identity must be non-zero")]
    ZeroEntity,
    /// Component wire identity zero is reserved.
    #[error("component identity must be non-zero")]
    ZeroComponent,
    /// The 64-bit monotonic identity domain was exhausted.
    #[error("replicated entity identity domain exhausted")]
    Exhausted,
}

/// Invalid stable component registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// A component identity appeared more than once.
    #[error("component {id:?} appears more than once in the revision registry")]
    DuplicateComponent {
        /// Duplicate component identity.
        id: ComponentId,
    },
    /// A component declared no legal payload bytes.
    #[error("component {id:?} has a zero-byte maximum")]
    ZeroMaximum {
        /// Invalid component identity.
        id: ComponentId,
    },
}
