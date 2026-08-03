/// Ordered declarative intents proposed by one script evaluation.
///
/// The host must validate and resolve these proposals before mutating
/// authoritative state.
#[must_use = "script intents must be validated and resolved by the host"]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProposedIntents<Intent> {
    intents: Vec<Intent>,
}

impl<Intent> ProposedIntents<Intent> {
    /// Wrap intents in the deterministic order produced by the runtime.
    pub const fn new(intents: Vec<Intent>) -> Self {
        Self { intents }
    }

    /// Borrow the proposed intents in runtime-produced order.
    #[must_use]
    pub fn as_slice(&self) -> &[Intent] {
        &self.intents
    }

    /// Number of proposed intents.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.intents.len()
    }

    /// Whether the script proposed no intents.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.intents.is_empty()
    }

    /// Consume the wrapper without changing runtime-produced order.
    #[must_use]
    pub fn into_vec(self) -> Vec<Intent> {
        self.intents
    }
}

impl<Intent> From<Vec<Intent>> for ProposedIntents<Intent> {
    fn from(intents: Vec<Intent>) -> Self {
        Self::new(intents)
    }
}
