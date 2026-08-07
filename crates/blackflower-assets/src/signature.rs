use std::collections::BTreeMap;
use std::fs::File;
#[cfg(feature = "signing")]
use std::fs::OpenOptions;
#[cfg(feature = "signing")]
use std::io::Write;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use ed25519_dalek::{Signature, VerifyingKey};
#[cfg(feature = "signing")]
use ed25519_dalek::{
    Signer, SigningKey,
    pkcs8::{DecodePrivateKey, DecodePublicKey},
};

use crate::error::io_error;
use crate::{AssetKeyId, Error, PackageHash, PackagePayloadHash};

const FOOTER_MAGIC: &[u8; 8] = b"BFSIG001";
const FOOTER_SCHEMA: u16 = 1;
const HASH_ALGORITHM_BLAKE3: u8 = 1;
const SIGNATURE_ALGORITHM_ED25519: u8 = 1;
const FOOTER_LENGTH: usize = 152;
const FOOTER_LENGTH_U64: u64 = 152;
const IO_BUFFER_LENGTH: usize = 64 * 1024;
const IO_BUFFER_LENGTH_U64: u64 = 64 * 1024;

/// Public keys that an executable permits to sign asset packages.
///
/// Key trust authenticates provenance, not deployment freshness. Production
/// runtimes must additionally use [`crate::AssetStore::open_dir_verified`] or
/// [`crate::AssetPackage::open_verified`] with identities from trusted
/// deployment metadata.
#[derive(Clone, Default)]
pub struct AssetTrustStore {
    keys: BTreeMap<AssetKeyId, VerifyingKey>,
}

impl core::fmt::Debug for AssetTrustStore {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AssetTrustStore")
            .field("key_ids", &self.keys.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AssetTrustStore {
    /// Creates an empty trust store.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            keys: BTreeMap::new(),
        }
    }

    /// Creates a trust store from raw 32-byte Ed25519 public keys.
    ///
    /// # Errors
    ///
    /// Returns an error when any key has an invalid or weak encoding.
    pub fn from_public_keys(
        public_keys: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, Error> {
        let mut store = Self::new();
        for public_key in public_keys {
            let _key_id = store.trust(public_key)?;
        }
        Ok(store)
    }

    /// Adds one raw Ed25519 public key and returns its stable BLAKE3 identity.
    ///
    /// # Errors
    ///
    /// Returns an error when the key has an invalid or weak encoding.
    pub fn trust(&mut self, public_key: [u8; 32]) -> Result<AssetKeyId, Error> {
        let key =
            VerifyingKey::from_bytes(&public_key).map_err(|source| Error::InvalidPublicKey {
                reason: format!("encoding does not represent a valid curve point: {source}"),
            })?;
        if key.is_weak() {
            return Err(Error::InvalidPublicKey {
                reason: "weak public keys are forbidden".to_owned(),
            });
        }
        let key_id = key_id(&public_key);
        self.keys.insert(key_id, key);
        Ok(key_id)
    }

    /// Decodes and trusts one standard SPKI PEM public key.
    ///
    /// # Errors
    ///
    /// Returns an error when the PEM or Ed25519 public key is invalid or weak.
    #[cfg(feature = "signing")]
    pub fn trust_public_key_pem(&mut self, pem: &str) -> Result<AssetKeyId, Error> {
        let key =
            VerifyingKey::from_public_key_pem(pem).map_err(|source| Error::InvalidPublicKey {
                reason: format!("SPKI PEM does not contain a valid Ed25519 public key: {source}"),
            })?;
        self.trust(key.to_bytes())
    }

    /// Number of trusted public keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether no signing key is currently trusted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn key(&self, key_id: &AssetKeyId) -> Option<&VerifyingKey> {
        self.keys.get(key_id)
    }
}

pub(crate) struct VerifiedPackageSignature {
    pub(crate) package_hash: PackageHash,
    pub(crate) payload_hash: PackagePayloadHash,
    pub(crate) key_id: AssetKeyId,
}

