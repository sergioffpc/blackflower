use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::InvalidHash;

macro_rules! hash_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 32]);

        impl $name {
            /// Creates an identity from exactly 32 BLAKE3 bytes.
            #[must_use]
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            /// Returns the raw BLAKE3 bytes.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                let hash = blake3::Hash::from_bytes(self.0);
                write!(formatter, "{}", hash.to_hex())
            }
        }

        impl FromStr for $name {
            type Err = InvalidHash;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                if value.len() != 64 || !value.bytes().all(|byte| {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                }) {
                    return Err(InvalidHash(value.to_owned()));
                }
                let hash =
                    blake3::Hash::from_hex(value).map_err(|_| InvalidHash(value.to_owned()))?;
                Ok(Self(*hash.as_bytes()))
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::from_str(&value).map_err(serde::de::Error::custom)
            }
        }
    };
}

hash_type!(
    /// BLAKE3 identity of one cooked asset's final bytes.
    ContentHash
);
hash_type!(
    /// BLAKE3 identity of the complete deterministic recipe for one asset.
    RecipeHash
);
hash_type!(
    /// BLAKE3 identity of every byte in one SquashFS package.
    PackageHash
);
hash_type!(
    /// BLAKE3 digest of the SquashFS payload authenticated by its signature.
    PackagePayloadHash
);
hash_type!(
    /// BLAKE3 identity of one trusted Ed25519 public key.
    AssetKeyId
);
hash_type!(
    /// BLAKE3 identity of the ordered package filenames and package hashes in a store.
    AssetSetHash
);

impl ContentHash {
    /// Hashes final cooked bytes.
    #[must_use]
    pub fn hash_bytes(bytes: &[u8]) -> Self {
        Self::from_bytes(*blake3::hash(bytes).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::ContentHash;

    #[test]
    fn hashes_round_trip_as_lowercase_hex() -> Result<(), Box<dyn std::error::Error>> {
        let hash = ContentHash::hash_bytes(b"blackflower");
        let encoded = hash.to_string();
        assert_eq!(encoded.len(), 64);
        assert_eq!(ContentHash::from_str(&encoded)?, hash);
        Ok(())
    }

    #[test]
    fn rejects_uppercase_and_wrong_length() {
        assert!(ContentHash::from_str("AB").is_err());
        assert!(ContentHash::from_str(&"0".repeat(63)).is_err());
    }
}
