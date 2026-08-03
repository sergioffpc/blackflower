mod blast;
mod blosc;
mod boost;
mod common;
mod embree;
mod flatbuffers;
mod flecs;
mod flow;
mod jolt;
mod ktx;
mod luau;
mod mysofa;
mod openvdb;
mod opus;
mod ozz;
mod pffft;
mod recast;
mod slang;
mod steam_audio;
mod tbb;
mod zlib;

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use clap::ValueEnum;

use blackflower_build::{self, CargoProfile, Configuration};
use common::*;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
pub(crate) enum Vendor {
    Blosc,
    Blast,
    Boost,
    Embree,
    Flatbuffers,
    Flecs,
    Flow,
    Jolt,
    Ktx,
    Luau,
    Mysofa,
    Openvdb,
    Opus,
    Ozz,
    Pffft,
    Recast,
    Slang,
    SteamAudio,
    Tbb,
    Zlib,
}

impl Vendor {
    pub(crate) const ALL: &[Self] = &[
        Self::Boost,
        Self::Blast,
        Self::Zlib,
        Self::Blosc,
        Self::Tbb,
        Self::Openvdb,
        Self::Embree,
        Self::Flatbuffers,
        Self::Pffft,
        Self::Mysofa,
        Self::SteamAudio,
        Self::Flecs,
        Self::Flow,
        Self::Jolt,
        Self::Ozz,
        Self::Recast,
        Self::Luau,
        Self::Opus,
        Self::Ktx,
        Self::Slang,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Blosc => "blosc",
            Self::Blast => "blast",
            Self::Boost => "boost",
            Self::Embree => "embree",
            Self::Flatbuffers => "flatbuffers",
            Self::Flecs => "flecs",
            Self::Flow => "flow",
            Self::Jolt => "jolt",
            Self::Ktx => "ktx",
            Self::Luau => "luau",
            Self::Mysofa => "mysofa",
            Self::Openvdb => "openvdb",
            Self::Opus => "opus",
            Self::Ozz => "ozz",
            Self::Pffft => "pffft",
            Self::Recast => "recast",
            Self::Slang => "slang",
            Self::SteamAudio => "steam-audio",
            Self::Tbb => "tbb",
            Self::Zlib => "zlib",
        }
    }

    const fn version(self) -> &'static str {
        match self {
            Self::Blosc => blosc::VERSION,
            Self::Blast => blast::VERSION,
            Self::Boost => boost::VERSION,
            Self::Embree => embree::VERSION,
            Self::Flatbuffers => flatbuffers::VERSION,
            Self::Flecs => flecs::VERSION,
            Self::Flow => flow::VERSION,
            Self::Jolt => jolt::VERSION,
            Self::Ktx => ktx::VERSION,
            Self::Luau => luau::VERSION,
            Self::Mysofa => mysofa::VERSION,
            Self::Openvdb => openvdb::VERSION,
            Self::Opus => opus::VERSION,
            Self::Ozz => ozz::VERSION,
            Self::Pffft => pffft::VERSION,
            Self::Recast => recast::VERSION,
            Self::Slang => slang::VERSION,
            Self::SteamAudio => steam_audio::VERSION,
            Self::Tbb => tbb::VERSION,
            Self::Zlib => zlib::VERSION,
        }
    }

    const fn dependencies(self) -> &'static [Self] {
        match self {
            Self::Mysofa => &[Self::Zlib],
            Self::Openvdb => &[Self::Boost, Self::Blosc, Self::Tbb, Self::Zlib],
            Self::SteamAudio => &[
                Self::Embree,
                Self::Flatbuffers,
                Self::Mysofa,
                Self::Pffft,
                Self::Zlib,
            ],
            Self::Blosc
            | Self::Blast
            | Self::Boost
            | Self::Embree
            | Self::Flatbuffers
            | Self::Flecs
            | Self::Flow
            | Self::Jolt
            | Self::Ktx
            | Self::Luau
            | Self::Opus
            | Self::Ozz
            | Self::Pffft
            | Self::Recast
            | Self::Slang
            | Self::Tbb
            | Self::Zlib => &[],
        }
    }
}

