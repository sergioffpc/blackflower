use crate::ScriptContext;

/// Immutable input supplied to one policy-script evaluation.
///
/// Observation and capability types are owned by the calling domain. This
/// keeps language values and engine backend objects out of the shared contract.
#[derive(Debug, Clone, Copy)]
pub struct ScriptInvocation<'a, Observation: ?Sized, Capabilities: ?Sized> {
    context: ScriptContext,
    observation: &'a Observation,
    capabilities: &'a Capabilities,
}

impl<'a, Observation: ?Sized, Capabilities: ?Sized>
    ScriptInvocation<'a, Observation, Capabilities>
{
    /// Construct an invocation from authority-owned immutable inputs.
    #[must_use]
    pub const fn new(
        context: ScriptContext,
        observation: &'a Observation,
        capabilities: &'a Capabilities,
    ) -> Self {
        Self {
            context,
            observation,
            capabilities,
        }
    }

    /// Deterministic authority context for this evaluation.
    #[must_use]
    pub const fn context(&self) -> ScriptContext {
        self.context
    }

    /// Immutable, filtered facts visible to the script.
    #[must_use]
    pub const fn observation(&self) -> &'a Observation {
        self.observation
    }

    /// Immutable capability descriptors and policy limits visible to the script.
    #[must_use]
    pub const fn capabilities(&self) -> &'a Capabilities {
        self.capabilities
    }
}
