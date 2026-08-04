#![doc = include_str!("../README.md")]

mod asset;
mod effects;
mod error;
mod ffi;
mod hrtf;
mod probe;
mod scene;
mod types;

pub use effects::{
    DirectEffect, PathEffect, PropagationExchange, ReflectionSimulator, ReflectionUpdate,
};
pub use error::Error;
pub use glam::Vec3A;
pub use hrtf::{BinauralEffect, Context, Hrtf, RayTracerBackend};
pub use probe::{
    LoadedProbeBatch, PathBakeSettings, ProbeVolumeTransform, ReflectionsBakeSettings,
};
pub use scene::{AcousticMaterial, AcousticTriangle, InstancedMesh, Scene, StaticMesh};
pub use types::{AudioSettings, BinauralParams, Interpolation, TailState};

/// The Steam Audio SDK version whose source and headers are pinned.
pub const STEAM_AUDIO_VERSION: (u32, u32, u32) = (4, 8, 1);

/// Exact upstream Steam Audio revision linked by the native build.
pub const STEAM_AUDIO_REVISION: &str = "0da18255cca520771f363ee01f100572b39a308e";

/// Recipe identity for serialized static-acoustics scene assets.
pub const STATIC_ACOUSTICS_COOKER_RECIPE: &str = "blackflower-static-acoustics-v1";

/// Authenticated catalog toolchain identity required before native scene load.
#[must_use]
pub fn steam_audio_acoustics_identity() -> String {
    let (major, minor, patch) = STEAM_AUDIO_VERSION;
    format!(
        "steam-audio/{major}.{minor}.{patch}@{STEAM_AUDIO_REVISION};bfac={ACOUSTIC_ASSET_SCHEMA};{STATIC_ACOUSTICS_COOKER_RECIPE}"
    )
}

/// Embree version pinned for Steam Audio on supported x86-64 and ARM64 targets.
pub const EMBREE_VERSION: (u32, u32, u32) = (4, 4, 1);

/// Whether this target includes Steam Audio's statically linked Embree backend.
pub const STEAM_AUDIO_EMBREE_ENABLED: bool = cfg!(blackflower_steam_audio_embree);
pub use asset::{
    ACOUSTIC_ASSET_SCHEMA, AcousticEnvironment, AcousticProbe, AcousticScene, AcousticZone,
    AuthenticatedAcousticScene, BakedDataIdentifier, BakedDataType, BakedDataVariation, BakedLayer,
    ProbeBatch,
};
