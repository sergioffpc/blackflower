use std::fmt;
use std::str::FromStr;

/// Maximum portable map identifier length on the control stream.
pub const MAX_MAP_ID_BYTES: usize = 255;

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

        impl FromStr for $name {
            type Err = ByteIdParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let expected = $size * 2;
                if value.len() != expected {
                    return Err(ByteIdParseError::Length {
                        expected,
                        actual: value.len(),
                    });
                }
                let mut decoded = [0_u8; $size];
                for (index, byte) in decoded.iter_mut().enumerate() {
                    let offset = index * 2;
                    let high = decode_hex_nibble(value.as_bytes()[offset], offset)?;
                    let low = decode_hex_nibble(value.as_bytes()[offset + 1], offset + 1)?;
                    *byte = high << 4 | low;
                }
                Ok(Self::from_bytes(decoded))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in self.as_bytes() {
                    write!(formatter, "{byte:02x}")?;
                }
                Ok(())
            }
        }
    };
}

/// Invalid lowercase-or-uppercase hexadecimal byte identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ByteIdParseError {
    /// Hexadecimal text has the wrong exact length for the identity.
    #[error("hex identity length is {actual}, expected {expected}")]
    Length {
        /// Exact number of characters required by the identity.
        expected: usize,
        /// Number of characters supplied by the caller.
        actual: usize,
    },
    /// One character is not a hexadecimal digit.
    #[error("hex identity contains an invalid digit at byte {index}")]
    Digit {
        /// Byte position of the invalid ASCII character.
        index: usize,
    },
}

fn decode_hex_nibble(byte: u8, index: usize) -> Result<u8, ByteIdParseError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ByteIdParseError::Digit { index }),
    }
}

byte_id!(SessionId, 16, "Opaque 128-bit game-session identity.");
byte_id!(PlayerId, 16, "Opaque 128-bit player identity.");
byte_id!(MatchId, 16, "Opaque 128-bit match identity.");
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

/// Portable logical identity of the map selected by the server.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MapId(String);

impl MapId {
    /// Return the canonical wire and display representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for MapId {
    type Err = MapIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_map_id(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for MapId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Invalid portable map identity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid map identity `{value}`: {reason}")]
pub struct MapIdParseError {
    value: String,
    reason: &'static str,
}

fn validate_map_id(value: &str) -> Result<(), MapIdParseError> {
    let invalid = |reason| MapIdParseError {
        value: value.to_owned(),
        reason,
    };
    if value.is_empty() {
        return Err(invalid("value is empty"));
    }
    if value.len() > MAX_MAP_ID_BYTES {
        return Err(invalid("value exceeds 255 bytes"));
    }
    if !value.is_ascii() {
        return Err(invalid("value must be ASCII"));
    }
    for segment in value.split('/') {
        if segment.is_empty() {
            return Err(invalid("segments cannot be empty"));
        }
        if matches!(segment, "." | "..") {
            return Err(invalid("`.` and `..` segments are forbidden"));
        }
        if !segment.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        }) {
            return Err(invalid(
                "segments may contain only lowercase ASCII letters, digits, `.`, `_`, and `-`",
            ));
        }
    }
    Ok(())
}
