use std::fs;

use serde_json::json;
use tempfile::TempDir;

use super::validate_external_resources;
use crate::Error;

#[test]
fn external_resources_are_confined_to_the_source_directory() -> Result<(), Error> {
    let directory = TempDir::new().map_err(|source| Error::ReadSource {
        path: "temporary glTF directory".into(),
        source,
    })?;
    let source = directory.path().join("model.gltf");
    let buffer = directory.path().join("mesh.bin");
    fs::write(&source, b"{}").map_err(|source| Error::ReadSource {
        path: directory.path().join("model.gltf"),
        source,
    })?;
    fs::write(&buffer, b"mesh").map_err(|source| Error::ReadSource {
        path: buffer,
        source,
    })?;

    validate_external_resources(&source, &json!({"buffers": [{"uri": "mesh.bin"}]}))?;
    assert!(matches!(
        validate_external_resources(&source, &json!({"buffers": [{"uri": "../outside.bin"}]})),
        Err(Error::InvalidExternalResourceUri { .. })
    ));
    Ok(())
}

#[test]
fn file_validation_imports_a_valid_source() -> Result<(), Error> {
    let directory = TempDir::new().map_err(|source| Error::ReadSource {
        path: "temporary glTF directory".into(),
        source,
    })?;
    let source = directory.path().join("model.gltf");
    fs::write(
        &source,
        br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"name":"Root"}]}"#,
    )
    .map_err(|source| Error::ReadSource {
        path: directory.path().join("model.gltf"),
        source,
    })?;
    let root = json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "Root"}]
    });

    super::validate_file(&source, &root)
}
