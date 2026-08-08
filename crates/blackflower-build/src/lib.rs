use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use strum::IntoStaticStr;

const CONTRACT_SCHEMA: &str = "1";
pub const MANIFEST_FILE: &str = "blackflower-native-vendor.txt";

#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum CargoProfile {
    Debug,
    Release,
}

impl CargoProfile {
    pub fn cli_name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Configuration {
    pub target: String,
    pub cargo_profile: CargoProfile,
    pub cmake_profile: &'static str,
    pub crt_static: bool,
}

impl Configuration {
    pub fn new(target: String, cargo_profile: CargoProfile, crt_static: bool) -> Self {
        let cmake_profile = if target.ends_with("-msvc") {
            "RelWithDebInfo"
        } else {
            match cargo_profile {
                CargoProfile::Debug => "Debug",
                CargoProfile::Release => "Release",
            }
        };
        Self {
            target,
            cargo_profile,
            cmake_profile,
            crt_static,
        }
    }

    pub fn from_cargo_build_script() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let target = env::var("TARGET")?;
        let cargo_profile = if env::var_os("DEBUG").as_deref() == Some("true".as_ref()) {
            CargoProfile::Debug
        } else {
            CargoProfile::Release
        };
        let target_features = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
        let crt_static = target_features
            .split(',')
            .any(|feature| feature == "crt-static");
        Ok(Self::new(target, cargo_profile, crt_static))
    }

    pub const fn runtime_name(&self) -> &'static str {
        if self.crt_static {
            "crt-static"
        } else {
            "crt-dynamic"
        }
    }

    pub fn relative_directory(&self) -> PathBuf {
        PathBuf::from(&self.target)
            .join(self.cmake_profile.to_ascii_lowercase())
            .join(self.runtime_name())
    }

    pub fn build_hint(&self, vendor: &str) -> String {
        let static_crt = if self.crt_static { " --crt-static" } else { "" };
        format!(
            "cargo native build --profile {} --target {}{static_crt} {vendor}",
            self.cargo_profile.cli_name(),
            self.target
        )
    }
}

/// Emits Cargo inputs and the native contract schema used by FFI consumers.
pub fn emit_cargo_directives() {
    println!("cargo:rerun-if-env-changed=BLACKFLOWER_NATIVE_DIR");
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");
    println!(
        "cargo:rustc-check-cfg=cfg(blackflower_native_contract_schema, values(\"{CONTRACT_SCHEMA}\"))"
    );
    println!("cargo:rustc-cfg=blackflower_native_contract_schema=\"{CONTRACT_SCHEMA}\"");
}

pub fn find_workspace_root(start: &Path) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    for directory in start.ancestors() {
        let manifest = directory.join("Cargo.toml");
        if manifest.is_file() && fs::read_to_string(&manifest)?.contains("[workspace]") {
            return Ok(directory.to_path_buf());
        }
    }
    Err(format!(
        "could not find a workspace Cargo.toml above {}",
        start.display()
    )
    .into())
}

pub fn native_root(workspace_root: &Path) -> PathBuf {
    if let Some(path) = env::var_os("BLACKFLOWER_NATIVE_DIR") {
        return absolute_from(workspace_root, PathBuf::from(path));
    }
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map_or_else(
            || workspace_root.join("target"),
            |path| absolute_from(workspace_root, path),
        );
    target_dir.join("native")
}

pub fn vendor_directory(root: &Path, configuration: &Configuration, vendor: &str) -> PathBuf {
    root.join(configuration.relative_directory()).join(vendor)
}

pub fn locate_vendor(
    workspace_root: &Path,
    configuration: &Configuration,
    vendor: &str,
    version: &str,
) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let directory = vendor_directory(&native_root(workspace_root), configuration, vendor);
    let manifest = directory.join(MANIFEST_FILE);
    if !manifest.is_file() {
        return Err(format!(
            "shared native vendor `{vendor}` is not prepared at {}; run `{}` first",
            directory.display(),
            configuration.build_hint(vendor)
        )
        .into());
    }
    validate_manifest(&manifest, configuration, vendor, version)?;
    println!("cargo:rerun-if-changed={}", manifest.display());
    Ok(directory)
}

pub fn locate_from_cargo_build_script(
    manifest_dir: &Path,
    vendor: &str,
    version: &str,
) -> Result<(Configuration, PathBuf, PathBuf), Box<dyn Error + Send + Sync>> {
    let configuration = Configuration::from_cargo_build_script()?;
    let workspace_root = find_workspace_root(manifest_dir)?;
    let directory = locate_vendor(&workspace_root, &configuration, vendor, version)?;
    Ok((configuration, workspace_root, directory))
}

