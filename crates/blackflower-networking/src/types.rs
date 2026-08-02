use std::fmt;

/// Application protocol revision carried by admission and compatibility checks.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProtocolRevision(u32);

impl ProtocolRevision {
    /// Initial Blackflower network protocol revision.
    pub const V1: Self = Self(1);

    /// Construct a protocol revision.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the wire value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

macro_rules! integer_id {
    ($name:ident, $value:ty, $description:literal) => {
        #[doc = $description]
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($value);

        impl $name {
            #[doc = concat!("Construct a `", stringify!($name), "`.")]
            #[must_use]
            pub const fn new(value: $value) -> Self {
                Self(value)
            }

            #[doc = concat!("Return the `", stringify!($name), "` wire value.")]
            #[must_use]
            pub const fn get(self) -> $value {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

integer_id!(
    ConnectionEpoch,
    u32,
    "Identity of one QUIC connection within a session."
);
integer_id!(
    FlowSequence,
    u32,
    "Monotonic sequence local to one datagram flow."
);
integer_id!(
    InputSequence,
    u64,
    "Monotonic identity of one canonical control frame."
);
integer_id!(
    CommandId,
    u64,
    "Idempotency identity of one discrete command."
);
/// Opaque voice stream identifier assigned by the authenticated host session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceStreamId(pub u32);

integer_id!(
    SimulationTick,
    u64,
    "Authoritative simulation tick on the wire."
);
integer_id!(
    BootstrapId,
    u64,
    "Identity of one full-state bootstrap transfer."
);

macro_rules! byte_id {
    ($name:ident, $size:literal, $description:literal) => {
        #[doc = $description]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; $size]);

        impl $name {
            #[doc = concat!("Construct a `", stringify!($name), "` from canonical bytes.")]
            #[must_use]
            pub const fn from_bytes(bytes: [u8; $size]) -> Self {
                Self(bytes)
            }

            #[doc = concat!("Return the canonical bytes of this `", stringify!($name), "`.")]
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; $size] {
                &self.0
            }
        }
    };
}

byte_id!(SessionId, 16, "Opaque 128-bit game-session identity.");
byte_id!(PlayerId, 16, "Opaque 128-bit player identity.");
byte_id!(MatchId, 16, "Opaque 128-bit match identity.");
byte_id!(
    SimulationCompatibilityId,
    32,
    "Deterministic simulation compatibility identity."
);
byte_id!(
    RequiredContentSetId,
    32,
    "Required cooked-content set identity."
);
byte_id!(
    ProjectionDigest,
    32,
    "Digest of one reconstructed client projection."
);
