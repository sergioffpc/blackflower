use blackflower_networking_replication::QuantizationError;

/// Invalid revision-1 application component or movement-control bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    /// One fixed-layout value did not have its exact canonical length.
    #[error("{schema} requires exactly {expected} bytes, received {actual}")]
    InvalidLength {
        /// Stable schema label.
        schema: &'static str,
        /// Exact required byte count.
        expected: usize,
        /// Observed byte count.
        actual: usize,
    },
    /// A component ID is not registered by protocol revision 1.
    #[error("component ID {id} is not registered by protocol revision 1")]
    UnknownComponent {
        /// Unknown non-zero component ID.
        id: u16,
    },
    /// A boolean field was neither zero nor one.
    #[error("{field} must use canonical boolean zero or one")]
    InvalidBoolean {
        /// Stable field label.
        field: &'static str,
    },
    /// An optional-field presence tag was neither zero nor one.
    #[error("{field} must use canonical presence tag zero or one")]
    InvalidPresence {
        /// Stable field label.
        field: &'static str,
    },
    /// An absent input acknowledgement retained non-zero value bytes.
    #[error("an absent acknowledged input must encode a zero sequence")]
    NonCanonicalAbsentInput,
    /// A movement axis used the reserved negative code.
    #[error("movement axes reserve the i16 minimum code")]
    ReservedMovementAxis,
    /// The encoded movement vector exceeded its normalized circular domain.
    #[error("movement axis vector exceeds the normalized circular domain")]
    MovementMagnitude,
    /// A pitch source or code was outside the closed half-turn view domain.
    #[error("view pitch must be finite and between negative and positive pi over two")]
    InvalidViewPitch,
    /// A shared normative quantizer rejected the source or representation.
    #[error(transparent)]
    Quantization(#[from] QuantizationError),
}
