/// Errors produced while extracting Blackflower metadata from glTF sources.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A glTF source file could not be read.
    #[error("failed to read glTF source `{}`", path.display())]
    ReadSource {
        /// Source path supplied to the cooker.
        path: std::path::PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// The `gltf` crate rejected the glTF 2.0 structure.
    #[error("invalid glTF 2.0 document")]
    InvalidGltf(#[source] gltf::Error),
    /// The source targets a glTF version other than 2.0.
    #[error("unsupported glTF asset version `{0}`; expected `2.0`")]
    UnsupportedGltfVersion(String),
    /// A glTF extension is outside Blackflower's explicit cooker allowlist.
    #[error("unsupported glTF extension `{0}`")]
    UnsupportedExtension(String),
    /// An external resource URI is remote, non-portable, or escapes its source directory.
    #[error("invalid external glTF resource URI `{uri}`: {reason}")]
    InvalidExternalResourceUri {
        /// Rejected glTF resource URI.
        uri: String,
        /// Portable containment rule that the URI violated.
        reason: &'static str,
    },
    /// A contained external resource could not be resolved.
    #[error("failed to resolve external glTF resource `{uri}`")]
    ExternalResourceUnavailable {
        /// glTF resource URI.
        uri: String,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// The GLB header or chunk table is incomplete.
    #[error("GLB container is truncated")]
    TruncatedGlb,
    /// The GLB version is not the supported glTF 2.0 container version.
    #[error("unsupported GLB version {0}; expected version 2")]
    UnsupportedGlbVersion(u32),
    /// The GLB header length does not match the supplied bytes.
    #[error("GLB declares {declared} bytes but contains {actual}")]
    GlbLengthMismatch {
        /// Byte length recorded in the GLB header.
        declared: usize,
        /// Actual supplied byte length.
        actual: usize,
    },
    /// The first GLB chunk is not the required JSON chunk.
    #[error("GLB first chunk is not JSON")]
    MissingGlbJson,
    /// A GLB contains more than one JSON chunk.
    #[error("GLB contains multiple JSON chunks")]
    DuplicateGlbJson,
    /// The selected JSON document cannot be decoded.
    #[error("invalid glTF JSON")]
    InvalidJson(#[source] serde_json::Error),
    /// A glTF document must have an object at its root.
    #[error("glTF JSON root must be an object")]
    InvalidRoot,
    /// The glTF animations property is not an array or contains a non-object.
    #[error("glTF animations must be an array of objects")]
    InvalidAnimations,
    /// No animation has the requested stable name.
    #[error("glTF animation `{0}` does not exist")]
    AnimationNotFound(String),
    /// More than one animation has the requested name.
    #[error("glTF animation name `{0}` is ambiguous")]
    DuplicateAnimation(String),
    /// The owned Blackflower metadata does not match its strict schema.
    #[error("animation `{animation}` has invalid Blackflower metadata")]
    InvalidAnimationMetadata {
        /// Animation containing the invalid metadata.
        animation: String,
        /// Deserialization error describing the schema violation.
        #[source]
        source: serde_json::Error,
    },
    /// The metadata uses a schema unknown to this cooker.
    #[error("animation `{animation}` uses Blackflower metadata schema {schema}; expected schema 1")]
    UnsupportedAnimationSchema {
        /// Animation containing the unsupported schema.
        animation: String,
        /// Unsupported schema number.
        schema: u32,
    },
    /// One animation declares an excessive number of markers.
    #[error("animation `{animation}` declares {count} markers; the limit is {limit}")]
    TooManyAnimationMarkers {
        /// Animation containing the excessive marker list.
        animation: String,
        /// Authored marker count.
        count: usize,
        /// Supported marker limit.
        limit: usize,
    },
    /// A marker name is empty, padded, too long, or contains control text.
    #[error("animation `{animation}` marker {index} has an invalid name")]
    InvalidMarkerName {
        /// Animation containing the marker.
        animation: String,
        /// Zero-based marker index in source order.
        index: usize,
    },
    /// A marker time must be finite and non-negative.
    #[error("animation `{animation}` marker {index} has an invalid time")]
    InvalidMarkerTime {
        /// Animation containing the marker.
        animation: String,
        /// Zero-based marker index in source order.
        index: usize,
    },
    /// The same marker name and exact time occur more than once.
    #[error("animation `{animation}` duplicates marker `{name}` at {time_seconds} seconds")]
    DuplicateMarker {
        /// Animation containing the duplicate marker.
        animation: String,
        /// Duplicated marker name.
        name: String,
        /// Duplicated marker time.
        time_seconds: f32,
    },
    /// Root-motion metadata is internally inconsistent.
    #[error("animation `{animation}` has invalid root-motion metadata")]
    InvalidRootMotion {
        /// Animation containing the invalid policy.
        animation: String,
    },
    /// The cooked clip duration is invalid.
    #[error("animation `{animation}` has an invalid cooked duration")]
    InvalidAnimationDuration {
        /// Animation containing the invalid duration.
        animation: String,
    },
    /// A marker occurs after the cooked clip duration.
    #[error(
        "animation `{animation}` marker {index} at {time_seconds} seconds exceeds duration {duration_seconds}"
    )]
    MarkerBeyondDuration {
        /// Animation containing the marker.
        animation: String,
        /// Marker index in deterministic order.
        index: usize,
        /// Marker time.
        time_seconds: f32,
        /// Cooked clip duration.
        duration_seconds: f32,
    },
    /// The glTF nodes property is not an array or contains a non-object.
    #[error("glTF nodes must be an array of objects")]
    InvalidNodes,
    /// No node has the requested stable name.
    #[error("glTF node `{0}` does not exist")]
    NodeNotFound(String),
    /// More than one node has the requested name.
    #[error("glTF node name `{0}` is ambiguous")]
    DuplicateNode(String),
    /// The owned Blackflower node metadata does not match its strict schema.
    #[error("node `{node}` has invalid Blackflower metadata")]
    InvalidNodeMetadata {
        /// Node containing the invalid metadata.
        node: String,
        /// Deserialization error describing the schema violation.
        #[source]
        source: serde_json::Error,
    },
    /// The node metadata uses a schema unknown to this cooker.
    #[error("node `{node}` uses Blackflower metadata schema {schema}; expected schema 1")]
    UnsupportedNodeSchema {
        /// Node containing the unsupported schema.
        node: String,
        /// Unsupported schema number.
        schema: u32,
    },
    /// A node kind is not a bounded lower-snake-case domain type.
    #[error("node `{0}` has an invalid Blackflower kind")]
    InvalidNodeKind(String),
    /// A node ID is empty, padded, too long, or contains control text.
    #[error("node `{0}` has an invalid Blackflower ID")]
    InvalidNodeId(String),
}
