use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, bail};
use blackflower_assets::{CookingProfileIdentity, ProfileHash, ProfileName};
use blackflower_rendering_textures::{EncodeOptions as TextureEncodeOptions, TextureQuality};
use blackflower_scripting::{
    CompileOptions, CoverageLevel, DebugLevel, OptimizationLevel, TypeInfoLevel,
};
use blackflower_shader_compiler::{
    CompileOptions as ShaderCompileOptions, DebugInfoLevel as ShaderDebugInfoLevel,
    OptimizationLevel as ShaderOptimizationLevel, ShaderStage,
};
use serde::{Deserialize, Serialize};

const PROFILE_SCHEMA: u32 = 1;
const PROFILE_HASH_DOMAIN: &[u8] = b"blackflower.cooking-profile.v1";

#[derive(Debug)]
pub(crate) struct CookingProfiles {
    profiles: BTreeMap<ProfileName, CookingProfile>,
}

impl CookingProfiles {
    pub(crate) fn load(root: &Path) -> anyhow::Result<Self> {
        let canonical_root = root.canonicalize().with_context(|| {
            format!(
                "cooking profile directory `{}` does not exist",
                root.display()
            )
        })?;
        let mut paths = fs::read_dir(&canonical_root)
            .with_context(|| {
                format!(
                    "failed to enumerate cooking profiles in `{}`",
                    canonical_root.display()
                )
            })?
            .map(|entry| entry.map(|value| value.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();

        let mut profiles = BTreeMap::new();
        for path in paths {
            let metadata = fs::symlink_metadata(&path)
                .with_context(|| format!("failed to inspect `{}`", path.display()))?;
            if metadata.file_type().is_symlink() {
                if is_profile_path(&path) {
                    bail!("cooking profile `{}` cannot be a symlink", path.display());
                }
                continue;
            }
            if !metadata.is_file() || !is_profile_path(&path) {
                continue;
            }
            let profile = CookingProfile::load(&path)?;
            let name = profile.identity.name.clone();
            if profiles.insert(name.clone(), profile).is_some() {
                bail!("duplicate cooking profile `{name}`");
            }
        }
        if profiles.is_empty() {
            bail!(
                "cooking profile directory `{}` contains no `.toml` profiles",
                canonical_root.display()
            );
        }
        Ok(Self { profiles })
    }

    pub(crate) fn get(&self, name: &ProfileName) -> anyhow::Result<&CookingProfile> {
        self.profiles
            .get(name)
            .with_context(|| format!("cooking profile `{name}` does not exist"))
    }

    pub(crate) fn len(&self) -> usize {
        self.profiles.len()
    }
}

#[derive(Debug)]
pub(crate) struct CookingProfile {
    pub(crate) identity: CookingProfileIdentity,
    pub(crate) scripting: ScriptingProfile,
    pub(crate) shaders: ShaderProfile,
    pub(crate) textures: TextureProfile,
}

impl CookingProfile {
    fn load(path: &Path) -> anyhow::Result<Self> {
        let name = profile_name_from_path(path)?;
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read cooking profile `{}`", path.display()))?;
        let file: CookingProfileFile = toml::from_str(&text)
            .with_context(|| format!("invalid cooking profile `{}`", path.display()))?;
        if file.schema != PROFILE_SCHEMA {
            bail!(
                "unsupported cooking profile schema {} in `{}`",
                file.schema,
                path.display()
            );
        }
        let _options = file
            .textures
            .encode_options()
            .with_context(|| format!("invalid texture settings in profile `{}`", path.display()))?;
        let hash = hash_profile(&file)?;
        Ok(Self {
            identity: CookingProfileIdentity { name, hash },
            scripting: file.scripting,
            shaders: file.shaders,
            textures: file.textures,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CookingProfileFile {
    schema: u32,
    scripting: ScriptingProfile,
    shaders: ShaderProfile,
    textures: TextureProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScriptingProfile {
    pub(crate) luau: LuauProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LuauProfile {
    optimization: LuauOptimization,
    debug: LuauDebug,
    type_info: LuauTypeInfo,
}

impl LuauProfile {
    pub(crate) const fn compile_options(self) -> CompileOptions {
        CompileOptions {
            optimization: match self.optimization {
                LuauOptimization::None => OptimizationLevel::None,
                LuauOptimization::Baseline => OptimizationLevel::Baseline,
                LuauOptimization::Aggressive => OptimizationLevel::Aggressive,
            },
            debug: match self.debug {
                LuauDebug::None => DebugLevel::None,
                LuauDebug::LineInfo => DebugLevel::LineInfo,
                LuauDebug::Full => DebugLevel::Full,
            },
            type_info: match self.type_info {
                LuauTypeInfo::NativeModules => TypeInfoLevel::NativeModules,
                LuauTypeInfo::AllModules => TypeInfoLevel::AllModules,
            },
            coverage: CoverageLevel::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LuauOptimization {
    None,
    Baseline,
    Aggressive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LuauDebug {
    None,
    LineInfo,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LuauTypeInfo {
    NativeModules,
    AllModules,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShaderProfile {
    target: ShaderTarget,
    capability: ShaderCapability,
    optimization: ShaderOptimization,
    debug: ShaderDebug,
}

impl ShaderProfile {
    pub(crate) const fn compile_options(self, stage: ShaderStage) -> ShaderCompileOptions {
        ShaderCompileOptions {
            stage,
            optimization: match self.optimization {
                ShaderOptimization::None => ShaderOptimizationLevel::None,
                ShaderOptimization::Default => ShaderOptimizationLevel::Default,
                ShaderOptimization::High => ShaderOptimizationLevel::High,
                ShaderOptimization::Maximal => ShaderOptimizationLevel::Maximal,
            },
            debug_info: match self.debug {
                ShaderDebug::None => ShaderDebugInfoLevel::None,
                ShaderDebug::Minimal => ShaderDebugInfoLevel::Minimal,
                ShaderDebug::Standard => ShaderDebugInfoLevel::Standard,
                ShaderDebug::Maximal => ShaderDebugInfoLevel::Maximal,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ShaderTarget {
    Spirv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ShaderCapability {
    #[serde(rename = "spirv_1_5")]
    Spirv1_5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ShaderOptimization {
    None,
    Default,
    High,
    Maximal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ShaderDebug {
    None,
    Minimal,
    Standard,
    Maximal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TextureProfile {
    ldr_encoding: TextureLdrEncoding,
    hdr_encoding: TextureHdrEncoding,
    quality: TextureQualityProfile,
    zstd_level: u32,
    generate_mipmaps: bool,
}

impl TextureProfile {
    pub(crate) fn encode_options(self) -> anyhow::Result<TextureEncodeOptions> {
        if !self.generate_mipmaps {
            bail!("texture profiles must generate a complete mip chain");
        }
        if !(1..=22).contains(&self.zstd_level) {
            bail!("texture profile Zstandard level must be from 1 through 22");
        }
        Ok(TextureEncodeOptions {
            quality: match self.quality {
                TextureQualityProfile::Fast => TextureQuality::Fast,
                TextureQualityProfile::High => TextureQuality::High,
            },
            uastc_rdo: self.quality == TextureQualityProfile::High,
            zstd_level: self.zstd_level,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TextureLdrEncoding {
    Uastc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TextureHdrEncoding {
    Rgba16f,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TextureQualityProfile {
    Fast,
    High,
}

fn is_profile_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "toml")
}

fn profile_name_from_path(path: &Path) -> anyhow::Result<ProfileName> {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .context("cooking profile filename must be UTF-8")?;
    ProfileName::from_str(stem).map_err(anyhow::Error::from)
}

fn hash_profile(file: &CookingProfileFile) -> anyhow::Result<ProfileHash> {
    let canonical = serde_json::to_vec(file)?;
    let mut hasher = blake3::Hasher::new();
    hash_field(&mut hasher, PROFILE_HASH_DOMAIN);
    hash_field(&mut hasher, &canonical);
    Ok(ProfileHash::from_bytes(*hasher.finalize().as_bytes()))
}

fn hash_field(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&usize_to_u64(bytes.len()).to_le_bytes());
    hasher.update(bytes);
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
