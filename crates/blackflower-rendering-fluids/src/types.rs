use std::num::NonZeroU64;
use std::ops::{BitOr, BitOrAssign};

/// Opaque backend-owned resource identifier. Zero is reserved for no resource.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ResourceId(NonZeroU64);

impl ResourceId {
    /// Creates an identifier when `value` is non-zero.
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the renderer-owned numeric identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Optional context capabilities understood by Flow's optimization layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Feature {
    /// A resource may be viewed through a compatible alternate format.
    AliasResourceFormats,
    /// Buffers may be imported/exported through operating-system handles.
    BufferExternalHandle,
    /// A future Flow feature not yet known to this wrapper.
    Unknown(u32),
}

/// Placement and mapping behavior requested for a buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryType {
    /// GPU-local memory.
    Device,
    /// CPU-writable upload memory.
    Upload,
    /// CPU-readable result memory.
    Readback,
    /// A future Flow memory type.
    Unknown(u32),
}

/// Bitset describing Flow buffer use.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BufferUsage(u32);

impl BufferUsage {
    pub const CONSTANT: Self = Self(0x01);
    pub const STRUCTURED: Self = Self(0x02);
    pub const RAW: Self = Self(0x04);
    pub const STORAGE_STRUCTURED: Self = Self(0x08);
    pub const STORAGE_RAW: Self = Self(0x10);
    pub const INDIRECT: Self = Self(0x20);
    pub const COPY_SOURCE: Self = Self(0x40);
    pub const COPY_DESTINATION: Self = Self(0x80);

    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for BufferUsage {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for BufferUsage {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Bitset describing Flow texture use.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextureUsage(u32);

impl TextureUsage {
    pub const SAMPLED: Self = Self(0x01);
    pub const STORAGE: Self = Self(0x02);
    pub const COPY_SOURCE: Self = Self(0x04);
    pub const COPY_DESTINATION: Self = Self(0x08);

    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for TextureUsage {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for TextureUsage {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Exact numeric Flow resource format for adapter capability mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Format(u32);

impl Format {
    pub const UNKNOWN: Self = Self(0);
    pub const RGBA32_FLOAT: Self = Self(1);
    pub const RGBA32_UINT: Self = Self(2);
    pub const RGBA32_SINT: Self = Self(3);
    pub const RGB32_FLOAT: Self = Self(4);
    pub const RGB32_UINT: Self = Self(5);
    pub const RGB32_SINT: Self = Self(6);
    pub const RGBA16_FLOAT: Self = Self(7);
    pub const RGBA16_UNORM: Self = Self(8);
    pub const RGBA16_UINT: Self = Self(9);
    pub const RGBA16_SNORM: Self = Self(10);
    pub const RGBA16_SINT: Self = Self(11);
    pub const RG32_FLOAT: Self = Self(12);
    pub const RG32_UINT: Self = Self(13);
    pub const RG32_SINT: Self = Self(14);
    pub const RGB10A2_UNORM: Self = Self(15);
    pub const RGB10A2_UINT: Self = Self(16);
    pub const RG11B10_FLOAT: Self = Self(17);
    pub const RGBA8_UNORM: Self = Self(18);
    pub const RGBA8_UNORM_SRGB: Self = Self(19);
    pub const RGBA8_UINT: Self = Self(20);
    pub const RGBA8_SNORM: Self = Self(21);
    pub const RGBA8_SINT: Self = Self(22);
    pub const RG16_FLOAT: Self = Self(23);
    pub const RG16_UNORM: Self = Self(24);
    pub const RG16_UINT: Self = Self(25);
    pub const RG16_SNORM: Self = Self(26);
    pub const RG16_SINT: Self = Self(27);
    pub const R32_FLOAT: Self = Self(28);
    pub const R32_UINT: Self = Self(29);
    pub const R32_SINT: Self = Self(30);
    pub const RG8_UNORM: Self = Self(31);
    pub const RG8_UINT: Self = Self(32);
    pub const RG8_SNORM: Self = Self(33);
    pub const RG8_SINT: Self = Self(34);
    pub const R16_FLOAT: Self = Self(35);
    pub const R16_UNORM: Self = Self(36);
    pub const R16_UINT: Self = Self(37);
    pub const R16_SNORM: Self = Self(38);
    pub const R16_SINT: Self = Self(39);
    pub const R8_UNORM: Self = Self(40);
    pub const R8_UINT: Self = Self(41);
    pub const R8_SNORM: Self = Self(42);
    pub const R8_SINT: Self = Self(43);
    pub const BGRA8_UNORM: Self = Self(44);
    pub const BGRA8_UNORM_SRGB: Self = Self(45);

    #[must_use]
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Texture dimensionality requested by Flow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureType {
    OneDimensional,
    TwoDimensional,
    ThreeDimensional,
    Unknown(u32),
}

/// Sampler addressing requested independently for each texture axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressMode {
    Wrap,
    Clamp,
    Mirror,
    BorderZero,
    Unknown(u32),
}

/// Point or linear filtering requested by Flow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterMode {
    Point,
    Linear,
    Unknown(u32),
}

/// Vulkan-style descriptor class emitted by the Flow shader cook.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescriptorType(u32);

impl DescriptorType {
    pub const UNKNOWN: Self = Self(0);
    pub const CONSTANT_BUFFER: Self = Self(1);
    pub const STRUCTURED_BUFFER: Self = Self(2);
    pub const BUFFER: Self = Self(3);
    pub const TEXTURE: Self = Self(4);
    pub const SAMPLER: Self = Self(5);
    pub const RW_STRUCTURED_BUFFER: Self = Self(6);
    pub const RW_BUFFER: Self = Self(7);
    pub const RW_TEXTURE: Self = Self(8);
    pub const TEXTURE_SAMPLER: Self = Self(9);
    pub const INDIRECT_BUFFER: Self = Self(10);
    pub const BUFFER_COPY_SOURCE: Self = Self(11);
    pub const BUFFER_COPY_DESTINATION: Self = Self(12);
    pub const TEXTURE_COPY_SOURCE: Self = Self(13);
    pub const TEXTURE_COPY_DESTINATION: Self = Self(14);

    #[must_use]
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Backend-neutral Flow buffer description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferDesc {
    pub usage: BufferUsage,
    pub format: Format,
    pub structure_stride: u32,
    pub size_in_bytes: u64,
    pub memory_type: MemoryType,
}

/// Backend-neutral Flow texture description.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureDesc {
    pub texture_type: TextureType,
    pub usage: TextureUsage,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mip_levels: u32,
    pub optimized_clear_value: [f32; 4],
}

/// Backend-neutral sampler description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SamplerDesc {
    pub address_mode_u: AddressMode,
    pub address_mode_v: AddressMode,
    pub address_mode_w: AddressMode,
    pub filter_mode: FilterMode,
}

/// One bind-group layout entry reflected from a Flow SPIR-V module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindingDesc {
    pub descriptor_type: DescriptorType,
    pub binding: u32,
    pub descriptor_count: u32,
    pub set: u32,
}

/// Compute pipeline data. Bytecode is SPIR-V because the wrapper reports Vulkan binding metadata.
#[derive(Clone, Copy, Debug)]
pub struct ComputePipelineDesc<'a> {
    pub bindings: &'a [BindingDesc],
    pub bytecode: &'a [u8],
}

/// Resource assigned to one descriptor write for a compute pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceBinding {
    pub descriptor_type: DescriptorType,
    pub binding: u32,
    pub array_index: u32,
    pub set: u32,
    pub buffer: Option<ResourceId>,
    pub texture: Option<ResourceId>,
    pub sampler: Option<ResourceId>,
}

/// Flow compute dispatch to encode in the renderer's current command encoder.
#[derive(Clone, Copy, Debug)]
pub struct ComputePass<'a> {
    pub pipeline: ResourceId,
    pub grid: [u32; 3],
    pub resources: &'a [ResourceBinding],
    pub debug_label: Option<&'a str>,
}

/// Buffer-to-buffer copy requested by Flow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopyBufferPass<'a> {
    pub source: ResourceId,
    pub destination: ResourceId,
    pub source_offset: u64,
    pub destination_offset: u64,
    pub size: u64,
    pub debug_label: Option<&'a str>,
}

/// Buffer/texture copy. The callback method determines the copy direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferTextureCopyPass<'a> {
    pub buffer: ResourceId,
    pub texture: ResourceId,
    pub buffer_offset: u64,
    pub buffer_row_pitch: u32,
    pub buffer_depth_pitch: u32,
    pub mip_level: u32,
    pub offset: [u32; 3],
    pub extent: [u32; 3],
    pub debug_label: Option<&'a str>,
}

/// Texture-to-texture copy requested by Flow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopyTexturePass<'a> {
    pub source: ResourceId,
    pub destination: ResourceId,
    pub source_mip_level: u32,
    pub source_offset: [u32; 3],
    pub destination_mip_level: u32,
    pub destination_offset: [u32; 3],
    pub extent: [u32; 3],
    pub debug_label: Option<&'a str>,
}