pub(crate) fn verify_package_signature(
    path: &Path,
    file: &mut File,
    trust_store: &AssetTrustStore,
) -> Result<VerifiedPackageSignature, Error> {
    let file_length = file
        .metadata()
        .map_err(|source| io_error(path, source))?
        .len();
    let payload_length = file_length.checked_sub(FOOTER_LENGTH_U64).ok_or_else(|| {
        Error::InvalidSignatureFooter {
            path: path.to_path_buf(),
            reason: "file is shorter than the signature footer",
        }
    })?;
    let footer = read_footer(path, file, payload_length)?;
    validate_footer(path, &footer, payload_length)?;

    let claimed_payload_hash = PackagePayloadHash::from_bytes(array_from_slice(&footer[24..56]));
    let key_id = AssetKeyId::from_bytes(array_from_slice(&footer[56..88]));
    let signature_bytes = array_from_slice(&footer[88..152]);
    let verifying_key = trust_store
        .key(&key_id)
        .ok_or_else(|| Error::UntrustedSigningKey {
            path: path.to_path_buf(),
            key_id,
        })?;

    let (package_hash, payload_hash) =
        hash_package_and_payload(path, file, payload_length, &footer)?;
    if payload_hash != claimed_payload_hash {
        return Err(Error::InvalidSignatureFooter {
            path: path.to_path_buf(),
            reason: "signed BLAKE3 digest does not match the SquashFS payload",
        });
    }

    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify_strict(payload_hash.as_bytes(), &signature)
        .map_err(|source| Error::InvalidPackageSignature {
            path: path.to_path_buf(),
            key_id,
            source,
        })?;

    Ok(VerifiedPackageSignature {
        package_hash,
        payload_hash,
        key_id,
    })
}

fn read_footer(
    path: &Path,
    file: &mut File,
    payload_length: u64,
) -> Result<[u8; FOOTER_LENGTH], Error> {
    file.seek(SeekFrom::Start(payload_length))
        .map_err(|source| io_error(path, source))?;
    let mut footer = [0_u8; FOOTER_LENGTH];
    file.read_exact(&mut footer)
        .map_err(|source| io_error(path, source))?;
    Ok(footer)
}

fn validate_footer(
    path: &Path,
    footer: &[u8; FOOTER_LENGTH],
    payload_length: u64,
) -> Result<(), Error> {
    if &footer[0..8] != FOOTER_MAGIC {
        return invalid_footer(path, "signature footer magic is absent");
    }
    if u16::from_le_bytes([footer[8], footer[9]]) != FOOTER_SCHEMA {
        return invalid_footer(path, "signature footer schema is unsupported");
    }
    if footer[10] != HASH_ALGORITHM_BLAKE3 {
        return invalid_footer(path, "signature hash algorithm is unsupported");
    }
    if footer[11] != SIGNATURE_ALGORITHM_ED25519 {
        return invalid_footer(path, "signature algorithm is unsupported");
    }
    if footer[12..16] != [0_u8; 4] {
        return invalid_footer(path, "signature footer reserved bytes are not zero");
    }
    let declared_length = u64::from_le_bytes(array_from_slice(&footer[16..24]));
    if declared_length != payload_length {
        return invalid_footer(path, "declared SquashFS payload length is incorrect");
    }
    Ok(())
}

fn hash_package_and_payload(
    path: &Path,
    file: &mut File,
    payload_length: u64,
    footer: &[u8; FOOTER_LENGTH],
) -> Result<(PackageHash, PackagePayloadHash), Error> {
    file.seek(SeekFrom::Start(0))
        .map_err(|source| io_error(path, source))?;
    let mut package_hasher = blake3::Hasher::new();
    let mut payload_hasher = blake3::Hasher::new();
    let mut remaining = payload_length;
    let mut buffer = [0_u8; IO_BUFFER_LENGTH];
    while remaining > 0 {
        let read_length = usize::try_from(remaining.min(IO_BUFFER_LENGTH_U64))
            .map_err(|source| io_error(path, std::io::Error::other(source)))?;
        file.read_exact(&mut buffer[..read_length])
            .map_err(|source| io_error(path, source))?;
        package_hasher.update(&buffer[..read_length]);
        payload_hasher.update(&buffer[..read_length]);
        remaining = remaining
            .checked_sub(
                u64::try_from(read_length)
                    .map_err(|source| io_error(path, std::io::Error::other(source)))?,
            )
            .ok_or_else(|| {
                io_error(
                    path,
                    std::io::Error::other("package payload length underflow"),
                )
            })?;
    }
    package_hasher.update(footer);
    Ok((
        PackageHash::from_bytes(*package_hasher.finalize().as_bytes()),
        PackagePayloadHash::from_bytes(*payload_hasher.finalize().as_bytes()),
    ))
}

fn invalid_footer<T>(path: &Path, reason: &'static str) -> Result<T, Error> {
    Err(Error::InvalidSignatureFooter {
        path: path.to_path_buf(),
        reason,
    })
}

