//! Headless ordinary-client composition for autonomous Blackflower agents.
//!
//! The crate wires the existing QUIC client, shared client harness, prediction,
//! and navigation runtime without defining an observation schema, policy,
//! planner, or gameplay controls. Those gameplay-specific boundaries remain
//! explicit inputs to [`AgentRuntime`]. Runtime-owned aggregate telemetry and a
//! bounded process-local diagnostic observer make real controller state visible
//! without granting the foreground UI access to live mutable state.

mod diagnostics;
pub mod foreground;
mod runtime;

pub use diagnostics::{
    AgentDescriptor, AgentDiagnosticConfig, AgentDiagnosticError, AgentDiagnosticReceiver,
    AgentDiagnosticRecord, AgentDiagnosticSender, AgentDiagnostics, AgentHealth, AgentId,
    AgentStatusSnapshot, DecisionCandidate, DecisionConstraint, DecisionOutcome, DecisionRecord,
    DiagnosticRecordKind, DiagnosticText, FallbackReason, MemoryEvictionReason, MemoryItemSnapshot,
    MemoryKind, MemoryStatus, NavigationQueryResult, PolicySource, SensoriumAvailability,
    SensoriumChannelKind, SensoriumChannelSnapshot, SensoriumSnapshot, agent_diagnostic_channel,
    initialize_agent_metrics,
};
pub use runtime::{AgentRuntime, AgentRuntimeConfig, AgentRuntimeError};
