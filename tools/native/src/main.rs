mod vendor;

use std::fs;
use std::path::PathBuf;

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
                fs::canonicalize(&arguments.workspace_root).with_context(|| {
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
