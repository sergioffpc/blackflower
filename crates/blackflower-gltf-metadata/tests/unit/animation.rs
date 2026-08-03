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
