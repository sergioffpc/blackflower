use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use backhand::compression::{CompressionOptions, Compressor};
use backhand::{FilesystemReader, InnerNode, Squashfs, SquashfsFileReader};

use crate::catalog::ASSET_CATALOG_SCHEMA;
use crate::error::io_error;
use crate::signature::verify_package_signature;
use crate::{
    AssetCatalog, AssetId, AssetKeyId, AssetRecord, AssetTrustStore, ContentHash, Error,
    PackageHash, PackageName, PackagePayloadHash,
};

const CATALOG_PATH: &str = "blackflower/catalog.json";
const OBJECT_ROOT: &str = "objects/blake3";
const MAX_CATALOG_BYTES: u64 = 16 * 1024 * 1024;
const ARCHIVE_BLOCK_SIZE: u32 = 128 * 1024;
const ARCHIVE_ZSTD_LEVEL: u32 = 3;

/// One immutable cooked SquashFS package.
pub struct AssetPackage {
    name: PackageName,
    path: PathBuf,
    hash: PackageHash,
    payload_hash: PackagePayloadHash,
    signing_key_id: AssetKeyId,
    catalog: AssetCatalog,
    filesystem: FilesystemReader<'static>,
    object_nodes: BTreeMap<String, usize>,
}

impl core::fmt::Debug for AssetPackage {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AssetPackage")
            .field("name", &self.name)
            .field("path", &self.path)
            .field("hash", &self.hash)
            .field("payload_hash", &self.payload_hash)
            .field("signing_key_id", &self.signing_key_id)
            .field("catalog", &self.catalog)
            .finish_non_exhaustive()
    }
}

impl AssetPackage {
    /// Opens and validates one package without eagerly reading its asset objects.
    ///
    /// # Errors
    ///
    /// Returns an error when the filename, SquashFS structure, fixed archive
    /// settings, embedded catalog, or catalog-to-object mapping is invalid.
    pub fn open(path: impl AsRef<Path>, trust_store: &AssetTrustStore) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        let name = package_name_from_path(&path)?;
        let mut file = File::open(&path).map_err(|source| io_error(&path, source))?;
        let verified = verify_package_signature(&path, &mut file, trust_store)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|source| io_error(&path, source))?;
        let squashfs =
            Squashfs::from_reader(BufReader::new(file)).map_err(|source| Error::Squashfs {
                path: path.clone(),
                source,
            })?;
        validate_superblock(&path, &squashfs)?;
        let filesystem = squashfs
            .into_filesystem_reader()
            .map_err(|source| Error::Squashfs {
                path: path.clone(),
                source,
            })?;
        let catalog = read_catalog(&path, &filesystem)?;
        let object_nodes = validate_catalog_and_nodes(&path, &catalog, &filesystem)?;
        Ok(Self {
            name,
            path,
            hash: verified.package_hash,
            payload_hash: verified.payload_hash,
            signing_key_id: verified.key_id,
            catalog,
            filesystem,
            object_nodes,
        })
    }

    /// Opens a package and checks its exact byte identity.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::open`] or a hash mismatch.
    pub fn open_verified(
        path: impl AsRef<Path>,
        expected: PackageHash,
        trust_store: &AssetTrustStore,
    ) -> Result<Self, Error> {
        let package = Self::open(path, trust_store)?;
        if package.hash != expected {
            return Err(Error::PackageHashMismatch {
                path: package.path.clone(),
                expected,
                actual: package.hash,
            });
        }
        Ok(package)
    }

    /// Logical filename stem that controls package precedence.
    #[must_use]
    pub const fn name(&self) -> &PackageName {
        &self.name
    }

    /// Filesystem path from which the package was opened.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// BLAKE3 identity of every package byte.
    #[must_use]
    pub const fn hash(&self) -> PackageHash {
        self.hash
    }

    /// BLAKE3 digest of the SquashFS payload authenticated by Ed25519.
    #[must_use]
    pub const fn payload_hash(&self) -> PackagePayloadHash {
        self.payload_hash
    }

    /// Identity of the trusted public key that authenticated this package.
    #[must_use]
    pub const fn signing_key_id(&self) -> AssetKeyId {
        self.signing_key_id
    }

    /// Embedded strict catalog.
    #[must_use]
    pub const fn catalog(&self) -> &AssetCatalog {
        &self.catalog
    }

    /// Opens an asset object as a verified sequential reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the ID is absent or its object mapping is invalid.
    pub fn open_asset(&self, id: &AssetId) -> Result<AssetReader<'_>, Error> {
        let record = self
            .catalog
            .find(id)
            .ok_or_else(|| Error::AssetNotFound(id.clone()))?;
        let index = self
            .object_nodes
            .get(&record.object_path)
            .copied()
            .ok_or_else(|| Error::MissingObject {
                asset: id.clone(),
                object_path: record.object_path.clone(),
                package: self.path.clone(),
            })?;
        let node = self
            .filesystem
            .root
            .nodes
            .get(index)
            .ok_or_else(|| Error::MissingObject {
                asset: id.clone(),
                object_path: record.object_path.clone(),
                package: self.path.clone(),
            })?;
        let InnerNode::File(file) = &node.inner else {
            return Err(Error::MissingObject {
                asset: id.clone(),
                object_path: record.object_path.clone(),
                package: self.path.clone(),
            });
        };
        Ok(AssetReader::new(
            self.filesystem.file(file).reader(),
            record.content_hash,
            record.byte_len,
        ))
    }

    /// Reads and verifies one complete asset object.
    ///
    /// # Errors
    ///
    /// Returns an error if resolution, decompression, byte length, or content
    /// verification fails.
    pub fn read_asset(&self, id: &AssetId) -> Result<Vec<u8>, Error> {
        let mut reader = self.open_asset(id)?;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|source| io_error(self.path.clone(), source))?;
        Ok(bytes)
    }
}

