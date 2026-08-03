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
#[path = "../tests/unit/gltf_source.rs"]
mod tests;
