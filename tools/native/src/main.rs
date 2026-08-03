mod vendor;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};

use blackflower_build::CargoProfile;
use vendor::Vendor;

fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    match arguments.command {
        Task::Build {
            profile,
            target,
            crt_static,
            mut vendors,
        } => {
            let workspace_root =
                canonical_workspace_root(&arguments.workspace_root).with_context(|| {
                    format!(
                        "failed to resolve workspace root {}",
                        arguments.workspace_root.display()
                    )
                })?;
            if vendors.is_empty() {
                vendors = Vendor::ALL.to_vec();
            }
            vendor::build(
                &workspace_root,
                profile.into(),
                target,
                crt_static,
                &vendors,
            )
        }
    }
}

fn canonical_workspace_root(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        // Native build tools and Chocolatey shims cannot reliably use verbatim paths as working
        // directories, even though Windows canonicalization returns them.
        Ok(without_windows_verbatim_prefix(canonical))
    }
    #[cfg(not(windows))]
    {
        Ok(canonical)
    }
}

#[cfg(windows)]
fn without_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const VERBATIM_PREFIX: &[u16] = &[92, 92, 63, 92];
    const VERBATIM_UNC_PREFIX: &[u16] = &[92, 92, 63, 92, 85, 78, 67, 92];

    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if let Some(remainder) = units.strip_prefix(VERBATIM_UNC_PREFIX) {
        let mut normalized = vec![92, 92];
        normalized.extend_from_slice(remainder);
        PathBuf::from(OsString::from_wide(&normalized))
    } else if let Some(remainder) = units.strip_prefix(VERBATIM_PREFIX) {
        PathBuf::from(OsString::from_wide(remainder))
    } else {
        path
    }
}

#[derive(Debug, Parser)]
#[command(name = "native")]
struct Arguments {
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,
    #[command(subcommand)]
    command: Task,
}

#[derive(Debug, Subcommand)]
enum Task {
    /// Builds shared static libraries from the repository-level vendor directory.
    Build {
        #[arg(long, value_enum, default_value_t = BuildProfile::Debug)]
        profile: BuildProfile,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        crt_static: bool,
        #[arg(value_enum)]
        vendors: Vec<Vendor>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BuildProfile {
    Debug,
    Release,
}

impl From<BuildProfile> for CargoProfile {
    fn from(value: BuildProfile) -> Self {
        match value {
            BuildProfile::Debug => Self::Debug,
            BuildProfile::Release => Self::Release,
        }
    }
}