/// Lazy sequential reader for one content-addressed package object.
pub struct AssetReader<'a> {
    inner: backhand::SquashfsReadFile<'a, 'static>,
    hasher: blake3::Hasher,
    expected_hash: ContentHash,
    expected_len: u64,
    bytes_read: u64,
    verified: bool,
}

impl<'a> AssetReader<'a> {
    fn new(
        inner: backhand::SquashfsReadFile<'a, 'static>,
        expected_hash: ContentHash,
        expected_len: u64,
    ) -> Self {
        Self {
            inner,
            hasher: blake3::Hasher::new(),
            expected_hash,
            expected_len,
            bytes_read: 0,
            verified: false,
        }
    }

    fn verify_complete(&mut self) -> std::io::Result<()> {
        let actual = ContentHash::from_bytes(*self.hasher.finalize().as_bytes());
        if actual != self.expected_hash {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "asset content hash mismatch: expected {}, found {actual}",
                    self.expected_hash
                ),
            ));
        }
        self.verified = true;
        Ok(())
    }
}

impl Read for AssetReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.verified {
            return Ok(0);
        }
        let read = self.inner.read(buffer)?;
        if read == 0 {
            if self.bytes_read != self.expected_len {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!(
                        "asset ended after {} bytes; expected {}",
                        self.bytes_read, self.expected_len
                    ),
                ));
            }
            self.verify_complete()?;
            return Ok(0);
        }
        let read_u64 = u64::try_from(read)
            .map_err(|source| std::io::Error::new(std::io::ErrorKind::InvalidData, source))?;
        self.bytes_read = self.bytes_read.checked_add(read_u64).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "asset byte count overflow")
        })?;
        if self.bytes_read > self.expected_len {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "asset contains more bytes than its catalog record",
            ));
        }
        self.hasher.update(&buffer[..read]);
        if self.bytes_read == self.expected_len {
            self.verify_complete()?;
        }
        Ok(read)
    }
}

fn package_name_from_path(path: &Path) -> Result<PackageName, Error> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            crate::InvalidPackageName::new(path.display().to_string(), "filename must be UTF-8")
        })?;
    PackageName::from_file_name(file_name).map_err(Error::from)
}

