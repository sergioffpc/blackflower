use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{
    AssetAudience, AssetId, AssetKind, AssetRecord, AssetSetHash, AssetStore, AssetTrustStore,
    Error,
};

/// Monotonic identity of one successfully published runtime asset snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetGeneration(u64);

impl AssetGeneration {
    const INITIAL: Self = Self(0);

    /// Returns the generation as an integer.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    fn next(self) -> Result<Self, Error> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(Error::AssetGenerationExhausted)
    }
}

/// Semantic change made to one winning asset record by a reload.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetChangeKind {
    /// The asset was absent from the previous snapshot.
    Added,
    /// The winning record changed while retaining its kind and audience.
    Modified,
    /// The asset is absent from the new snapshot.
    Removed,
}

/// One domain-classified semantic asset change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetChange {
    id: AssetId,
    kind: AssetKind,
    audience: AssetAudience,
    change: AssetChangeKind,
}

impl AssetChange {
    /// Changed logical asset.
    #[must_use]
    pub const fn id(&self) -> &AssetId {
        &self.id
    }

    /// Stable runtime representation of the asset.
    #[must_use]
    pub const fn kind(&self) -> AssetKind {
        self.kind
    }

    /// Domain that decides when the change may become visible.
    #[must_use]
    pub const fn audience(&self) -> AssetAudience {
        self.audience
    }

    /// How the winning record changed.
    #[must_use]
    pub const fn change(&self) -> AssetChangeKind {
        self.change
    }
}

/// Asset changes ordered lexicographically by [`AssetId`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetChangeSet {
    changes: Vec<AssetChange>,
}

impl AssetChangeSet {
    /// Ordered semantic changes.
    #[must_use]
    pub fn changes(&self) -> &[AssetChange] {
        &self.changes
    }

    /// Whether no winning record changed semantically.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Number of changed winning records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.changes.len()
    }
}

/// Cheap immutable handle that keeps one complete store generation alive.
#[derive(Debug, Clone)]
pub struct AssetStoreSnapshot {
    generation: AssetGeneration,
    store: Arc<AssetStore>,
}

impl AssetStoreSnapshot {
    /// Generation published with this snapshot.
    #[must_use]
    pub const fn generation(&self) -> AssetGeneration {
        self.generation
    }

    /// Frozen layered store.
    #[must_use]
    pub fn store(&self) -> &AssetStore {
        &self.store
    }

    /// Exact package-set identity of this snapshot.
    #[must_use]
    pub fn asset_set_hash(&self) -> AssetSetHash {
        self.store.asset_set_hash()
    }
}

/// Result classification for one explicit reload request.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetReloadStatus {
    /// The authenticated directory still has the current package-set identity.
    Unchanged,
    /// A new package set was validated and published.
    Reloaded,
}

/// Transactional result of an explicit asset-store reload.
#[derive(Debug, Clone)]
pub struct AssetReload {
    status: AssetReloadStatus,
    previous_asset_set_hash: AssetSetHash,
    snapshot: AssetStoreSnapshot,
    changes: AssetChangeSet,
}

impl AssetReload {
    /// Whether a new snapshot was published.
    #[must_use]
    pub const fn status(&self) -> AssetReloadStatus {
        self.status
    }

    /// Package-set identity that was current when reload started.
    #[must_use]
    pub const fn previous_asset_set_hash(&self) -> AssetSetHash {
        self.previous_asset_set_hash
    }

    /// Exact snapshot observed or published by this reload.
    #[must_use]
    pub const fn snapshot(&self) -> &AssetStoreSnapshot {
        &self.snapshot
    }

    /// Semantic changes between the previous and resulting snapshots.
    #[must_use]
    pub const fn changes(&self) -> &AssetChangeSet {
        &self.changes
    }
}

/// Owns the current store snapshot and publishes fully validated replacements.
///
/// Reloading performs filesystem I/O and signature verification on the calling
/// thread without locking access to the current snapshot. Applications may call
/// [`Self::reload`] from a worker and distribute the returned snapshot at their
/// domain-specific synchronization boundary.
#[derive(Debug)]
pub struct AssetStoreManager {
    directory: PathBuf,
    trust_store: AssetTrustStore,
    reload_guard: Mutex<()>,
    current: RwLock<ManagerState>,
}

#[derive(Debug)]
struct ManagerState {
    snapshot: AssetStoreSnapshot,
    contracts: BTreeMap<AssetId, (AssetKind, AssetAudience)>,
}

impl AssetStoreManager {
    /// Opens and publishes generation zero from one package directory.
    ///
    /// # Errors
    ///
    /// Returns the same validation errors as [`AssetStore::open_dir`].
    #[cfg(feature = "unversioned-loading")]
    pub fn open_dir(path: impl AsRef<Path>, trust_store: AssetTrustStore) -> Result<Self, Error> {
        let directory = canonical_directory(path.as_ref())?;
        let store = AssetStore::open_dir(&directory, &trust_store)?;
        Ok(Self::from_store(directory, trust_store, store))
    }

    /// Opens generation zero and verifies its exact package-set identity.
    ///
    /// # Errors
    ///
    /// Returns the same validation errors as [`AssetStore::open_dir_verified`].
    pub fn open_dir_verified(
        path: impl AsRef<Path>,
        expected: AssetSetHash,
        trust_store: AssetTrustStore,
    ) -> Result<Self, Error> {
        let directory = canonical_directory(path.as_ref())?;
        let store = AssetStore::open_dir_verified(&directory, expected, &trust_store)?;
        Ok(Self::from_store(directory, trust_store, store))
    }

    /// Directory reopened by every reload.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Returns a cheap handle to the currently published immutable snapshot.
    #[must_use]
    pub fn snapshot(&self) -> AssetStoreSnapshot {
        read_lock(&self.current).snapshot.clone()
    }

