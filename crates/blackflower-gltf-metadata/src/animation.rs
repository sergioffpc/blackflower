use serde::Deserialize;
use serde_json::Value;

use crate::Error;

/// Current schema for Blackflower metadata attached to a glTF animation.
pub const ANIMATION_METADATA_SCHEMA: u32 = 1;

const MAX_MARKER_NAME_BYTES: usize = 128;
const MAX_JOINT_NAME_BYTES: usize = 128;
const MAX_ANIMATION_MARKERS: usize = 4_096;

/// One marker authored in seconds on a named glTF animation.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationMarker {
    name: String,
    time_seconds: f32,
}

impl AnimationMarker {
    /// Marker name consumed by later presentation policy.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Authored glTF time in seconds.
    #[must_use]
    pub const fn time_seconds(&self) -> f32 {
        self.time_seconds
    }
}

/// Reference pose used to cook an additive clip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdditiveReference {
    /// First animation keyframe.
    #[default]
    Animation,
    /// Skeleton rest pose.
    Skeleton,
}

/// Authored additive conversion policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdditiveMetadata {
    enabled: bool,
    reference: AdditiveReference,
}

impl AdditiveMetadata {
    /// Whether additive conversion is enabled.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Reference pose used by the offline converter.
    #[must_use]
    pub const fn reference(self) -> AdditiveReference {
        self.reference
    }
}

/// Axis selected for root-motion extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionAxis {
    /// X axis.
    X,
    /// Y axis.
    Y,
    /// Z axis.
    Z,
}

/// Reference used while extracting a root-motion track.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootMotionReference {
    /// Absolute source transform.
    Absolute,
    /// Skeleton rest transform.
    #[default]
    Skeleton,
    /// First animation keyframe.
    Animation,
}

/// Authored root-motion extraction policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootMotionMetadata {
    enabled: bool,
    joint: String,
    translation_axes: Box<[MotionAxis]>,
    rotation_axes: Box<[MotionAxis]>,
    reference: RootMotionReference,
    remove_from_pose: bool,
    loop_correction: bool,
}

impl RootMotionMetadata {
    /// Whether root-motion extraction is enabled.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Joint selected for extraction.
    #[must_use]
    pub fn joint(&self) -> &str {
        &self.joint
    }

    /// Selected translation axes.
    #[must_use]
    pub fn translation_axes(&self) -> &[MotionAxis] {
        &self.translation_axes
    }

    /// Selected rotation axes.
    #[must_use]
    pub fn rotation_axes(&self) -> &[MotionAxis] {
        &self.rotation_axes
    }

    /// Extraction reference.
    #[must_use]
    pub const fn reference(&self) -> RootMotionReference {
        self.reference
    }

    /// Whether extracted motion is removed from the sampled pose.
    #[must_use]
    pub const fn remove_from_pose(&self) -> bool {
        self.remove_from_pose
    }

    /// Whether extraction corrects the last key for a seamless loop.
    #[must_use]
    pub const fn loop_correction(&self) -> bool {
        self.loop_correction
    }
}

/// Validated Blackflower policy authored on one named glTF animation.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationMetadata {
    animation: String,
    looping: bool,
    additive: AdditiveMetadata,
    root_motion: RootMotionMetadata,
    markers: Box<[AnimationMarker]>,
}

impl AnimationMetadata {
    /// Stable glTF animation name.
    #[must_use]
    pub fn animation(&self) -> &str {
        &self.animation
    }

    /// Whether runtime playback loops.
    #[must_use]
    pub const fn looping(&self) -> bool {
        self.looping
    }

    /// Additive cooking policy.
    #[must_use]
    pub const fn additive(&self) -> AdditiveMetadata {
        self.additive
    }

    /// Root-motion extraction policy.
    #[must_use]
    pub const fn root_motion(&self) -> &RootMotionMetadata {
        &self.root_motion
    }

    /// Markers ordered by non-decreasing authored time.
    #[must_use]
    pub fn markers(&self) -> &[AnimationMarker] {
        &self.markers
    }

