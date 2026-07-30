use std::path::{Component, Path};

use serde_json::Value;

use crate::Error;

/// Exact `gltf` parser version used by the host-side boundary.
pub const GLTF_VERSION: &str = "1.4.1";

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "KHR_materials_unlit",
    "KHR_mesh_quantization",
    "KHR_texture_transform",
];

pub(crate) fn validate_bytes(bytes: &[u8]) -> Result<(), Error> {
    let document = gltf::Gltf::from_slice(bytes).map_err(Error::InvalidGltf)?;
    validate_extensions(&document.document)
}

pub(crate) fn validate_file(path: &Path, root: &Value) -> Result<(), Error> {
    validate_external_resources(path, root)?;
    let (document, _buffers, _images) = gltf::import(path).map_err(Error::InvalidGltf)?;
    validate_extensions(&document)
}

pub(crate) fn validate_root(root: &Value) -> Result<(), Error> {
    let version = root
        .get("asset")
        .and_then(Value::as_object)
        .and_then(|asset| asset.get("version"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if version != "2.0" {
        return Err(Error::UnsupportedGltfVersion(version.to_owned()));
    }
    if let Some(minimum) = root
        .get("asset")
        .and_then(Value::as_object)
        .and_then(|asset| asset.get("minVersion"))
        .and_then(Value::as_str)
        && minimum != "2.0"
    {
        return Err(Error::UnsupportedGltfVersion(minimum.to_owned()));
    }
    Ok(())
}

fn validate_extensions(document: &gltf::Document) -> Result<(), Error> {
    for extension in document.extensions_used() {
        if !SUPPORTED_EXTENSIONS.contains(&extension) {
            return Err(Error::UnsupportedExtension(extension.to_owned()));
        }
    }
    for extension in document.extensions_required() {
        if !SUPPORTED_EXTENSIONS.contains(&extension) {
            return Err(Error::UnsupportedExtension(extension.to_owned()));
        }
    }
    Ok(())
}

fn validate_external_resources(path: &Path, root: &Value) -> Result<(), Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let canonical_parent = std::fs::canonicalize(parent).map_err(|source| Error::ReadSource {
        path: path.to_path_buf(),
        source,
    })?;
    for collection in ["buffers", "images"] {
        let Some(entries) = root.get(collection).and_then(Value::as_array) else {
            continue;
        };
        for entry in entries {
            let Some(uri) = entry
                .as_object()
                .and_then(|object| object.get("uri"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            validate_external_resource(uri, &canonical_parent)?;
        }
    }
    Ok(())
}

fn validate_external_resource(uri: &str, canonical_parent: &Path) -> Result<(), Error> {
    if uri.starts_with("data:") {
        return Ok(());
    }
    let decoded = urlencoding::decode(uri).map_err(|_error| Error::InvalidExternalResourceUri {
        uri: uri.to_owned(),
        reason: "URI is not valid percent-encoded UTF-8",
    })?;
    validate_portable_relative_uri(uri, decoded.as_ref())?;
    let resolved = canonical_parent.join(decoded.as_ref());
    let canonical =
        std::fs::canonicalize(&resolved).map_err(|source| Error::ExternalResourceUnavailable {
            uri: uri.to_owned(),
            source,
        })?;
    if !canonical.starts_with(canonical_parent) {
        return Err(Error::InvalidExternalResourceUri {
            uri: uri.to_owned(),
            reason: "resource escapes the glTF source directory",
        });
    }
    Ok(())
}

fn validate_portable_relative_uri(uri: &str, decoded: &str) -> Result<(), Error> {
    if decoded.is_empty()
        || decoded.contains(['\0', '\\', ':', '?', '#'])
        || Path::new(decoded).components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
    {
        return Err(Error::InvalidExternalResourceUri {
            uri: uri.to_owned(),
            reason: "resource must be a contained portable relative path or data URI",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
