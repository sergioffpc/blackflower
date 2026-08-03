#![doc = include_str!("../README.md")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use blackflower_animation::cooking::{inspect_animation_ozz, inspect_skeleton_ozz};
use blackflower_animation_format::{
    AnimationContainer, ClipMarker, ClipMetadata, OzzVersion, SkeletonContainer,
};
use blackflower_gltf_metadata::{
    AdditiveReference, AnimationMetadata, Document, MotionAxis, RootMotionReference,
};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Exact pinned ozz-animation source revision used to build `gltf2ozz`.
pub const OZZ_REVISION: &str = "6cbdc790123aa4731d82e255df187b3a8a808256";
/// Exact ozz-animation release expected by cooked payloads.
pub const OZZ_VERSION: &str = "0.16.0";
/// Versioned Blackflower animation cooking recipe.
pub const COOKER_RECIPE: &str = "blackflower-cooker-animation-v1";

/// Profile-owned ozz compression and sampling settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimationProfile {
    /// Sampling rate in hertz; zero uses the source default.
    pub sampling_rate_hz: f32,
    /// Time between ozz iframe keys.
    pub iframe_interval_seconds: f32,
    /// Whether hierarchical key reduction is enabled.
    pub optimize: bool,
    /// Maximum hierarchy error.
    pub optimization_tolerance: f32,
    /// Distance at which hierarchy error is measured.
    pub optimization_distance: f32,
    /// Root-motion track reduction tolerance.
    pub root_motion_tolerance: f32,
}

impl AnimationProfile {
    /// Validate all profile values before invoking native tooling.
    pub fn validate(self) -> Result<Self, Error> {
        if !self.sampling_rate_hz.is_finite()
            || self.sampling_rate_hz < 0.0
            || !self.iframe_interval_seconds.is_finite()
            || self.iframe_interval_seconds < 0.0
            || !positive(self.optimization_tolerance)
            || !positive(self.optimization_distance)
            || !positive(self.root_motion_tolerance)
        {
            return Err(Error::InvalidProfile);
        }
        Ok(self)
    }
}

/// Cook one named glTF skin into a `.bfskel`.
pub fn cook_skeleton(source: &Path, skin: &str) -> Result<Vec<u8>, Error> {
    validate_skin(source, skin)?;
    let temporary = TempDir::new().map_err(Error::TemporaryDirectory)?;
    let raw_skeleton = temporary.path().join("skeleton.ozz");
    let configuration = json!({
        "skeleton": {
            "filename": path_text(&raw_skeleton)?,
            "import": {
                "enable": true,
                "raw": false,
                "types": {
                    "skeleton": true,
                    "marker": false,
                    "camera": false,
                    "geometry": false,
                    "light": false,
                    "null": false,
                    "any": false
                }
            }
        },
        "animations": []
    });
    run_gltf2ozz(source, temporary.path(), &configuration)?;
    let raw = read_output(&raw_skeleton)?;
    let inspection = inspect_skeleton_ozz(&raw).map_err(Error::InspectSkeleton)?;
    SkeletonContainer::encode(ozz_version()?, inspection.identity, &raw)
        .map_err(Error::EncodeContainer)
}

/// Cook one named glTF animation into a `.bfanim`.
pub fn cook_animation(
    source: &Path,
    clip: &str,
    skeleton_asset: &[u8],
    profile: AnimationProfile,
) -> Result<Vec<u8>, Error> {
    validate_selection_name(clip)?;
    let profile = profile.validate()?;
    let document = Document::open(source).map_err(Error::GltfMetadata)?;
    let metadata = document
        .animation_metadata(clip)
        .map_err(Error::GltfMetadata)?;
    let (skeleton, joint_count) = validate_skeleton_dependency(source, skeleton_asset)?;

    let temporary = TempDir::new().map_err(Error::TemporaryDirectory)?;
    let raw_skeleton = temporary.path().join("skeleton.ozz");
    let raw_animation = temporary.path().join("animation.ozz");
    let raw_motion = temporary.path().join("root-motion.ozz");
    fs::write(&raw_skeleton, skeleton.ozz_skeleton()).map_err(|source| Error::WriteTemporary {
        path: raw_skeleton.clone(),
        source,
    })?;
    let configuration = animation_configuration(
        &raw_skeleton,
        &raw_animation,
        &raw_motion,
        clip,
        &metadata,
        profile,
    )?;
    run_gltf2ozz(source, temporary.path(), &configuration)?;

    let animation_bytes = read_output(&raw_animation)?;
    let duration = validate_animation_payload(&animation_bytes, clip, joint_count, &metadata)?;
    let clip_metadata = runtime_metadata(&metadata, duration)?;
    let root_motion = if metadata.root_motion().enabled() {
        Some(read_output(&raw_motion)?)
    } else {
        None
    };
    AnimationContainer::encode(
        ozz_version()?,
        skeleton.identity(),
        &animation_bytes,
        &clip_metadata,
        root_motion.as_deref(),
    )
    .map_err(Error::EncodeContainer)
}