    /// Validate marker times against the cooked ozz duration.
    pub fn validate_duration(&self, duration_seconds: f32) -> Result<(), Error> {
        if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
            return Err(Error::InvalidAnimationDuration {
                animation: self.animation.clone(),
            });
        }
        if let Some((index, marker)) = self
            .markers
            .iter()
            .enumerate()
            .find(|(_index, marker)| marker.time_seconds > duration_seconds)
        {
            return Err(Error::MarkerBeyondDuration {
                animation: self.animation.clone(),
                index,
                time_seconds: marker.time_seconds,
                duration_seconds,
            });
        }
        Ok(())
    }
}

/// Compatibility marker-only view of [`AnimationMetadata`].
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationMarkers {
    animation: String,
    markers: Box<[AnimationMarker]>,
}

impl AnimationMarkers {
    /// Stable glTF animation name.
    #[must_use]
    pub fn animation(&self) -> &str {
        &self.animation
    }

    /// Markers ordered by non-decreasing authored time.
    #[must_use]
    pub fn markers(&self) -> &[AnimationMarker] {
        &self.markers
    }

    /// Whether no Blackflower markers were authored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.markers.is_empty()
    }
}

impl From<AnimationMetadata> for AnimationMarkers {
    fn from(metadata: AnimationMetadata) -> Self {
        Self {
            animation: metadata.animation,
            markers: metadata.markers,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimationMetadataFile {
    schema: u32,
    #[serde(default, rename = "loop")]
    looping: bool,
    #[serde(default)]
    additive: AdditiveFile,
    #[serde(default)]
    root_motion: RootMotionFile,
    #[serde(default)]
    markers: Vec<MarkerFile>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdditiveFile {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    reference: AdditiveReference,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootMotionFile {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    joint: String,
    #[serde(default = "default_translation_axes")]
    translation_axes: Vec<MotionAxis>,
    #[serde(default = "default_rotation_axes")]
    rotation_axes: Vec<MotionAxis>,
    #[serde(default)]
    reference: RootMotionReference,
    #[serde(default = "default_true")]
    remove_from_pose: bool,
    #[serde(default)]
    loop_correction: bool,
}

impl Default for RootMotionFile {
    fn default() -> Self {
        Self {
            enabled: false,
            joint: String::new(),
            translation_axes: default_translation_axes(),
            rotation_axes: default_rotation_axes(),
            reference: RootMotionReference::default(),
            remove_from_pose: true,
            loop_correction: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerFile {
    name: String,
    time_seconds: f32,
}

pub(crate) fn metadata(root: &Value, animation: &str) -> Result<AnimationMetadata, Error> {
    let source = find_animation(root, animation)?;
    let file = match source
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("blackflower"))
    {
        Some(value) => serde_json::from_value(value.clone()).map_err(|source| {
            Error::InvalidAnimationMetadata {
                animation: animation.to_owned(),
                source,
            }
        })?,
        None => AnimationMetadataFile {
            schema: ANIMATION_METADATA_SCHEMA,
            ..AnimationMetadataFile::default()
        },
    };
    validate_schema(animation, file.schema)?;
    validate_count(animation, file.markers.len())?;
    let mut markers = file
        .markers
        .into_iter()
        .enumerate()
        .map(|(index, marker)| validate_marker(animation, index, marker))
        .collect::<Result<Vec<_>, _>>()?;
    reject_duplicates(animation, &markers)?;
    markers.sort_by(|left, right| {
        left.time_seconds
            .total_cmp(&right.time_seconds)
            .then_with(|| left.name.cmp(&right.name))
    });
    let root_motion = validate_root_motion(animation, file.root_motion)?;
    Ok(AnimationMetadata {
        animation: animation.to_owned(),
        looping: file.looping,
        additive: AdditiveMetadata {
            enabled: file.additive.enabled,
            reference: file.additive.reference,
        },
        root_motion,
        markers: markers.into_boxed_slice(),
    })
}

fn find_animation<'a>(root: &'a Value, name: &str) -> Result<&'a Value, Error> {
    let animations = root
        .get("animations")
        .ok_or_else(|| Error::AnimationNotFound(name.to_owned()))?
        .as_array()
        .ok_or(Error::InvalidAnimations)?;
    let mut matches = animations.iter().filter_map(|animation| {
        animation
            .as_object()
            .and_then(|object| object.get("name"))
            .and_then(Value::as_str)
            .filter(|candidate| *candidate == name)
            .map(|_name| animation)
    });
    let found = matches
        .next()
        .ok_or_else(|| Error::AnimationNotFound(name.to_owned()))?;
    if matches.next().is_some() {
        return Err(Error::DuplicateAnimation(name.to_owned()));
    }
    Ok(found)
}

fn validate_schema(animation: &str, schema: u32) -> Result<(), Error> {
    if schema == ANIMATION_METADATA_SCHEMA {
        Ok(())
    } else {
        Err(Error::UnsupportedAnimationSchema {
            animation: animation.to_owned(),
            schema,
        })
    }
}

fn validate_count(animation: &str, count: usize) -> Result<(), Error> {
    if count <= MAX_ANIMATION_MARKERS {
        Ok(())
    } else {
        Err(Error::TooManyAnimationMarkers {
            animation: animation.to_owned(),
            count,
            limit: MAX_ANIMATION_MARKERS,
        })
    }
}

fn validate_marker(
    animation: &str,
    index: usize,
    marker: MarkerFile,
) -> Result<AnimationMarker, Error> {
    if !valid_text(&marker.name, MAX_MARKER_NAME_BYTES) {
        return Err(Error::InvalidMarkerName {
            animation: animation.to_owned(),
            index,
        });
    }
    if !marker.time_seconds.is_finite() || marker.time_seconds < 0.0 {
        return Err(Error::InvalidMarkerTime {
            animation: animation.to_owned(),
            index,
        });
    }
    Ok(AnimationMarker {
        name: marker.name,
        time_seconds: marker.time_seconds,
    })
}

fn validate_root_motion(
    animation: &str,
    source: RootMotionFile,
) -> Result<RootMotionMetadata, Error> {
    if source.enabled
        && (!valid_text(&source.joint, MAX_JOINT_NAME_BYTES)
            || source.translation_axes.len() + source.rotation_axes.len() == 0)
    {
        return Err(Error::InvalidRootMotion {
            animation: animation.to_owned(),
        });
    }
    if has_duplicate_axes(&source.translation_axes) || has_duplicate_axes(&source.rotation_axes) {
        return Err(Error::InvalidRootMotion {
            animation: animation.to_owned(),
        });
    }
    Ok(RootMotionMetadata {
        enabled: source.enabled,
        joint: source.joint,
        translation_axes: source.translation_axes.into_boxed_slice(),
        rotation_axes: source.rotation_axes.into_boxed_slice(),
        reference: source.reference,
        remove_from_pose: source.remove_from_pose,
        loop_correction: source.loop_correction,
    })
}

fn reject_duplicates(animation: &str, markers: &[AnimationMarker]) -> Result<(), Error> {
    for (index, marker) in markers.iter().enumerate() {
        if markers[..index].iter().any(|candidate| {
            candidate.name == marker.name
                && candidate.time_seconds.to_bits() == marker.time_seconds.to_bits()
        }) {
            return Err(Error::DuplicateMarker {
                animation: animation.to_owned(),
                name: marker.name.clone(),
                time_seconds: marker.time_seconds,
            });
        }
    }
    Ok(())
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn has_duplicate_axes(axes: &[MotionAxis]) -> bool {
    axes.iter()
        .enumerate()
        .any(|(index, axis)| axes[..index].contains(axis))
}

fn default_translation_axes() -> Vec<MotionAxis> {
    vec![MotionAxis::X, MotionAxis::Z]
}

fn default_rotation_axes() -> Vec<MotionAxis> {
    vec![MotionAxis::Y]
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use crate::{AdditiveReference, Document, Error, MotionAxis, RootMotionReference};

    #[test]
    fn complete_animation_metadata_is_typed() -> Result<(), Error> {
        let document = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Walk", "channels": [], "samplers": [],
                    "extras": {"blackflower": {
                        "schema": 1,
                        "loop": true,
                        "additive": {"enabled": true, "reference": "skeleton"},
                        "root_motion": {
                            "enabled": true,
                            "joint": "Root",
                            "translation_axes": ["x", "z"],
                            "rotation_axes": ["y"],
                            "reference": "animation",
                            "remove_from_pose": true,
                            "loop_correction": true
                        },
                        "markers": [
                            {"name": "right", "time_seconds": 0.75},
                            {"name": "left", "time_seconds": 0.25}
                        ]
                    }}
                }]
            }"#,
        )?;
        let metadata = document.animation_metadata("Walk")?;
        assert!(metadata.looping());
        assert!(metadata.additive().enabled());
        assert_eq!(metadata.additive().reference(), AdditiveReference::Skeleton);
        assert_eq!(
            metadata.root_motion().translation_axes(),
            &[MotionAxis::X, MotionAxis::Z]
        );
        assert_eq!(
            metadata.root_motion().reference(),
            RootMotionReference::Animation
        );
        assert_eq!(metadata.markers()[0].name(), "left");
        Ok(())
    }

    #[test]
    fn missing_metadata_uses_disabled_defaults() -> Result<(), Error> {
        let document = Document::from_bytes(
            br#"{"asset":{"version":"2.0"},"animations":[
                {"name":"Idle","channels":[],"samplers":[]}
            ]}"#,
        )?;
        let metadata = document.animation_metadata("Idle")?;
        assert!(!metadata.looping());
        assert!(!metadata.additive().enabled());
        assert!(!metadata.root_motion().enabled());
        assert!(metadata.markers().is_empty());
        Ok(())
    }