    /// Authenticates and validates the directory, then publishes it if changed.
    ///
    /// The current snapshot remains available throughout candidate loading. If
    /// any validation fails, the error is returned and the current generation
    /// is left untouched.
    ///
    /// # Errors
    ///
    /// Returns package validation errors, a cross-generation kind or audience
    /// reclassification, or generation exhaustion.
    #[cfg(feature = "unversioned-loading")]
    pub fn reload(&self) -> Result<AssetReload, Error> {
        let _reload = mutex_lock(&self.reload_guard);
        let candidate = AssetStore::open_dir(&self.directory, &self.trust_store)?;
        self.publish(candidate)
    }

    /// Reloads only if the candidate has an exact expected package-set identity.
    ///
    /// # Errors
    ///
    /// Returns package validation errors, cross-generation contract errors, or
    /// an asset-set hash mismatch.
    pub fn reload_verified(&self, expected: AssetSetHash) -> Result<AssetReload, Error> {
        let _reload = mutex_lock(&self.reload_guard);
        let candidate =
            AssetStore::open_dir_verified(&self.directory, expected, &self.trust_store)?;
        self.publish(candidate)
    }

    fn from_store(directory: PathBuf, trust_store: AssetTrustStore, store: AssetStore) -> Self {
        let contracts = contracts_from_store(&store);
        Self {
            directory,
            trust_store,
            reload_guard: Mutex::new(()),
            current: RwLock::new(ManagerState {
                snapshot: AssetStoreSnapshot {
                    generation: AssetGeneration::INITIAL,
                    store: Arc::new(store),
                },
                contracts,
            }),
        }
    }

    fn publish(&self, candidate: AssetStore) -> Result<AssetReload, Error> {
        let previous = self.snapshot();
        let previous_hash = previous.asset_set_hash();
        if candidate.asset_set_hash() == previous_hash {
            return Ok(AssetReload {
                status: AssetReloadStatus::Unchanged,
                previous_asset_set_hash: previous_hash,
                snapshot: previous,
                changes: AssetChangeSet::default(),
            });
        }

        validate_historical_contracts(&read_lock(&self.current).contracts, &candidate)?;
        let changes = compare_stores(previous.store(), &candidate);
        let snapshot = AssetStoreSnapshot {
            generation: previous.generation().next()?,
            store: Arc::new(candidate),
        };
        let mut state = write_lock(&self.current);
        extend_contracts(&mut state.contracts, snapshot.store());
        state.snapshot = snapshot.clone();
        Ok(AssetReload {
            status: AssetReloadStatus::Reloaded,
            previous_asset_set_hash: previous_hash,
            snapshot,
            changes,
        })
    }
}

fn canonical_directory(path: &Path) -> Result<PathBuf, Error> {
    std::fs::canonicalize(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn compare_stores(previous: &AssetStore, candidate: &AssetStore) -> AssetChangeSet {
    let previous = winning_records(previous);
    let candidate = winning_records(candidate);
    let mut changes = Vec::new();

    for (id, previous_record) in &previous {
        match candidate.get(id) {
            None => changes.push(change_from_record(
                previous_record,
                AssetChangeKind::Removed,
            )),
            Some(candidate_record) => {
                if previous_record != candidate_record {
                    changes.push(change_from_record(
                        candidate_record,
                        AssetChangeKind::Modified,
                    ));
                }
            }
        }
    }
    for (id, candidate_record) in candidate {
        if !previous.contains_key(id) {
            changes.push(change_from_record(candidate_record, AssetChangeKind::Added));
        }
    }
    changes.sort_by(|left, right| left.id.cmp(&right.id));
    AssetChangeSet { changes }
}

fn winning_records(store: &AssetStore) -> BTreeMap<&AssetId, &AssetRecord> {
    let mut winners = BTreeMap::new();
    for package in store.packages() {
        for record in &package.catalog().assets {
            winners.insert(&record.id, record);
        }
    }
    winners
}

fn contracts_from_store(store: &AssetStore) -> BTreeMap<AssetId, (AssetKind, AssetAudience)> {
    winning_records(store)
        .into_values()
        .map(|record| (record.id.clone(), (record.kind, record.audience)))
        .collect()
}

fn validate_historical_contracts(
    contracts: &BTreeMap<AssetId, (AssetKind, AssetAudience)>,
    candidate: &AssetStore,
) -> Result<(), Error> {
    for record in winning_records(candidate).into_values() {
        if let Some((kind, audience)) = contracts.get(&record.id).copied()
            && (kind != record.kind || audience != record.audience)
        {
            return Err(Error::HotReloadReclassification {
                asset: record.id.clone(),
                expected_kind: kind,
                expected_audience: audience,
                actual_kind: record.kind,
                actual_audience: record.audience,
            });
        }
    }
    Ok(())
}

fn extend_contracts(
    contracts: &mut BTreeMap<AssetId, (AssetKind, AssetAudience)>,
    candidate: &AssetStore,
) {
    for record in winning_records(candidate).into_values() {
        contracts
            .entry(record.id.clone())
            .or_insert((record.kind, record.audience));
    }
}

fn change_from_record(record: &AssetRecord, change: AssetChangeKind) -> AssetChange {
    AssetChange {
        id: record.id.clone(),
        kind: record.kind,
        audience: record.audience,
        change,
    }
}

fn mutex_lock(value: &Mutex<()>) -> MutexGuard<'_, ()> {
    match value.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn read_lock(value: &RwLock<ManagerState>) -> RwLockReadGuard<'_, ManagerState> {
    match value.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn write_lock(value: &RwLock<ManagerState>) -> RwLockWriteGuard<'_, ManagerState> {
    match value.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/reload.rs"]
mod tests;