fn validate_superblock(path: &Path, squashfs: &Squashfs<'_>) -> Result<(), Error> {
    let superblock = squashfs.superblock;
    if superblock.version_major != 4 || superblock.version_minor != 0 {
        return unsupported(path, "SquashFS must use version 4.0");
    }
    if superblock.block_size != ARCHIVE_BLOCK_SIZE {
        return unsupported(path, "SquashFS block size must be 128 KiB");
    }
    if superblock.mod_time != 0 {
        return unsupported(path, "SquashFS modification time must be zero");
    }
    if superblock.compressor != Compressor::Zstd {
        return unsupported(path, "SquashFS compressor must be Zstd");
    }
    if squashfs.compression_options
        != Some(CompressionOptions::Zstd(backhand::compression::Zstd {
            compression_level: ARCHIVE_ZSTD_LEVEL,
        }))
    {
        return unsupported(path, "SquashFS Zstd level must be 3");
    }
    if superblock.xattr_table != backhand::v4::squashfs::NOT_SET {
        return unsupported(path, "SquashFS xattrs are forbidden");
    }
    if superblock.export_table != backhand::v4::squashfs::NOT_SET {
        return unsupported(path, "SquashFS export tables are forbidden");
    }
    Ok(())
}

fn unsupported<T>(path: &Path, reason: &'static str) -> Result<T, Error> {
    Err(Error::UnsupportedPackage {
        path: path.to_path_buf(),
        reason,
    })
}

fn read_catalog(
    path: &Path,
    filesystem: &FilesystemReader<'static>,
) -> Result<AssetCatalog, Error> {
    let node = find_node(filesystem, CATALOG_PATH)
        .ok_or_else(|| Error::MissingCatalog(path.to_path_buf()))?;
    let InnerNode::File(file) = &node.inner else {
        return Err(Error::MissingCatalog(path.to_path_buf()));
    };
    let mut reader = filesystem.file(file).reader().take(MAX_CATALOG_BYTES);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|source| io_error(path, source))?;
    serde_json::from_slice(&bytes).map_err(|source| Error::CatalogJson {
        path: path.to_path_buf(),
        source,
    })
}

fn validate_catalog_and_nodes(
    path: &Path,
    catalog: &AssetCatalog,
    filesystem: &FilesystemReader<'static>,
) -> Result<BTreeMap<String, usize>, Error> {
    validate_catalog(path, catalog)?;
    let expected_objects: BTreeSet<&str> = catalog
        .assets
        .iter()
        .map(|record| record.object_path.as_str())
        .collect();
    let mut object_nodes = BTreeMap::new();
    for (index, node) in filesystem.root.nodes.iter().enumerate() {
        let archive_path = archive_path(&node.fullpath).ok_or_else(|| Error::InvalidCatalog {
            path: path.to_path_buf(),
            reason: "archive contains a non-UTF-8 path".to_owned(),
        })?;
        validate_node(path, &archive_path, node, &expected_objects)?;
        if expected_objects.contains(archive_path.as_str()) {
            object_nodes.insert(archive_path, index);
        }
    }
    for record in &catalog.assets {
        validate_record_object(path, record, filesystem, &object_nodes)?;
    }
    Ok(object_nodes)
}

fn validate_catalog(path: &Path, catalog: &AssetCatalog) -> Result<(), Error> {
    if catalog.schema != ASSET_CATALOG_SCHEMA {
        return Err(Error::UnsupportedCatalogSchema {
            path: path.to_path_buf(),
            schema: catalog.schema,
        });
    }
    if catalog.profile.is_empty()
        || catalog.toolchain.cooker.is_empty()
        || catalog.toolchain.squashfs.is_empty()
        || catalog.toolchain.archive.is_empty()
    {
        return invalid_catalog(path, "profile and toolchain fields cannot be empty");
    }
    let mut previous_id: Option<&AssetId> = None;
    for record in &catalog.assets {
        if previous_id.is_some_and(|previous| previous >= &record.id) {
            return invalid_catalog(path, "asset records must be strictly ordered by ID");
        }
        validate_dependencies(path, record)?;
        let expected_path = format!("{OBJECT_ROOT}/{}", record.content_hash);
        if record.object_path != expected_path {
            return invalid_catalog(
                path,
                format!("asset `{}` has non-canonical object path", record.id),
            );
        }
        previous_id = Some(&record.id);
    }
    Ok(())
}