    #[test]
    fn invalid_root_motion_and_duration_are_rejected() -> Result<(), Error> {
        let document = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Walk", "channels": [], "samplers": [],
                    "extras": {"blackflower": {
                        "schema": 1,
                        "root_motion": {
                            "enabled": true,
                            "joint": "",
                            "translation_axes": ["x"],
                            "rotation_axes": ["y"]
                        }
                    }}
                }]
            }"#,
        )?;
        assert!(matches!(
            document.animation_metadata("Walk"),
            Err(Error::InvalidRootMotion { .. })
        ));

        let timed = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Walk", "channels": [], "samplers": [],
                    "extras": {"blackflower": {
                        "schema": 1,
                        "markers": [{"name": "late", "time_seconds": 2.0}]
                    }}
                }]
            }"#,
        )?;
        assert!(matches!(
            timed.animation_metadata("Walk")?.validate_duration(1.0),
            Err(Error::MarkerBeyondDuration { .. })
        ));
        Ok(())
    }

    #[test]
    fn duplicate_axes_and_unknown_references_are_rejected() -> Result<(), Error> {
        let duplicate_axes = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Walk", "channels": [], "samplers": [],
                    "extras": {"blackflower": {
                        "schema": 1,
                        "root_motion": {
                            "enabled": true,
                            "joint": "Root",
                            "translation_axes": ["x", "x"],
                            "rotation_axes": []
                        }
                    }}
                }]
            }"#,
        )?;
        assert!(matches!(
            duplicate_axes.animation_metadata("Walk"),
            Err(Error::InvalidRootMotion { .. })
        ));

        let unknown_reference = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Walk", "channels": [], "samplers": [],
                    "extras": {"blackflower": {
                        "schema": 1,
                        "additive": {"enabled": true, "reference": "bind_pose"}
                    }}
                }]
            }"#,
        )?;
        assert!(matches!(
            unknown_reference.animation_metadata("Walk"),
            Err(Error::InvalidAnimationMetadata { .. })
        ));
        Ok(())
    }
}