fn array_from_slice<const LENGTH: usize>(value: &[u8]) -> [u8; LENGTH] {
    let mut bytes = [0_u8; LENGTH];
    bytes.copy_from_slice(value);
    bytes
}

fn key_id(public_key: &[u8; 32]) -> AssetKeyId {
    AssetKeyId::from_bytes(*blake3::hash(public_key).as_bytes())
}

/// Ed25519 private key used only by the offline asset cooker.
#[cfg(feature = "signing")]
pub struct AssetSigningKey(SigningKey);

#[cfg(feature = "signing")]
impl core::fmt::Debug for AssetSigningKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("AssetSigningKey([redacted])")
    }
}

#[cfg(feature = "signing")]
impl AssetSigningKey {
    /// Creates a signing key from a raw 32-byte Ed25519 secret.
    #[must_use]
    pub fn from_bytes(secret: &[u8; 32]) -> Self {
        Self(SigningKey::from_bytes(secret))
    }

    /// Decodes a standard PKCS#8 PEM private key.
    ///
    /// # Errors
    ///
    /// Returns an error when the PEM or Ed25519 private key is invalid.
    pub fn from_pkcs8_pem(pem: &str) -> Result<Self, SigningKeyError> {
        SigningKey::from_pkcs8_pem(pem)
            .map(Self)
            .map_err(SigningKeyError)
    }

    /// Raw public key bytes to embed in an executable's trust configuration.
    #[must_use]
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.0.verifying_key().to_bytes()
    }
}

/// A PKCS#8 private key could not be decoded for the offline cooker.
#[cfg(feature = "signing")]
#[derive(Debug, thiserror::Error)]
#[error("invalid Ed25519 PKCS#8 signing key")]
pub struct SigningKeyError(#[source] ed25519_dalek::pkcs8::Error);

/// Signs the BLAKE3 digest of an unsigned SquashFS payload and appends its footer.
///
/// The returned hash identifies the authenticated payload. The final
/// [`PackageHash`] is calculated when the signed package is opened.
///
/// # Errors
///
/// Returns an error if the payload cannot be read, appended, or synchronized.
#[cfg(feature = "signing")]
pub fn sign_package(
    path: impl AsRef<Path>,
    signing_key: &AssetSigningKey,
) -> Result<PackagePayloadHash, Error> {
    let path = path.as_ref();
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .open(path)
        .map_err(|source| io_error(path, source))?;
    let payload_length = file
        .metadata()
        .map_err(|source| io_error(path, source))?
        .len();
    let payload_hash = hash_unsigned_payload(path, &mut file)?;
    let public_key = signing_key.public_key_bytes();
    let key_id = key_id(&public_key);
    let signature = signing_key.0.sign(payload_hash.as_bytes());
    let footer = encode_footer(payload_length, payload_hash, key_id, signature);
    file.write_all(&footer)
        .map_err(|source| io_error(path, source))?;
    file.sync_all().map_err(|source| io_error(path, source))?;
    Ok(payload_hash)
}

#[cfg(feature = "signing")]
fn hash_unsigned_payload(path: &Path, file: &mut File) -> Result<PackagePayloadHash, Error> {
    file.seek(SeekFrom::Start(0))
        .map_err(|source| io_error(path, source))?;
    let mut hasher = blake3::Hasher::new();
    hasher
        .update_reader(file)
        .map_err(|source| io_error(path, source))?;
    Ok(PackagePayloadHash::from_bytes(
        *hasher.finalize().as_bytes(),
    ))
}

#[cfg(feature = "signing")]
fn encode_footer(
    payload_length: u64,
    payload_hash: PackagePayloadHash,
    key_id: AssetKeyId,
    signature: Signature,
) -> [u8; FOOTER_LENGTH] {
    let mut footer = [0_u8; FOOTER_LENGTH];
    footer[0..8].copy_from_slice(FOOTER_MAGIC);
    footer[8..10].copy_from_slice(&FOOTER_SCHEMA.to_le_bytes());
    footer[10] = HASH_ALGORITHM_BLAKE3;
    footer[11] = SIGNATURE_ALGORITHM_ED25519;
    footer[16..24].copy_from_slice(&payload_length.to_le_bytes());
    footer[24..56].copy_from_slice(payload_hash.as_bytes());
    footer[56..88].copy_from_slice(key_id.as_bytes());
    footer[88..152].copy_from_slice(&signature.to_bytes());
    footer
}

#[cfg(all(test, feature = "signing"))]
#[path = "../tests/unit/signature.rs"]
mod tests;
