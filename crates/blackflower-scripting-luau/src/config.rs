/// Default maximum memory owned by one Luau VM.
pub const DEFAULT_VM_MEMORY_LIMIT_BYTES: usize = 16 * 1024 * 1024;

/// Default number of VM safepoints that one execution may cross.
pub const DEFAULT_EXECUTION_FUEL: u64 = 100_000;

/// Native codegen is disabled unless an explicit executable-memory limit is set.
pub const DEFAULT_NATIVE_CODEGEN_LIMIT_BYTES: usize = 0;

/// Smallest supported executable-memory budget for native codegen.
pub const MIN_NATIVE_CODEGEN_LIMIT_BYTES: usize = 4 * 1024 * 1024;

/// A safe Luau standard library that can be exposed to scripts.
///
/// Filesystem, networking, `os`, `debug`, and module loading are never
/// available through this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Library {
    /// Basic functions such as `assert`, `error`, `ipairs`, and `type`.
    Base,
    /// Coroutine creation and scheduling.
    Coroutine,
    /// Table construction and transformation helpers.
    Table,
    /// Bounded string inspection and transformation helpers.
    ///
    /// Pattern matching, formatting, and binary packing are excluded because
    /// Luau implements them as non-preemptible native calls. The remaining
    /// operations reject string arguments or results larger than 64 KiB.
    String,
    /// Deterministically seeded mathematical helpers.
    Math,
    /// UTF-8 inspection and transformation helpers.
    Utf8,
    /// 32-bit bitwise helpers.
    Bit32,
    /// Luau's mutable byte buffer helpers.
    Buffer,
    /// Luau's vector value and constructor.
    Vector,
    /// Luau's 64-bit integer helpers.
    Integer,
}

impl Library {
    pub(crate) const ALL: [Self; 10] = [
        Self::Base,
        Self::Coroutine,
        Self::Table,
        Self::String,
        Self::Math,
        Self::Utf8,
        Self::Bit32,
        Self::Buffer,
        Self::Vector,
        Self::Integer,
    ];

    const fn mask(self) -> u16 {
        match self {
            Self::Base => 1 << 0,
            Self::Coroutine => 1 << 1,
            Self::Table => 1 << 2,
            Self::String => 1 << 3,
            Self::Math => 1 << 4,
            Self::Utf8 => 1 << 5,
            Self::Bit32 => 1 << 6,
            Self::Buffer => 1 << 7,
            Self::Vector => 1 << 8,
            Self::Integer => 1 << 9,
        }
    }
}

/// Allowlist for the safe standard libraries registered in a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxPolicy {
    libraries: u16,
}

impl SandboxPolicy {
    /// A policy that does not register any standard library.
    #[must_use]
    pub const fn empty() -> Self {
        Self { libraries: 0 }
    }

    /// The default policy, preserving the crate's complete safe library set.
    #[must_use]
    pub const fn standard() -> Self {
        let mut policy = Self::empty();
        let mut index = 0;
        while index < Library::ALL.len() {
            policy = policy.with_library(Library::ALL[index], true);
            index += 1;
        }
        policy
    }

    /// Enable or disable one safe standard library.
    #[must_use]
    pub const fn with_library(mut self, library: Library, enabled: bool) -> Self {
        let mask = library.mask();
        if enabled {
            self.libraries |= mask;
        } else {
            self.libraries &= !mask;
        }
        self
    }

    /// Whether the policy enables a standard library.
    #[must_use]
    pub const fn allows(self, library: Library) -> bool {
        self.libraries & library.mask() != 0
    }
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::standard()
    }
}

/// Resource limits and sandbox policy used to create a [`crate::Runtime`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeConfig {
    random_seed: i32,
    vm_memory_limit_bytes: usize,
    native_codegen_limit_bytes: usize,
    execution_fuel: u64,
    sandbox_policy: SandboxPolicy,
}

impl RuntimeConfig {
    /// Set the deterministic `math.random` seed.
    #[must_use]
    pub const fn with_random_seed(mut self, random_seed: i32) -> Self {
        self.random_seed = random_seed;
        self
    }

    /// Set the maximum number of bytes owned by the Luau VM.
    ///
    /// A zero limit is rejected when the runtime is created.
    #[must_use]
    pub const fn with_vm_memory_limit_bytes(mut self, limit: usize) -> Self {
        self.vm_memory_limit_bytes = limit;
        self
    }

    /// Set the maximum executable-memory budget for Luau native codegen.
    ///
    /// Zero disables native codegen. A non-zero value smaller than
    /// [`MIN_NATIVE_CODEGEN_LIMIT_BYTES`] is rejected when the runtime is
    /// created.
    #[must_use]
    pub const fn with_native_codegen_limit_bytes(mut self, limit: usize) -> Self {
        self.native_codegen_limit_bytes = limit;
        self
    }

    /// Set the VM safepoint fuel restored before every execution.
    ///
    /// A zero budget is rejected when the runtime is created. Fuel measures
    /// interruptible VM safepoints, not individual bytecode instructions.
    #[must_use]
    pub const fn with_execution_fuel(mut self, fuel: u64) -> Self {
        self.execution_fuel = fuel;
        self
    }

    /// Set the allowlist of safe standard libraries.
    #[must_use]
    pub const fn with_sandbox_policy(mut self, policy: SandboxPolicy) -> Self {
        self.sandbox_policy = policy;
        self
    }

    /// The deterministic `math.random` seed.
    #[must_use]
    pub const fn random_seed(self) -> i32 {
        self.random_seed
    }

    /// Maximum number of bytes owned by the Luau VM.
    #[must_use]
    pub const fn vm_memory_limit_bytes(self) -> usize {
        self.vm_memory_limit_bytes
    }

    /// Maximum executable-memory budget for native codegen, or zero when disabled.
    #[must_use]
    pub const fn native_codegen_limit_bytes(self) -> usize {
        self.native_codegen_limit_bytes
    }

    /// VM safepoint fuel restored before every execution.
    #[must_use]
    pub const fn execution_fuel(self) -> u64 {
        self.execution_fuel
    }

    /// Allowlist of safe standard libraries.
    #[must_use]
    pub const fn sandbox_policy(self) -> SandboxPolicy {
        self.sandbox_policy
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            random_seed: 0,
            vm_memory_limit_bytes: DEFAULT_VM_MEMORY_LIMIT_BYTES,
            native_codegen_limit_bytes: DEFAULT_NATIVE_CODEGEN_LIMIT_BYTES,
            execution_fuel: DEFAULT_EXECUTION_FUEL,
            sandbox_policy: SandboxPolicy::standard(),
        }
    }
}

/// Current and peak allocator accounting for one Luau VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryUsage {
    /// Bytes currently owned by the VM allocator.
    pub current_bytes: usize,
    /// Highest allocator usage observed since runtime creation.
    pub peak_bytes: usize,
    /// Configured allocator ceiling.
    pub limit_bytes: usize,
}

/// Native-code generation performed for the most recently loaded chunk.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NativeCodegenStats {
    /// Size of bytecode considered by the code generator.
    pub bytecode_size_bytes: usize,
    /// Executable machine-code bytes emitted.
    pub native_code_size_bytes: usize,
    /// Read-only native data bytes emitted.
    pub native_data_size_bytes: usize,
    /// Metadata bytes retained by the native runtime.
    pub native_metadata_size_bytes: usize,
    /// Luau functions considered for native compilation.
    pub functions_total: u32,
    /// Functions lowered successfully.
    pub functions_compiled: u32,
    /// Functions bound to executable native entry points.
    pub functions_bound: u32,
}