fn validate_animation_payload(
    bytes: &[u8],
    clip: &str,
    joint_count: usize,
    metadata: &AnimationMetadata,
) -> Result<f32, Error> {
    let inspection = inspect_animation_ozz(bytes).map_err(Error::InspectAnimation)?;
    if inspection.name != clip {
        return Err(Error::ClipNameMismatch);
    }
    if inspection.track_count != joint_count {
        return Err(Error::TrackCountMismatch {
            joints: joint_count,
            tracks: inspection.track_count,
        });
    }
    metadata
        .validate_duration(inspection.duration)
        .map_err(Error::GltfMetadata)?;
    Ok(inspection.duration)
}

fn validate_skeleton_dependency<'a>(
    source: &Path,
    skeleton_asset: &'a [u8],
) -> Result<(SkeletonContainer<'a>, usize), Error> {
    let skeleton =
        SkeletonContainer::decode(skeleton_asset).map_err(Error::DecodeSkeletonContainer)?;
    if skeleton.ozz_version() != ozz_version()? {
        return Err(Error::OzzVersionMismatch);
    }
    let inspection =
        inspect_skeleton_ozz(skeleton.ozz_skeleton()).map_err(Error::InspectSkeleton)?;
    if inspection.identity != skeleton.identity() {
        return Err(Error::SkeletonIdentityMismatch);
    }
    validate_animation_source_skeleton(source, skeleton.identity())?;
    Ok((skeleton, inspection.joint_count))
}

fn animation_configuration(
    skeleton: &Path,
    animation: &Path,
    motion: &Path,
    clip: &str,
    metadata: &AnimationMetadata,
    profile: AnimationProfile,
) -> Result<Value, Error> {
    let root = metadata.root_motion();
    let reference = root_reference(root.reference());
    Ok(json!({
        "skeleton": {
            "filename": path_text(skeleton)?,
            "import": {"enable": false}
        },
        "animations": [{
            "clip": clip,
            "filename": path_text(animation)?,
            "raw": false,
            "additive": metadata.additive().enabled(),
            "additive_reference": additive_reference(metadata.additive().reference()),
            "sampling_rate": profile.sampling_rate_hz,
            "iframe_interval": profile.iframe_interval_seconds,
            "optimize": profile.optimize,
            "optimization_settings": {
                "tolerance": profile.optimization_tolerance,
                "distance": profile.optimization_distance,
                "override": []
            },
            "tracks": {
                "properties": [],
                "motion": {
                    "enable": root.enabled(),
                    "filename": path_text(motion)?,
                    "joint_name": root.joint(),
                    "position": motion_settings(
                        axes(root.translation_axes()),
                        reference,
                        root.remove_from_pose(),
                        root.loop_correction(),
                        profile.root_motion_tolerance,
                    ),
                    "rotation": motion_settings(
                        axes(root.rotation_axes()),
                        reference,
                        root.remove_from_pose(),
                        root.loop_correction(),
                        profile.root_motion_tolerance,
                    )
                }
            }
        }]
    }))
}

fn motion_settings(
    components: String,
    reference: &'static str,
    bake: bool,
    looping: bool,
    tolerance: f32,
) -> Value {
    json!({
        "components": components,
        "reference": reference,
        "bake": bake,
        "loop": looping,
        "raw": false,
        "optimize": true,
        "optimization_tolerance": tolerance
    })
}

