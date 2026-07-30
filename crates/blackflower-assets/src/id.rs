use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{InvalidAssetId, InvalidPackageName};

const MAX_ASSET_ID_BYTES: usize = 255;
const MAX_PACKAGE_NAME_BYTES: usize = 64;

/// Stable logical identifier used to resolve an asset through layered packages.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetId(String);

impl AssetId {
    /// Returns the canonical string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AssetId {
    type Err = InvalidAssetId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_asset_id(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl Serialize for AssetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AssetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(serde::de::Error::custom)
    }
}

/// Validated logical filename stem for one cooked package.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName(String);

impl PackageName {
    /// Returns the logical name without the `.squashfs` suffix.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the complete portable filename.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.squashfs", self.0)
    }

    pub(crate) fn from_file_name(value: &str) -> Result<Self, InvalidPackageName> {
        let stem = value
            .strip_suffix(".squashfs")
            .ok_or_else(|| InvalidPackageName::new(value, "filename must end with `.squashfs`"))?;
        Self::from_str(stem)
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PackageName {
    type Err = InvalidPackageName;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_package_name(value)?;
        Ok(Self(value.to_owned()))
    }
}

fn validate_asset_id(value: &str) -> Result<(), InvalidAssetId> {
    if value.is_empty() {
        return Err(InvalidAssetId::new(value, "value is empty"));
    }
    if value.len() > MAX_ASSET_ID_BYTES {
        return Err(InvalidAssetId::new(value, "value exceeds 255 bytes"));
    }
    if !value.is_ascii() {
        return Err(InvalidAssetId::new(value, "value must be ASCII"));
    }
    for segment in value.split('/') {
        validate_asset_segment(value, segment)?;
    }
    Ok(())
}

fn validate_asset_segment(value: &str, segment: &str) -> Result<(), InvalidAssetId> {
    if segment.is_empty() {
        return Err(InvalidAssetId::new(value, "segments cannot be empty"));
    }
    if matches!(segment, "." | "..") {
        return Err(InvalidAssetId::new(
            value,
            "`.` and `..` segments are forbidden",
        ));
    }
    if !segment.bytes().all(is_portable_name_byte) {
        return Err(InvalidAssetId::new(
            value,
            "segments may contain only lowercase ASCII letters, digits, `.`, `_`, and `-`",
        ));
    }
    Ok(())
}

fn validate_package_name(value: &str) -> Result<(), InvalidPackageName> {
    if value.is_empty() {
        return Err(InvalidPackageName::new(value, "value is empty"));
    }
    if value.len() > MAX_PACKAGE_NAME_BYTES {
        return Err(InvalidPackageName::new(value, "value exceeds 64 bytes"));
    }
    if !value.is_ascii() {
        return Err(InvalidPackageName::new(value, "value must be ASCII"));
    }
    let mut bytes = value.bytes();
    let first = bytes
        .next()
        .ok_or_else(|| InvalidPackageName::new(value, "value is empty"))?;
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(InvalidPackageName::new(
            value,
            "value must start with a lowercase ASCII letter or digit",
        ));
    }
    if !bytes.all(is_portable_name_byte) {
        return Err(InvalidPackageName::new(
            value,
            "value may contain only lowercase ASCII letters, digits, `.`, `_`, and `-`",
        ));
    }
    Ok(())
}

fn is_portable_name_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::{AssetId, PackageName};

    #[test]
    fn accepts_canonical_ids() -> Result<(), Box<dyn std::error::Error>> {
        let id = AssetId::from_str("characters/soldier.mesh_v2")?;
        assert_eq!(id.as_str(), "characters/soldier.mesh_v2");
        Ok(())
    }

    #[test]
    fn rejects_non_canonical_ids() {
        for invalid in [
            "",
            "/root",
            "root/",
            "root//child",
            "root/../child",
            "Root/child",
            "root/café",
        ] {
            assert!(
                AssetId::from_str(invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
    }

    #[test]
    fn package_names_have_portable_filenames() -> Result<(), Box<dyn std::error::Error>> {
        let name = PackageName::from_str("pak900-hotfix")?;
        assert_eq!(name.file_name(), "pak900-hotfix.squashfs");
        assert_eq!(PackageName::from_file_name(&name.file_name())?, name);
        Ok(())
    }

    #[test]
    fn rejects_non_portable_package_names() {
        for invalid in ["", "-pak", "Pak000", "pák000", &"p".repeat(65)] {
            assert!(
                PackageName::from_str(invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
    }
}
