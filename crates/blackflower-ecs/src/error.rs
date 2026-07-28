/// Errors produced while configuring or using an ECS world.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Flecs could not allocate or initialize a world.
    #[error("Flecs world initialization failed")]
    WorldInitialization,
    /// A worker count does not fit the Flecs C API.
    #[error("worker count {0} exceeds the Flecs limit")]
    WorkerCountTooLarge(u32),
    /// A name is empty or contains an interior NUL byte.
    #[error("name must be non-empty and contain no NUL")]
    InvalidName,
    /// A query expression contains an interior NUL byte.
    #[error("query expression must contain no NUL")]
    InvalidExpression,
    /// The Rust type was already registered with another kind or name.
    #[error("Rust type {0} is already registered")]
    DuplicateType(&'static str),
    /// The requested name is already registered in this world.
    #[error("ECS name {0:?} is already registered")]
    DuplicateName(String),
    /// The Rust type has not been registered in this world.
    #[error("Rust type {0} is not registered")]
    UnregisteredType(&'static str),
    /// A handle belongs to another world.
    #[error("handle belongs to another ECS world")]
    WrongWorld,
    /// The entity handle is no longer alive.
    #[error("entity is not alive")]
    DeadEntity,
    /// Flecs rejected component registration.
    #[error("Flecs rejected component {0}")]
    ComponentRegistration(&'static str),
    /// A data-bearing component cannot have a zero-sized Rust layout.
    #[error("component {0} must have a nonzero size")]
    InvalidComponentLayout(&'static str),
    /// Flecs rejected tag or entity creation.
    #[error("Flecs rejected entity {0:?}")]
    EntityCreation(String),
    /// Flecs rejected a query expression.
    #[error("Flecs rejected query {0:?}")]
    QueryCreation(String),
    /// Flecs rejected a system descriptor.
    #[error("Flecs rejected system {0:?}")]
    SystemCreation(String),
    /// Flecs rejected a pipeline descriptor.
    #[error("Flecs rejected pipeline {0:?}")]
    PipelineCreation(String),
    /// A projected field did not match its Rust declaration.
    #[error(transparent)]
    Projection(ProjectionError),
    /// A tick delta must be finite and strictly positive.
    #[error("tick delta must be finite and strictly positive")]
    InvalidTickDelta,
}

/// A precise reason why a DSL field could not be projected to Rust.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectionError {
    /// The requested field index is outside the iterator.
    #[error("field {0} is out of range")]
    FieldOutOfRange(i8),
    /// A required field was not set for this result.
    #[error("required field {0} is not set")]
    RequiredFieldMissing(i8),
    /// An ordinary component projection matched a pair, or vice versa.
    #[error("field {0} has an unexpected pair shape")]
    UnexpectedPair(i8),
    /// The field contains another registered component type.
    #[error("field {0} has another component type")]
    ComponentMismatch(i8),
    /// The C field size differs from the Rust component size.
    #[error("field {0} has another size")]
    SizeMismatch(i8),
    /// Flecs returned a pointer that does not satisfy the Rust alignment.
    #[error("field {0} is not correctly aligned")]
    AlignmentMismatch(i8),
    /// A read projection targeted a write-only field.
    #[error("field {0} is write-only")]
    WriteOnly(i8),
    /// A write projection targeted a read-only field.
    #[error("field {0} is read-only")]
    ReadOnly(i8),
    /// A write projection targeted shared or inherited storage.
    #[error("field {0} is shared and cannot be mutably projected")]
    SharedWrite(i8),
    /// Flecs returned a null pointer for a present data field.
    #[error("field {0} returned a null pointer")]
    NullField(i8),
    /// Two projections would create overlapping mutable Rust references.
    #[error("fields {0} and {1} overlap mutably")]
    AliasedMutableFields(i8, i8),
}

/// Failure reported while executing a world or pipeline.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    /// The selected pipeline belongs to another world.
    #[error("pipeline handle belongs to another ECS world")]
    WrongWorld,
    /// A Rust system callback returned an error or panicked.
    #[error("system {system:?} failed: {message}")]
    Callback {
        /// Stable system name.
        system: String,
        /// Display form of the returned error or captured panic.
        message: String,
    },
}

impl RunError {
    pub(crate) fn new(system: String, message: String) -> Self {
        Self::Callback { system, message }
    }

    /// Name of the system that failed, when this is a callback failure.
    #[must_use]
    pub fn system(&self) -> Option<&str> {
        match self {
            Self::WrongWorld => None,
            Self::Callback { system, .. } => Some(system),
        }
    }

    /// Error or panic message captured by the trampoline, when applicable.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::WrongWorld => None,
            Self::Callback { message, .. } => Some(message),
        }
    }
}

/// Result returned by Rust system callbacks.
pub type SystemResult = Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
