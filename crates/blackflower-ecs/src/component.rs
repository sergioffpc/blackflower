use bytemuck::Pod;

/// A plain-data Rust type that can be stored directly in Flecs.
///
/// Implementations must also implement [`Pod`], which requires a stable,
/// padding-free representation without references or drop glue. Prefer
/// `#[derive(Component)]` instead of implementing this trait manually.
pub trait Component: Pod + Send + Sync + 'static {
    /// Stable name used to register the component in the Flecs world.
    const NAME: &'static str;
}

/// A zero-sized marker that can be registered as a Flecs tag. Prefer
/// `#[derive(Tag)]` instead of implementing this trait manually.
pub trait Tag: 'static {
    /// Stable name used to register the tag in the Flecs world.
    const NAME: &'static str;
}
