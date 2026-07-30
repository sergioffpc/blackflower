use serde::Deserialize;
use serde_json::Value;

use crate::Error;

/// Current schema for Blackflower metadata attached to a glTF animation.
pub const ANIMATION_METADATA_SCHEMA: u32 = 1;

const MAX_MARKER_NAME_BYTES: usize = 128;
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

/// Validated and deterministically ordered markers for one glTF animation.
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimationMetadataFile {
    schema: u32,
    #[serde(default)]
    markers: Vec<MarkerFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerFile {
    name: String,
    time_seconds: f32,
}

pub(crate) fn markers(root: &Value, animation: &str) -> Result<AnimationMarkers, Error> {
    let source = find_animation(root, animation)?;
    let Some(metadata) = source
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("blackflower"))
    else {
        return Ok(AnimationMarkers {
            animation: animation.to_owned(),
            markers: Box::new([]),
        });
    };
    let file: AnimationMetadataFile =
        serde_json::from_value(metadata.clone()).map_err(|source| {
            Error::InvalidAnimationMetadata {
                animation: animation.to_owned(),
                source,
            }
        })?;
    validate_schema(animation, file.schema)?;
    validate_count(animation, file.markers.len())?;
    let mut markers = file
        .markers
        .into_iter()
        .enumerate()
        .map(|(index, marker)| validate_marker(animation, index, marker))
        .collect::<Result<Vec<_>, _>>()?;
    reject_duplicates(animation, &markers)?;
    markers.sort_by(|left, right| left.time_seconds.total_cmp(&right.time_seconds));
    Ok(AnimationMarkers {
        animation: animation.to_owned(),
        markers: markers.into_boxed_slice(),
    })
}

fn find_animation<'a>(root: &'a Value, name: &str) -> Result<&'a Value, Error> {
    let Some(animations) = root.get("animations") else {
        return Err(Error::AnimationNotFound(name.to_owned()));
    };
    let animations = animations.as_array().ok_or(Error::InvalidAnimations)?;
    let mut matching = animations
        .iter()
        .map(|animation| {
            animation
                .as_object()
                .ok_or(Error::InvalidAnimations)
                .map(|object| (animation, object.get("name").and_then(Value::as_str)))
        })
        .filter_map(|result| match result {
            Ok((animation, Some(candidate))) if candidate == name => Some(Ok(animation)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        });
    let Some(found) = matching.next().transpose()? else {
        return Err(Error::AnimationNotFound(name.to_owned()));
    };
    if matching.next().transpose()?.is_some() {
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
    if marker.name.is_empty()
        || marker.name.len() > MAX_MARKER_NAME_BYTES
        || marker.name.trim() != marker.name
        || marker.name.chars().any(char::is_control)
    {
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

#[cfg(test)]
mod tests {
    use crate::{Document, Error};

    #[test]
    fn markers_are_extracted_and_ordered_by_time() -> Result<(), Error> {
        let document = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Walk",
                    "channels": [],
                    "samplers": [],
                    "extras": {
                        "vendor": {"untouched": true},
                        "blackflower": {
                            "schema": 1,
                            "markers": [
                                {"name": "right_foot", "time_seconds": 0.71},
                                {"name": "left_foot", "time_seconds": 0.24}
                            ]
                        }
                    }
                }]
            }"#,
        )?;
        let markers = document.animation_markers("Walk")?;

        assert_eq!(markers.animation(), "Walk");
        assert_eq!(markers.markers().len(), 2);
        assert_eq!(markers.markers()[0].name(), "left_foot");
        assert_eq!(
            markers.markers()[0].time_seconds().to_bits(),
            0.24_f32.to_bits()
        );
        assert_eq!(markers.markers()[1].name(), "right_foot");
        Ok(())
    }

    #[test]
    fn animation_without_blackflower_metadata_has_no_markers() -> Result<(), Error> {
        let document = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Idle",
                    "channels": [],
                    "samplers": [],
                    "extras": {"vendor": true}
                }]
            }"#,
        )?;
        assert!(document.animation_markers("Idle")?.is_empty());
        Ok(())
    }

    #[test]
    fn duplicate_animation_names_are_ambiguous() -> Result<(), Error> {
        let document = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [
                    {"name": "Idle", "channels": [], "samplers": []},
                    {"name": "Idle", "channels": [], "samplers": []}
                ]
            }"#,
        )?;
        assert!(matches!(
            document.animation_markers("Idle"),
            Err(Error::DuplicateAnimation(name)) if name == "Idle"
        ));
        Ok(())
    }

    #[test]
    fn owned_metadata_is_strict_and_versioned() -> Result<(), Error> {
        let unknown_field = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Idle",
                    "channels": [],
                    "samplers": [],
                    "extras": {"blackflower": {"schema": 1, "marker": []}}
                }]
            }"#,
        )?;
        assert!(matches!(
            unknown_field.animation_markers("Idle"),
            Err(Error::InvalidAnimationMetadata { .. })
        ));

        let unsupported = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Idle",
                    "channels": [],
                    "samplers": [],
                    "extras": {"blackflower": {"schema": 2, "markers": []}}
                }]
            }"#,
        )?;
        assert!(matches!(
            unsupported.animation_markers("Idle"),
            Err(Error::UnsupportedAnimationSchema { schema: 2, .. })
        ));
        Ok(())
    }

    #[test]
    fn invalid_and_duplicate_markers_are_rejected() -> Result<(), Error> {
        let invalid = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Walk",
                    "channels": [],
                    "samplers": [],
                    "extras": {
                        "blackflower": {
                            "schema": 1,
                            "markers": [{"name": " left_foot", "time_seconds": -0.1}]
                        }
                    }
                }]
            }"#,
        )?;
        assert!(matches!(
            invalid.animation_markers("Walk"),
            Err(Error::InvalidMarkerName { index: 0, .. })
        ));

        let duplicate = Document::from_bytes(
            br#"{
                "asset": {"version": "2.0"},
                "animations": [{
                    "name": "Walk",
                    "channels": [],
                    "samplers": [],
                    "extras": {
                        "blackflower": {
                            "schema": 1,
                            "markers": [
                                {"name": "footstep", "time_seconds": 0.25},
                                {"name": "footstep", "time_seconds": 0.25}
                            ]
                        }
                    }
                }]
            }"#,
        )?;
        assert!(matches!(
            duplicate.animation_markers("Walk"),
            Err(Error::DuplicateMarker { .. })
        ));
        Ok(())
    }
}
