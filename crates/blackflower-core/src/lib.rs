//! Core engine library — shared by client and server.
//!
//! Provides foundational subsystems, all code here is headless and intended
//! to be deterministic where applicable.

pub mod ecs;
pub mod math;
pub mod time;