fn validate_dependencies(path: &Path, record: &AssetRecord) -> Result<(), Error> {
    let mut previous: Option<&AssetId> = None;
    for dependency in &record.dependencies {
        if previous.is_some_and(|value| value >= dependency) {
            return invalid_catalog(
                path,
                format!("dependencies for `{}` must be strictly ordered", record.id),
            );
        }
        previous = Some(dependency);
    }
    Ok(())
}

fn validate_node(
    path: &Path,
    archive_path: &str,
    node: &backhand::Node<SquashfsFileReader>,
    expected_objects: &BTreeSet<&str>,
) -> Result<(), Error> {
    if node.header.uid != 0 || node.header.gid != 0 || node.header.mtime != 0 {
        return invalid_catalog(
            path,
            format!("`{archive_path}` has non-canonical ownership"),
        );
    }
    match &node.inner {
        InnerNode::Dir(_) => {
            if !matches!(
                archive_path,
                "/" | "blackflower" | "objects" | "objects/blake3"
            ) {
                return invalid_catalog(path, format!("unexpected directory `{archive_path}`"));
            }
            if node.header.permissions != 0o555 {
                return invalid_catalog(path, format!("directory `{archive_path}` must be 0555"));
            }
        }
        InnerNode::File(_) => {
            if archive_path != CATALOG_PATH && !expected_objects.contains(archive_path) {
                return invalid_catalog(path, format!("unexpected file `{archive_path}`"));
            }
            if node.header.permissions != 0o444 {
                return invalid_catalog(path, format!("file `{archive_path}` must be 0444"));
            }
        }
        InnerNode::Symlink(_)
        | InnerNode::CharacterDevice(_)
        | InnerNode::BlockDevice(_)
        | InnerNode::NamedPipe
        | InnerNode::Socket => {
            return invalid_catalog(path, format!("special node `{archive_path}` is forbidden"));
        }
    }
    Ok(())
}

fn validate_record_object(
    path: &Path,
    record: &AssetRecord,
    filesystem: &FilesystemReader<'static>,
    object_nodes: &BTreeMap<String, usize>,
) -> Result<(), Error> {
    let index = object_nodes
        .get(&record.object_path)
        .copied()
        .ok_or_else(|| Error::MissingObject {
            asset: record.id.clone(),
            object_path: record.object_path.clone(),
            package: path.to_path_buf(),
        })?;
    let node = filesystem
        .root
        .nodes
        .get(index)
        .ok_or_else(|| Error::MissingObject {
            asset: record.id.clone(),
            object_path: record.object_path.clone(),
            package: path.to_path_buf(),
        })?;
    let InnerNode::File(file) = &node.inner else {
        return Err(Error::MissingObject {
            asset: record.id.clone(),
            object_path: record.object_path.clone(),
            package: path.to_path_buf(),
        });
    };
    let actual = u64::try_from(file.file_len()).map_err(|source| Error::InvalidCatalog {
        path: path.to_path_buf(),
        reason: format!("object size does not fit u64: {source}"),
    })?;
    if actual != record.byte_len {
        return Err(Error::ObjectSizeMismatch {
            asset: record.id.clone(),
            expected: record.byte_len,
            actual,
            package: path.to_path_buf(),
        });
    }
    Ok(())
}

fn find_node<'a>(
    filesystem: &'a FilesystemReader<'static>,
    expected_path: &str,
) -> Option<&'a backhand::Node<SquashfsFileReader>> {
    filesystem
        .files()
        .find(|node| archive_path(&node.fullpath).as_deref() == Some(expected_path))
}

fn archive_path(path: &Path) -> Option<String> {
    if path == Path::new("/") {
        return Some("/".to_owned());
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_str()?),
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.join("/"))
}

fn invalid_catalog(path: &Path, reason: impl Into<String>) -> Result<(), Error> {
    Err(Error::InvalidCatalog {
        path: path.to_path_buf(),
        reason: reason.into(),
    })
}
