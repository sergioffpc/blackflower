#![doc = include_str!("../README.md")]

mod error;

use std::fs;
use std::path::Path;
use std::process::Command;

use blackflower_rendering_volumes::Vdb;
use bytes::Bytes;
use tempfile::TempDir;

pub use error::Error;

/// Exact OpenVDB source revision compiled into the offline cooker.
pub const OPENVDB_REVISION: &str = "7c03e1f084873cd1b3422c7ff7aec6ee681b3b38";
/// Exact OpenVDB release compiled into the offline cooker.
pub const OPENVDB_VERSION: &str = "13.0.0";
/// NanoVDB format implementation supplied by the pinned OpenVDB source.
pub const NANOVDB_VERSION: &str = "32.9.0";
/// Exact Boost release supplying OpenVDB headers.
pub const BOOST_VERSION: &str = "1.85.0";
/// Exact oneTBB release linked into the cooker tool.
pub const ONE_TBB_VERSION: &str = "2022.1.0";
/// Exact c-blosc release used to read compressed OpenVDB sources.
pub const BLOSC_VERSION: &str = "1.21.6";
/// Exact zlib release used to read compressed OpenVDB sources.
pub const ZLIB_VERSION: &str = "1.3.1";
/// Versioned Blackflower volume cooking recipe.
pub const COOKER_RECIPE: &str =
    "blackflower-cooker-volume-v1;lossless;stats=bbox;checksum=full;codec=none";

/// Converts selected OpenVDB grids into one runtime NanoVDB asset.
///
/// `grids` must be sorted by name and contain no duplicates. The output
/// preserves directly supported grid types, stores only bounds and active
/// counts, writes full checksums, and never applies file compression.
///
/// # Errors
///
/// Returns an error for an invalid selection, unsupported source grid,
/// failed native conversion, or invalid runtime output.
pub fn cook(source: &Path, grids: &[String]) -> Result<Bytes, Error> {
    validate_selection(grids)?;
    let temporary = TempDir::new().map_err(Error::TemporaryDirectory)?;
    let output_path = temporary.path().join("volume.nvdb");
    let mut command = Command::new(cooker_path());
    command.arg("--input").arg(source);
    command.arg("--output").arg(&output_path);
    for grid in grids {
        command.arg("--grid").arg(grid);
    }
    let output = command.output().map_err(Error::Launch)?;
    if !output.status.success() {
        return Err(Error::ToolFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let bytes = fs::read(&output_path).map_err(|source| Error::ReadOutput {
        path: output_path,
        source,
    })?;
    let runtime = Vdb::from_bytes(&bytes).map_err(Error::InvalidOutput)?;
    let output_names = runtime
        .grids()
        .map(|grid| grid.metadata().name())
        .collect::<Vec<_>>();
    if output_names != grids.iter().map(String::as_str).collect::<Vec<_>>() {
        return Err(Error::OutputSelectionMismatch);
    }
    Ok(Bytes::from(bytes))
}

fn validate_selection(grids: &[String]) -> Result<(), Error> {
    if grids.is_empty() {
        return Err(Error::EmptyGridSelection);
    }
    if grids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(Error::NonCanonicalGridSelection);
    }
    Ok(())
}

fn cooker_path() -> &'static str {
    env!("BLACKFLOWER_VDB_COOKER")
}
