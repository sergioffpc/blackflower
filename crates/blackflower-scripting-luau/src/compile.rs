use crate::{Error, ffi};
use blackflower_assets::{AssetKind, AuthenticatedAsset};

/// Luau bytecode optimization level.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OptimizationLevel {
    /// Disable bytecode optimizations.
    None = 0,
    /// Apply the baseline optimizations that preserve debuggability.
    #[default]
    Baseline = 1,
    /// Apply aggressive optimizations, including inlining.
    Aggressive = 2,
}

/// Debug information emitted into Luau bytecode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum DebugLevel {
    /// Omit debug information.
    None = 0,
    /// Emit line information and function names.
    #[default]
    LineInfo = 1,
    /// Emit full local and upvalue information.
    Full = 2,
}

/// Type information emitted into Luau bytecode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TypeInfoLevel {
    /// Emit type information only for native modules.
    #[default]
    NativeModules = 0,
    /// Emit type information for every module.
    AllModules = 1,
}

/// Coverage counters emitted into Luau bytecode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CoverageLevel {
    /// Disable coverage instrumentation.
    #[default]
    None = 0,
    /// Instrument statements.
    Statements = 1,
    /// Instrument statements and expressions.
    StatementsAndExpressions = 2,
}

/// Supported options for compiling Luau source.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CompileOptions {
    /// Bytecode optimization level.
    pub optimization: OptimizationLevel,
    /// Debug information level.
    pub debug: DebugLevel,
    /// Type information level.
    pub type_info: TypeInfoLevel,
    /// Coverage instrumentation level.
    pub coverage: CoverageLevel,
}

/// Owned bytecode produced by Luau 0.731 or recovered from authenticated content.
///
/// Bytecode is tied to the pinned Luau VM version and must be versioned with
/// cooked content rather than treated as a stable interchange format. Raw
/// bytes cannot construct this type because Luau's native loader is not a
/// memory-safe verifier for untrusted bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBytecode {
    bytes: Box<[u8]>,
    compile_options: CompileOptions,
}

impl VerifiedBytecode {
    fn from_vec(bytes: Vec<u8>, compile_options: CompileOptions) -> Self {
        Self {
            bytes: bytes.into_boxed_slice(),
            compile_options,
        }
    }

    /// Reconstruct bytecode and compiler policy from an authenticated Luau asset.
    ///
    /// The asset must come from a package signed by a key in the application's
    /// [`blackflower_assets::AssetTrustStore`]. Its authenticated catalog must
    /// identify Luau bytecode produced for the exact linked VM toolchain.
    ///
    /// The runtime uses [`CompileOptions::type_info`] to select whether native
    /// codegen is restricted to `--!native` modules or may compile every
    /// loaded module. The caller must obtain these options from the same
    /// authenticated cooking profile identified by the asset.
    pub fn from_authenticated_asset(
        asset: AuthenticatedAsset,
        compile_options: CompileOptions,
    ) -> Result<Self, Error> {
        validate_authenticated_asset(&asset)?;
        Ok(Self {
            bytes: asset.into_bytes().to_vec().into_boxed_slice(),
            compile_options,
        })
    }

    /// Return the encoded Luau bytecode.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consume the wrapper and return its owned encoded bytecode.
    #[must_use]
    pub fn into_bytes(self) -> Box<[u8]> {
        self.bytes
    }

    /// Return the encoded bytecode length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Return whether the encoded bytecode is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Compiler options associated with these bytecode bytes.
    #[must_use]
    pub const fn compile_options(&self) -> CompileOptions {
        self.compile_options
    }
}

/// Compile Luau source with the selected bytecode options.
///
/// Compilation runs outside any [`crate::RuntimeConfig`] VM allocator limit.
/// Compile untrusted source in a separately constrained cooker or worker.
pub fn compile(source: &str, options: CompileOptions) -> Result<VerifiedBytecode, Error> {
    let bytes = ffi::compile(source, options)?;
    if bytes.first() == Some(&0) {
        return Err(Error::Compile(
            String::from_utf8_lossy(&bytes[1..]).into_owned(),
        ));
    }
    Ok(VerifiedBytecode::from_vec(bytes, options))
}

fn validate_authenticated_asset(asset: &AuthenticatedAsset) -> Result<(), Error> {
    validate_asset_contract(asset.record().kind, &asset.toolchain().luau)
}

fn validate_asset_contract(kind: AssetKind, actual_toolchain: &str) -> Result<(), Error> {
    if kind != AssetKind::LuauBytecode {
        return Err(Error::InvalidBytecodeAssetKind { actual: kind });
    }

    let (major, minor, patch) = ffi::luau_version();
    let expected = format!("luau/{major}.{minor}.{patch}");
    if actual_toolchain != expected {
        return Err(Error::IncompatibleBytecodeToolchain {
            expected,
            actual: actual_toolchain.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/compile.rs"]
mod tests;
