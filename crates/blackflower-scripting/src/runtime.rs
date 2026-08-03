use std::error::Error;

use crate::{ProposedIntents, ScriptInvocation};

/// Language-neutral policy-script execution boundary.
///
/// Implementations are normally role-specific adapters around a concrete VM.
/// They translate typed observations and capabilities into backend values and
/// translate the result back into declarative intents. The trait does not
/// require [`Send`] or [`Sync`], because embedded VMs may be thread-affine.
pub trait ScriptRuntime {
    /// Immutable facts exposed by this policy role.
    type Observation: ?Sized;
    /// Explicit host capabilities exposed by this policy role.
    type Capabilities: ?Sized;
    /// Declarative command proposed by this policy role.
    type Intent;
    /// Backend, binding, or contract failure.
    type Error: Error + 'static;

    /// Evaluate one immutable observation and return declarative proposals.
    fn evaluate(
        &mut self,
        invocation: ScriptInvocation<'_, Self::Observation, Self::Capabilities>,
    ) -> Result<ProposedIntents<Self::Intent>, Self::Error>;
}
