use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::io_error;
use crate::{
    AssetAudience, AssetId, AssetKind, AssetPackage, AssetReader, AssetRecord, AssetSetHash,
    AssetTrustStore, Error, PackageName,
};

const ASSET_SET_SCHEMA: u32 = 1;
const ASSET_SET_DOMAIN: &[u8] = b"blackflower.asset-set.v1";

/// Immutable set of lexicographically layered cooked packages.
#[derive(Debug)]
pub struct AssetStore {
    directory: PathBuf,
    packages: Vec<AssetPackage>,
    hash: AssetSetHash,
}

impl AssetStore {
    /// Opens every direct `.squashfs` child and validates the resulting overlay.
    ///
    /// Package data remains lazy. Packages are stored in ascending ASCII
    /// filename order and resolution searches them in reverse.
    ///
    /// # Errors
    ///
    /// Returns an error for filesystem failures, invalid package names or
    /// contents, incompatible overrides, or unresolved winning dependencies.
    pub fn open_dir(path: impl AsRef<Path>, trust_store: &AssetTrustStore) -> Result<Self, Error> {
        let directory = path.as_ref().to_path_buf();
        let paths = discover_packages(&directory)?;
        let mut packages = Vec::with_capacity(paths.len());
        for package_path in paths {
            packages.push(AssetPackage::open(package_path, trust_store)?);
        }
        validate_overrides(&packages)?;
        validate_winning_dependencies(&packages)?;
        let hash = calculate_asset_set_hash(&packages);
        Ok(Self {
            directory,
            packages,
            hash,
        })
    }

    /// Opens a store and checks the exact ordered package-set identity.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::open_dir`] or an identity mismatch.
    pub fn open_dir_verified(
        path: impl AsRef<Path>,
        expected: AssetSetHash,
        trust_store: &AssetTrustStore,
    ) -> Result<Self, Error> {
        let store = Self::open_dir(path, trust_store)?;
        if store.hash != expected {
            return Err(Error::AssetSetHashMismatch {
                expected,
                actual: store.hash,
            });
        }
        Ok(store)
    }

    /// Directory snapshot used to construct this store.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Packages in ascending filename order.
    #[must_use]
    pub fn packages(&self) -> &[AssetPackage] {
        &self.packages
    }

    /// Exact identity of ordered filenames and package bytes.
    #[must_use]
    pub const fn asset_set_hash(&self) -> AssetSetHash {
        self.hash
    }

    /// Resolves an ID from the lexicographically greatest package downwards.
    #[must_use]
    pub fn resolve(&self, id: &AssetId) -> Option<ResolvedAsset<'_>> {
        self.packages.iter().rev().find_map(|package| {
            package
                .catalog()
                .find(id)
                .map(|record| ResolvedAsset { package, record })
        })
    }

    /// Opens the winning object as a verified sequential reader.
    ///
    /// # Errors
    ///
    /// Returns an error if no package defines the ID or the winning object
    /// cannot be opened.
    pub fn open_asset(&self, id: &AssetId) -> Result<AssetReader<'_>, Error> {
        let resolved = self
            .resolve(id)
            .ok_or_else(|| Error::AssetNotFound(id.clone()))?;
        resolved.package.open_asset(id)
    }

    /// Reads and verifies the complete winning object.
    ///
    /// # Errors
    ///
    /// Returns an error if the asset is absent, corrupt, or cannot be read.
    pub fn read_asset(&self, id: &AssetId) -> Result<Vec<u8>, Error> {
        let resolved = self
            .resolve(id)
            .ok_or_else(|| Error::AssetNotFound(id.clone()))?;
        let mut reader = resolved.package.open_asset(id)?;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|source| io_error(resolved.package.path(), source))?;
        Ok(bytes)
    }
}

/// Winning record and the package from which its bytes will be read.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedAsset<'a> {
    package: &'a AssetPackage,
    record: &'a AssetRecord,
}

impl<'a> ResolvedAsset<'a> {
    /// Winning package.
    #[must_use]
    pub const fn package(&self) -> &'a AssetPackage {
        self.package
    }

    /// Winning complete catalog record.
    #[must_use]
    pub const fn record(&self) -> &'a AssetRecord {
        self.record
    }
}

fn discover_packages(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    let entries = fs::read_dir(directory).map_err(|source| io_error(directory, source))?;
    let mut packages = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| io_error(directory, source))?;
        let path = entry.path();
        let Some(raw_file_name) = path.file_name() else {
            continue;
        };
        let Some(file_name) = raw_file_name.to_str() else {
            if path
                .extension()
                .is_some_and(|extension| extension == "squashfs")
            {
                return Err(crate::InvalidPackageName::new(
                    path.display().to_string(),
                    "filename must be UTF-8",
                )
                .into());
            }
            continue;
        };
        if !file_name.ends_with(".squashfs") {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|source| io_error(&path, source))?;
        if !file_type.is_file() {
            return Err(Error::PackageNotFile(path));
        }
        PackageName::from_file_name(file_name)?;
        packages.push(path);
    }
    packages.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(packages)
}

fn validate_overrides(packages: &[AssetPackage]) -> Result<(), Error> {
    let mut contracts: BTreeMap<&AssetId, (AssetKind, AssetAudience)> = BTreeMap::new();
    for package in packages {
        for record in &package.catalog().assets {
            if let Some((kind, audience)) = contracts.get(&record.id).copied() {
                if kind != record.kind || audience != record.audience {
                    return Err(Error::IncompatibleOverride {
                        asset: record.id.clone(),
                        package: package.name().clone(),
                        expected_kind: kind,
                        expected_audience: audience,
                        actual_kind: record.kind,
                        actual_audience: record.audience,
                    });
                }
            } else {
                contracts.insert(&record.id, (record.kind, record.audience));
            }
        }
    }
    Ok(())
}

fn validate_winning_dependencies(packages: &[AssetPackage]) -> Result<(), Error> {
    let mut winners: BTreeMap<&AssetId, &AssetRecord> = BTreeMap::new();
    for package in packages {
        for record in &package.catalog().assets {
            winners.insert(&record.id, record);
        }
    }
    for record in winners.values() {
        for dependency in &record.dependencies {
            if !winners.contains_key(dependency) {
                return Err(Error::MissingDependency {
                    asset: record.id.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
    }
    Ok(())
}

fn calculate_asset_set_hash(packages: &[AssetPackage]) -> AssetSetHash {
    let mut hasher = blake3::Hasher::new();
    hash_field(&mut hasher, ASSET_SET_DOMAIN);
    hasher.update(&ASSET_SET_SCHEMA.to_le_bytes());
    hasher.update(&usize_to_u64(packages.len()).to_le_bytes());
    for package in packages {
        hash_field(&mut hasher, package.name().file_name().as_bytes());
        hash_field(&mut hasher, package.hash().as_bytes());
    }
    AssetSetHash::from_bytes(*hasher.finalize().as_bytes())
}

fn hash_field(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&usize_to_u64(bytes.len()).to_le_bytes());
    hasher.update(bytes);
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
