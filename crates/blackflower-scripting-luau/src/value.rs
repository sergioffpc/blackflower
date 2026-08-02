/// A value that can cross the initial safe Luau boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Luau `nil`.
    Nil,
    /// Luau boolean.
    Boolean(bool),
    /// Luau double-precision number.
    Number(f64),
    /// Luau 64-bit integer.
    Integer(i64),
    /// Luau byte string. Luau strings are not required to contain UTF-8.
    String(Box<[u8]>),
    /// Luau's default three-component, single-precision vector.
    Vector([f32; 3]),
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.as_bytes().into())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value.into_bytes().into_boxed_slice())
    }
}