pub(crate) fn build(
    workspace_root: &Path,
    profile: CargoProfile,
    target: Option<String>,
    crt_static: bool,
    vendors: &[Vendor],
) -> anyhow::Result<()> {
    let host = rustc_host()?;
    let target = target.unwrap_or_else(|| host.clone());
    if target != host {
        bail!(
            "shared native vendor prebuilds currently require a native target; host is {host}, requested {target}"
        );
    }
    let configuration = Configuration::new(target, profile, crt_static);
    let native_root = blackflower_build::native_root(workspace_root);
    let _build_lock = acquire_build_lock(&native_root, &configuration)?;
    let mut ordered = Vec::new();
    let mut visited = BTreeSet::new();
    for vendor in vendors {
        add_with_dependencies(*vendor, &mut visited, &mut ordered);
    }
    for vendor in ordered {
        match vendor {
            Vendor::Blosc => blosc::build(workspace_root, &native_root, &configuration)?,
            Vendor::Blast => blast::build(workspace_root, &native_root, &configuration)?,
            Vendor::Boost => boost::build(workspace_root, &native_root, &configuration)?,
            Vendor::Embree => embree::build(workspace_root, &native_root, &configuration)?,
            Vendor::Flatbuffers => {
                flatbuffers::build(workspace_root, &native_root, &configuration)?;
            }
            Vendor::Flecs => flecs::build(workspace_root, &native_root, &configuration)?,
            Vendor::Flow => flow::build(workspace_root, &native_root, &configuration)?,
            Vendor::Jolt => jolt::build(workspace_root, &native_root, &configuration)?,
            Vendor::Ktx => ktx::build(workspace_root, &native_root, &configuration)?,
            Vendor::Luau => luau::build(workspace_root, &native_root, &configuration)?,
            Vendor::Mysofa => mysofa::build(workspace_root, &native_root, &configuration)?,
            Vendor::Openvdb => openvdb::build(workspace_root, &native_root, &configuration)?,
            Vendor::Opus => opus::build(workspace_root, &native_root, &configuration)?,
            Vendor::Ozz => ozz::build(workspace_root, &native_root, &configuration)?,
            Vendor::Pffft => pffft::build(workspace_root, &native_root, &configuration)?,
            Vendor::Recast => recast::build(workspace_root, &native_root, &configuration)?,
            Vendor::Slang => slang::build(workspace_root, &native_root, &configuration)?,
            Vendor::SteamAudio => {
                steam_audio::build(workspace_root, &native_root, &configuration)?;
            }
            Vendor::Tbb => tbb::build(workspace_root, &native_root, &configuration)?,
            Vendor::Zlib => zlib::build(workspace_root, &native_root, &configuration)?,
        }
        println!(
            "prepared {} at {}",
            vendor.name(),
            blackflower_build::vendor_directory(&native_root, &configuration, vendor.name(),)
                .display()
        );
    }
    Ok(())
}

fn acquire_build_lock(native_root: &Path, configuration: &Configuration) -> anyhow::Result<File> {
    let configuration_root = native_root.join(configuration.relative_directory());
    fs::create_dir_all(&configuration_root).with_context(|| {
        format!(
            "failed to create native build directory {}",
            configuration_root.display()
        )
    })?;
    let lock_path = configuration_root.join(".blackflower-native-build.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("failed to open native build lock {}", lock_path.display()))?;

    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            eprintln!(
                "waiting for another native build using {}",
                configuration_root.display()
            );
            lock.lock().with_context(|| {
                format!(
                    "failed to acquire native build lock {}",
                    lock_path.display()
                )
            })?;
        }
        Err(TryLockError::Error(error)) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to acquire native build lock {}",
                    lock_path.display()
                )
            });
        }
    }

    Ok(lock)
}

fn add_with_dependencies(
    vendor: Vendor,
    visited: &mut BTreeSet<Vendor>,
    ordered: &mut Vec<Vendor>,
) {
    if !visited.insert(vendor) {
        return;
    }
    for dependency in vendor.dependencies() {
        add_with_dependencies(*dependency, visited, ordered);
    }
    ordered.push(vendor);
}

fn rustc_host() -> anyhow::Result<String> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsStr::new("rustc").to_os_string());
    let output = Command::new(rustc)
        .arg("-vV")
        .output()
        .context("failed to execute rustc -vV")?;
    if !output.status.success() {
        bail!("rustc -vV failed with {}", output.status);
    }
    let stdout = String::from_utf8(output.stdout).context("rustc -vV did not emit UTF-8")?;
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .context("rustc -vV did not report a host triple")
}

#[cfg(test)]
#[path = "../tests/unit/vendor.rs"]
mod tests;
