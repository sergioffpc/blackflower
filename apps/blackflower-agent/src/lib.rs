//! Headless ordinary-client composition for autonomous Blackflower agents.
//!
//! The crate wires the existing QUIC client, shared client harness, prediction,
//! and navigation runtime without defining an observation schema, policy,
//! planner, or gameplay controls. Those gameplay-specific boundaries remain
//! explicit inputs to [`AgentRuntime`].

pub mod foreground;
mod runtime;

pub use runtime::{AgentRuntime, AgentRuntimeConfig, AgentRuntimeError};