/// Verifies that a prepared vendor artifact was built from the checked-out source revision.
pub fn validate_vendor_source_revision(
    vendor_directory: &Path,
    source_directory: &Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let manifest = vendor_directory.join(MANIFEST_FILE);
    let manifest_contents = fs::read_to_string(&manifest)?;
    let expected = manifest_contents
        .lines()
        .find_map(|line| line.strip_prefix("source_revision="))
        .ok_or_else(|| {
            format!(
                "native vendor manifest {} has no source revision",
                manifest.display()
            )
        })?;
    let source = fs::canonicalize(source_directory)?;
    let top_level = run_git(&source, &["rev-parse", "--show-toplevel"])?;
    let reported_top_level = fs::canonicalize(Path::new(top_level.trim()))?;
    if reported_top_level != source {
        return Err(format!(
            "native vendor source {} resolved to unexpected Git worktree {}",
            source.display(),
            reported_top_level.display()
        )
        .into());
    }
    let actual = run_git(&source, &["rev-parse", "HEAD"])?;
    if actual.trim() != expected {
        return Err(format!(
            "native vendor artifact {} was built from {}, but {} is checked out; rebuild the vendor",
            vendor_directory.display(),
            expected,
            actual.trim()
        )
        .into());
    }
    let git_head = PathBuf::from(run_git(&source, &["rev-parse", "--git-path", "HEAD"])?.trim());
    let git_head = if git_head.is_absolute() {
        git_head
    } else {
        source.join(git_head)
    };
    println!("cargo:rerun-if-changed={}", git_head.display());
    Ok(())
}

/// Records one vendor-specific build contract in the prepared artifact manifest.
pub fn write_vendor_manifest_field(
    vendor_directory: &Path,
    key: &str,
    value: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if key.is_empty() || key.contains(['=', '\n', '\r']) || value.contains(['\n', '\r']) {
        return Err("native vendor manifest field is invalid".into());
    }
    let manifest = vendor_directory.join(MANIFEST_FILE);
    let mut contents = fs::read_to_string(&manifest)?;
    writeln!(contents, "{key}={value}")?;
    fs::write(manifest, contents)?;
    Ok(())
}

/// Requires one vendor-specific build contract from the prepared artifact.
pub fn validate_vendor_manifest_field(
    vendor_directory: &Path,
    key: &str,
    expected: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let manifest = vendor_directory.join(MANIFEST_FILE);
    let contents = fs::read_to_string(&manifest)?;
    let actual = contents
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(candidate, value)| (candidate == key).then_some(value));
    if actual != Some(expected) {
        return Err(format!(
            "native vendor manifest {} has incompatible {key}; rebuild the vendor",
            manifest.display()
        )
        .into());
    }
    Ok(())
}

fn run_git(source: &Path, arguments: &[&str]) -> Result<String, Box<dyn Error + Send + Sync>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(arguments)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .output()?;
    if !output.status.success() {
        return Err(format!("git {} failed in {}", arguments.join(" "), source.display()).into());
    }
    String::from_utf8(output.stdout).map_err(Into::into)
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;

pub fn find_static_library(
    root: &Path,
    configuration: &Configuration,
    unix_name: &str,
    windows_name: &str,
) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let file_name = if configuration.target.contains("windows") {
        format!("{windows_name}.lib")
    } else {
        format!("lib{unix_name}.a")
    };
    for directory in ["lib", "lib64"] {
        let candidate = root.join(directory).join(&file_name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    find_file(root, &file_name)
}

pub fn emit_static_library(library: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    let directory = library
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", library.display()))?;
    let stem = library
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("invalid static library name {}", library.display()))?;
    let name = stem.strip_prefix("lib").unwrap_or(stem);
    println!("cargo:rustc-link-search=native={}", directory.display());
    println!("cargo:rustc-link-lib=static={name}");
    Ok(())
}

pub fn write_manifest(
    directory: &Path,
    configuration: &Configuration,
    vendor: &str,
    version: &str,
    source_revision: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut contents = String::new();
    writeln!(contents, "schema={CONTRACT_SCHEMA}")?;
    writeln!(contents, "vendor={vendor}")?;
    writeln!(contents, "version={version}")?;
    writeln!(contents, "target={}", configuration.target)?;
    writeln!(contents, "cmake_profile={}", configuration.cmake_profile)?;
    writeln!(contents, "crt={}", configuration.runtime_name())?;
    writeln!(contents, "source_revision={source_revision}")?;
    fs::write(directory.join(MANIFEST_FILE), contents)?;
    Ok(())
}

fn validate_manifest(
    path: &Path,
    configuration: &Configuration,
    vendor: &str,
    version: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let contents = fs::read_to_string(path)?;
    let values = contents
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect::<BTreeMap<_, _>>();
    for (key, expected) in [
        ("schema", CONTRACT_SCHEMA),
        ("vendor", vendor),
        ("version", version),
        ("target", configuration.target.as_str()),
        ("cmake_profile", configuration.cmake_profile),
        ("crt", configuration.runtime_name()),
    ] {
        if values.get(key).copied() != Some(expected) {
            return Err(format!(
                "shared native vendor manifest {} has invalid {key}; run `{}` again",
                path.display(),
                configuration.build_hint(vendor)
            )
            .into());
        }
    }
    Ok(())
}

fn absolute_from(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn find_file(root: &Path, file_name: &str) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    if !root.is_dir() {
        return Err(format!("native build directory {} does not exist", root.display()).into());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if let Ok(found) = find_file(&path, file_name) {
                return Ok(found);
            }
        } else if entry.file_name() == OsStr::new(file_name) {
            return Ok(path);
        }
    }
    Err(format!(
        "native build did not produce {file_name} below {}",
        root.display()
    )
    .into())
}
