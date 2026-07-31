use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use notify_debouncer_mini::notify::{Error as NotifyError, RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

use crate::{AssetReload, AssetStoreManager, Error};

/// One asynchronous result emitted by an [`AssetStoreWatcher`].
#[non_exhaustive]
#[derive(Debug)]
pub enum AssetWatchEvent {
    /// A relevant filesystem change completed a transactional reload attempt.
    Reloaded(AssetReload),
    /// The package directory changed, but its candidate store was rejected.
    ReloadFailed(Error),
    /// The native filesystem watcher reported an operating-system error.
    WatcherFailed(AssetWatcherError),
}

/// An error produced while starting or running the native filesystem watcher.
#[derive(Debug, thiserror::Error)]
#[error("asset watcher failed: {source}")]
pub struct AssetWatcherError {
    #[source]
    source: NotifyError,
}

impl AssetWatcherError {
    fn new(source: NotifyError) -> Self {
        Self { source }
    }
}

/// Debounced native watcher for one [`AssetStoreManager`] package directory.
///
/// The watcher performs reloads on its backend callback thread and sends the
/// result through [`Self::events`]. Receiving an event never changes when a
/// domain adopts the new snapshot; applications retain that responsibility.
pub struct AssetStoreWatcher {
    directory: PathBuf,
    debounce: Duration,
    events: Receiver<AssetWatchEvent>,
    _debouncer: Debouncer<RecommendedWatcher>,
}

impl std::fmt::Debug for AssetStoreWatcher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AssetStoreWatcher")
            .field("directory", &self.directory)
            .field("debounce", &self.debounce)
            .finish_non_exhaustive()
    }
}

impl AssetStoreWatcher {
    /// Starts watching the manager's package directory without recursion.
    ///
    /// Only direct children with the exact `.squashfs` extension cause a
    /// reload. Multiple relevant notifications in one debounce window cause
    /// one reload attempt.
    ///
    /// # Errors
    ///
    /// Returns an error if the platform watcher cannot be created or attached
    /// to the package directory.
    pub fn watch(
        manager: Arc<AssetStoreManager>,
        debounce: Duration,
    ) -> Result<Self, AssetWatcherError> {
        let directory = manager.directory().to_path_buf();
        let callback_directory = directory.clone();
        let (sender, events) = mpsc::channel();
        let mut debouncer =
            new_debouncer(debounce, move |result: DebounceEventResult| match result {
                Ok(events) => {
                    if events
                        .iter()
                        .any(|event| is_direct_package(&callback_directory, &event.path))
                    {
                        send_reload(&sender, &manager);
                    }
                }
                Err(error) => send_watcher_error(&sender, error),
            })
            .map_err(AssetWatcherError::new)?;
        debouncer
            .watcher()
            .watch(&directory, RecursiveMode::NonRecursive)
            .map_err(AssetWatcherError::new)?;

        Ok(Self {
            directory,
            debounce,
            events,
            _debouncer: debouncer,
        })
    }

    /// Directory observed by the watcher.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Coalescing window used by the watcher.
    #[must_use]
    pub const fn debounce(&self) -> Duration {
        self.debounce
    }

    /// Event channel consumed by development runtime code.
    #[must_use]
    pub const fn events(&self) -> &Receiver<AssetWatchEvent> {
        &self.events
    }
}

fn is_direct_package(directory: &Path, path: &Path) -> bool {
    path.parent() == Some(directory) && path.extension() == Some(OsStr::new("squashfs"))
}

fn send_reload(sender: &Sender<AssetWatchEvent>, manager: &AssetStoreManager) {
    let event = match manager.reload() {
        Ok(reload) => AssetWatchEvent::Reloaded(reload),
        Err(error) => AssetWatchEvent::ReloadFailed(error),
    };
    // A disconnected receiver means the owning runtime dropped the watcher.
    let _send_result = sender.send(event);
}

fn send_watcher_error(sender: &Sender<AssetWatchEvent>, error: NotifyError) {
    // A disconnected receiver means the owning runtime dropped the watcher.
    let _send_result = sender.send(AssetWatchEvent::WatcherFailed(AssetWatcherError::new(
        error,
    )));
}

#[cfg(test)]
#[path = "../tests/unit/watcher.rs"]
mod tests;