fn runtime_metadata(source: &AnimationMetadata, duration: f32) -> Result<ClipMetadata, Error> {
    let markers = source
        .markers()
        .iter()
        .map(|marker| {
            ClipMarker::new(marker.name(), marker.time_seconds() / duration)
                .map_err(Error::EncodeContainer)
        })
        .collect::<Result<Vec<_>, _>>()?;
    ClipMetadata::new(
        source.animation(),
        source.looping(),
        source.additive().enabled(),
        markers,
    )
    .map_err(Error::EncodeContainer)
}

fn run_gltf2ozz(source: &Path, directory: &Path, config: &Value) -> Result<(), Error> {
    let config_path = directory.join("gltf2ozz.json");
    let bytes = serde_json::to_vec_pretty(config).map_err(Error::SerializeConfiguration)?;
    fs::write(&config_path, bytes).map_err(|source| Error::WriteTemporary {
        path: config_path.clone(),
        source,
    })?;
    let output = Command::new(gltf2ozz_path())
        .arg(format!("--file={}", path_text(source)?))
        .arg(format!("--config_file={}", path_text(&config_path)?))
        .arg("--endian=little")
        .arg("--log_level=standard")
        .current_dir(directory)
        .output()
        .map_err(Error::LaunchTool)?;
    if !output.status.success() {
        return Err(Error::ToolFailed {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

fn validate_skin(source: &Path, selected: &str) -> Result<(), Error> {
    validate_selection_name(selected)?;
    let imported = gltf::import(source).map_err(Error::ImportGltf)?;
    let names = imported
        .0
        .skins()
        .map(|skin| skin.name().map(str::to_owned))
        .collect::<Vec<_>>();
    if names.len() != 1 || names[0].as_deref() != Some(selected) {
        return Err(Error::SkinSelection {
            selected: selected.to_owned(),
            available: names.into_iter().flatten().collect(),
        });
    }
    Ok(())
}

fn validate_animation_source_skeleton(
    source: &Path,
    expected: blackflower_animation_format::SkeletonIdentity,
) -> Result<(), Error> {
    let imported = gltf::import(source).map_err(Error::ImportGltf)?;
    let skins = imported
        .0
        .skins()
        .map(|skin| skin.name().map(str::to_owned))
        .collect::<Vec<_>>();
    if skins.is_empty() {
        return Ok(());
    }
    let Some(name) = skins.as_slice().first().and_then(Option::as_deref) else {
        return Err(Error::AmbiguousAnimationSkeleton);
    };
    if skins.len() != 1 {
        return Err(Error::AmbiguousAnimationSkeleton);
    }
    let source_skeleton = cook_skeleton(source, name)?;
    let source_container =
        SkeletonContainer::decode(&source_skeleton).map_err(Error::DecodeSkeletonContainer)?;
    if source_container.identity() != expected {
        return Err(Error::AnimationSkeletonIdentityMismatch);
    }
    Ok(())
}

fn validate_selection_name(name: &str) -> Result<(), Error> {
    if name.is_empty()
        || name.trim() != name
        || name.chars().any(char::is_control)
        || name.contains(['*', '?'])
    {
        Err(Error::InvalidSelectionName)
    } else {
        Ok(())
    }
}

fn read_output(path: &Path) -> Result<Vec<u8>, Error> {
    fs::read(path).map_err(|source| Error::ReadOutput {
        path: path.to_path_buf(),
        source,
    })
}

fn path_text(path: &Path) -> Result<&str, Error> {
    path.to_str()
        .ok_or_else(|| Error::NonUtf8Path(path.to_path_buf()))
}

fn axes(values: &[MotionAxis]) -> String {
    values
        .iter()
        .map(|axis| match axis {
            MotionAxis::X => 'x',
            MotionAxis::Y => 'y',
            MotionAxis::Z => 'z',
        })
        .collect()
}

const fn additive_reference(reference: AdditiveReference) -> &'static str {
    match reference {
        AdditiveReference::Animation => "animation",
        AdditiveReference::Skeleton => "skeleton",
    }
}

const fn root_reference(reference: RootMotionReference) -> &'static str {
    match reference {
        RootMotionReference::Absolute => "absolute",
        RootMotionReference::Skeleton => "skeleton",
        RootMotionReference::Animation => "animation",
    }
}

fn ozz_version() -> Result<OzzVersion, Error> {
    let version = blackflower_animation::ozz_version();
    Ok(OzzVersion::new(
        u16::try_from(version.0).map_err(|_error| Error::OzzVersionMismatch)?,
        u16::try_from(version.1).map_err(|_error| Error::OzzVersionMismatch)?,
        u16::try_from(version.2).map_err(|_error| Error::OzzVersionMismatch)?,
    ))
}

fn gltf2ozz_path() -> &'static Path {
    Path::new(env!("BLACKFLOWER_GLTF2OZZ"))
}

fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

/// Errors produced by deterministic animation cooking.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Profile values are invalid.
    #[error("invalid animation cooking profile")]
    InvalidProfile,
    /// A manifest selection name is unsafe or ambiguous.
    #[error("animation source selection name is invalid")]
    InvalidSelectionName,
    /// The selected skin is not the only named skin in the source.
    #[error("selected skin `{selected}` does not uniquely match source skins {available:?}")]
    SkinSelection {
        /// Requested name.
        selected: String,
        /// Named skins found in the source.
        available: Vec<String>,
    },
    /// A source path cannot be passed portably to the host tool.
    #[error("path `{}` is not UTF-8", .0.display())]
    NonUtf8Path(PathBuf),
    /// glTF import failed.
    #[error("failed to import glTF source")]
    ImportGltf(#[source] gltf::Error),
    /// Typed source metadata failed validation.
    #[error("invalid Blackflower glTF metadata")]
    GltfMetadata(#[source] blackflower_gltf_metadata::Error),
    /// A temporary cooking directory could not be created.
    #[error("failed to create animation cooking directory")]
    TemporaryDirectory(#[source] std::io::Error),
    /// A temporary input or configuration could not be written.
    #[error("failed to write temporary animation file `{}`", path.display())]
    WriteTemporary {
        /// Temporary path.
        path: PathBuf,
        /// Filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// Generated JSON configuration could not be serialized.
    #[error("failed to serialize gltf2ozz configuration")]
    SerializeConfiguration(#[source] serde_json::Error),
    /// The host tool could not be started.
    #[error("failed to launch gltf2ozz")]
    LaunchTool(#[source] std::io::Error),
    /// The host tool rejected the source or configuration.
    #[error("gltf2ozz failed with status {status:?}: {stderr}")]
    ToolFailed {
        /// Process exit code.
        status: Option<i32>,
        /// Captured standard output.
        stdout: String,
        /// Captured standard error.
        stderr: String,
    },
    /// A declared tool output is missing.
    #[error("failed to read animation tool output `{}`", path.display())]
    ReadOutput {
        /// Expected output path.
        path: PathBuf,
        /// Filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// Private skeleton inspection failed.
    #[error("generated ozz skeleton is invalid")]
    InspectSkeleton(#[source] blackflower_animation::Error),
    /// Private animation inspection failed.
    #[error("generated ozz animation is invalid")]
    InspectAnimation(#[source] blackflower_animation::Error),
    /// The selected clip name differs from the tool output.
    #[error("gltf2ozz output clip name does not match the manifest")]
    ClipNameMismatch,
    /// Animation and skeleton joint counts differ.
    #[error("animation has {tracks} tracks but skeleton has {joints} joints")]
    TrackCountMismatch {
        /// Skeleton joints.
        joints: usize,
        /// Animation tracks.
        tracks: usize,
    },
    /// A `.bfskel` cannot be decoded.
    #[error("invalid Blackflower skeleton container")]
    DecodeSkeletonContainer(#[source] blackflower_animation_format::Error),
    /// A Blackflower container cannot be encoded.
    #[error("failed to encode Blackflower animation container")]
    EncodeContainer(#[source] blackflower_animation_format::Error),
    /// The dependency was built for a different ozz version.
    #[error("skeleton container requires another ozz version")]
    OzzVersionMismatch,
    /// The private skeleton differs from its declared identity.
    #[error("skeleton container identity does not match its payload")]
    SkeletonIdentityMismatch,
    /// An animation source contains no unique named skin to validate.
    #[error("animation source skeleton is ambiguous")]
    AmbiguousAnimationSkeleton,
    /// The animation source rig differs from the declared skeleton asset.
    #[error("animation source skeleton identity does not match its dependency")]
    AnimationSkeletonIdentityMismatch,
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
