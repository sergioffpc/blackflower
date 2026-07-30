mod asset_cooker;
mod cook;
mod manifest;
mod profile;
mod texture_cooker;

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Context;
use blackflower_assets::{
    AssetId, AssetSetHash, AssetSigningKey, AssetStore, AssetTrustStore, PackageName,
};
use clap::{Parser, Subcommand};

use crate::cook::{CookRequest, Pipeline};

/// Runs the repository's developer task command.
///
/// # Errors
///
/// Returns an error when command-line parsing or the selected task fails.
pub fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Assets(command) => run_assets(&args.workspace_root, command.command),
    }
}

#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct Args {
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Assets(AssetsArgs),
}

#[derive(Debug, clap::Args)]
struct AssetsArgs {
    #[command(subcommand)]
    command: AssetsCommand,
}

#[derive(Debug, Subcommand)]
enum AssetsCommand {
    Check,
    Cook {
        #[arg(long, default_value = "debug")]
        profile: blackflower_assets::ProfileName,
        #[arg(long, default_value = "pak000")]
        package: PackageName,
        #[arg(long)]
        signing_key: PathBuf,
    },
    Inspect {
        #[arg(long = "dir")]
        directory: PathBuf,
        #[arg(long = "trusted-key", required = true)]
        trusted_keys: Vec<PathBuf>,
        asset: AssetId,
    },
    Verify {
        #[arg(long = "dir")]
        directory: PathBuf,
        #[arg(long = "trusted-key", required = true)]
        trusted_keys: Vec<PathBuf>,
        #[arg(long)]
        expected_set_hash: Option<String>,
    },
}

fn run_assets(workspace_root: &Path, command: AssetsCommand) -> anyhow::Result<()> {
    let pipeline = Pipeline::for_workspace(workspace_root);
    match command {
        AssetsCommand::Check => {
            let checked = pipeline.check()?;
            println!(
                "checked {} profiles, {} assets, and {} packages",
                checked.profiles, checked.assets, checked.packages
            );
        }
        AssetsCommand::Cook {
            profile,
            package,
            signing_key,
        } => {
            let signing_key = load_signing_key(&signing_key)?;
            let cooked = pipeline.cook(&CookRequest {
                profile,
                package,
                signing_key,
            })?;
            println!(
                "cooked {} assets to {}",
                cooked.assets,
                cooked.path.display()
            );
            println!("package hash: {}", cooked.package_hash);
        }
        AssetsCommand::Inspect {
            directory,
            trusted_keys,
            asset,
        } => inspect(&directory, &trusted_keys, &asset)?,
        AssetsCommand::Verify {
            directory,
            trusted_keys,
            expected_set_hash,
        } => {
            verify(&directory, &trusted_keys, expected_set_hash.as_deref())?;
        }
    }
    Ok(())
}

fn inspect(directory: &Path, trusted_keys: &[PathBuf], asset: &AssetId) -> anyhow::Result<()> {
    let trust_store = load_trust_store(trusted_keys)?;
    let store = AssetStore::open_dir(directory, &trust_store)?;
    println!("asset set hash: {}", store.asset_set_hash());
    if let Some(profile) = store.cooking_profile() {
        println!("cooking profile: {} ({})", profile.name, profile.hash);
    }
    let winner = store
        .resolve(asset)
        .with_context(|| format!("asset `{asset}` was not found"))?;
    println!(
        "winner: {} ({}, signer {})",
        winner.package().name(),
        winner.record().content_hash,
        winner.package().signing_key_id()
    );
    for package in store.packages().iter().rev() {
        if let Some(record) = package.catalog().find(asset) {
            println!("candidate: {} ({})", package.name(), record.content_hash);
        }
    }
    Ok(())
}

fn verify(
    directory: &Path,
    trusted_keys: &[PathBuf],
    expected: Option<&str>,
) -> anyhow::Result<()> {
    let trust_store = load_trust_store(trusted_keys)?;
    let store = match expected {
        Some(value) => {
            let expected = AssetSetHash::from_str(value)?;
            AssetStore::open_dir_verified(directory, expected, &trust_store)?
        }
        None => AssetStore::open_dir(directory, &trust_store)?,
    };
    println!(
        "verified {} packages with asset set hash {}",
        store.packages().len(),
        store.asset_set_hash()
    );
    if let Some(profile) = store.cooking_profile() {
        println!("cooking profile: {} ({})", profile.name, profile.hash);
    }
    Ok(())
}

fn load_signing_key(path: &Path) -> anyhow::Result<AssetSigningKey> {
    let pem = fs::read_to_string(path)
        .with_context(|| format!("failed to read signing key `{}`", path.display()))?;
    AssetSigningKey::from_pkcs8_pem(&pem)
        .with_context(|| format!("failed to decode signing key `{}`", path.display()))
}

fn load_trust_store(paths: &[PathBuf]) -> anyhow::Result<AssetTrustStore> {
    let mut trust_store = AssetTrustStore::new();
    for path in paths {
        let pem = fs::read_to_string(path)
            .with_context(|| format!("failed to read trusted key `{}`", path.display()))?;
        trust_store
            .trust_public_key_pem(&pem)
            .with_context(|| format!("failed to decode trusted key `{}`", path.display()))?;
    }
    Ok(trust_store)
}
