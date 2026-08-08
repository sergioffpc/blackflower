/// Maximum accepted client/server position error, in metres.
pub const POSITION_TOLERANCE_METERS: f32 = 0.02;
/// Maximum accepted client/server linear-velocity error, in metres per second.
pub const VELOCITY_TOLERANCE_METERS_PER_SECOND: f32 = 0.05;
/// Maximum accepted shortest-arc orientation error: half a degree in radians.
pub const ORIENTATION_TOLERANCE_RADIANS: f32 = std::f32::consts::PI / 360.0;

const _: () = assert!(POSITION_TOLERANCE_METERS >= 0.005);
const _: () = assert!(VELOCITY_TOLERANCE_METERS_PER_SECOND >= 0.005);
const _: () = assert!(ORIENTATION_TOLERANCE_RADIANS > 0.0);
