#![doc = include_str!("../README.md")]

mod asset;
mod error;
mod ffi;
mod hrtf;
mod probe;
mod scene;
mod types;

pub use error::Error;
pub use glam::Vec3A;
pub use hrtf::{BinauralEffect, Context, Hrtf};
pub use probe::{
    LoadedProbeBatch, PathBakeSettings, ProbeVolumeTransform, ReflectionsBakeSettings,
};
pub use scene::{AcousticMaterial, AcousticTriangle, Scene, StaticMesh};
pub use types::{AudioSettings, BinauralParams, Interpolation, TailState};

/// The Steam Audio SDK version whose source and headers are pinned.
pub const STEAM_AUDIO_VERSION: (u32, u32, u32) = (4, 8, 1);
pub use asset::{
    ACOUSTIC_ASSET_SCHEMA, AcousticEnvironment, AcousticProbe, AcousticScene, AcousticZone,
    BakedDataIdentifier, BakedDataType, BakedDataVariation, BakedLayer, ProbeBatch,
};
