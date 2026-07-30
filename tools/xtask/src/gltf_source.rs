use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use blackflower_gltf_metadata::Document;

#[derive(Debug)]
pub(crate) struct ValidationSummary {
    pub(crate) sources: usize,
}

pub(crate) fn validate_tree(root: &Path) -> anyhow::Result<ValidationSummary> {
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("asset source directory `{}` does not exist", root.display()))?;
    let mut paths = Vec::new();
    collect_sources(&canonical_root, &mut paths)?;
    paths.sort();
    for path in &paths {
        let _document = Document::open(path)
            .with_context(|| format!("invalid glTF source `{}`", path.display()))?;
    }
    Ok(ValidationSummary {
        sources: paths.len(),
    })
}

fn collect_sources(directory: &Path, output: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let entries = fs::read_dir(directory)
        .with_context(|| format!("failed to read `{}`", directory.display()))?;
    let mut paths = entries
        .map(|entry| entry.map(|value| value.path()))
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to enumerate `{}`", directory.display()))?;
    paths.sort();
    for path in paths {
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to inspect `{}`", path.display()))?;
        if metadata.file_type().is_symlink() {
            if is_gltf_source(&path) {
                bail!("glTF source `{}` cannot be a symlink", path.display());
            }
            continue;
        }
        if metadata.is_dir() {
            collect_sources(&path, output)?;
        } else if metadata.is_file() && is_gltf_source(&path) {
            output.push(path);
        }
    }
    Ok(())
}

fn is_gltf_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("gltf") || extension.eq_ignore_ascii_case("glb")
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::validate_tree;

    #[test]
    fn validates_every_gltf_and_glb_source() -> anyhow::Result<()> {
        let directory = TempDir::new()?;
        let nested = directory.path().join("models");
        fs::create_dir(&nested)?;
        fs::write(nested.join("first.gltf"), br#"{"asset":{"version":"2.0"}}"#)?;
        fs::write(
            nested.join("second.glb"),
            minimal_glb(br#"{"asset":{"version":"2.0"}}"#)?,
        )?;
        fs::write(
            nested.join("ignored.json"),
            br#"{"asset":{"version":"1.0"}}"#,
        )?;

        let summary = validate_tree(directory.path())?;
        assert_eq!(summary.sources, 2);
        Ok(())
    }

    #[test]
    fn rejects_invalid_gltf_before_cooking() -> anyhow::Result<()> {
        let directory = TempDir::new()?;
        fs::write(
            directory.path().join("invalid.gltf"),
            br#"{"asset":{"version":"1.0"}}"#,
        )?;

        assert!(validate_tree(directory.path()).is_err());
        Ok(())
    }

    fn minimal_glb(json: &[u8]) -> anyhow::Result<Vec<u8>> {
        const HEADER_BYTES: usize = 12;
        const CHUNK_HEADER_BYTES: usize = 8;
        const JSON_CHUNK: u32 = 0x4e4f_534a;

        let mut json = json.to_vec();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let total = HEADER_BYTES + CHUNK_HEADER_BYTES + json.len();
        let mut glb = Vec::with_capacity(total);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2_u32.to_le_bytes());
        glb.extend_from_slice(&u32::try_from(total)?.to_le_bytes());
        glb.extend_from_slice(&u32::try_from(json.len())?.to_le_bytes());
        glb.extend_from_slice(&JSON_CHUNK.to_le_bytes());
        glb.extend_from_slice(&json);
        Ok(glb)
    }
}
