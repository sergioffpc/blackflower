/// Deterministic authority-owned context for one policy evaluation.
///
/// The host derives the random seed from recorded authoritative inputs. A
/// concrete backend must map it into its RNG without consulting wall-clock or
/// process-global entropy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptContext {
    authoritative_tick: u64,
    random_seed: u64,
}

impl ScriptContext {
    /// Construct the context for one deterministic policy evaluation.
    #[must_use]
    pub const fn new(authoritative_tick: u64, random_seed: u64) -> Self {
        Self {
            authoritative_tick,
            random_seed,
        }
    }

    /// Authoritative simulation tick observed by the script.
    #[must_use]
    pub const fn authoritative_tick(self) -> u64 {
        self.authoritative_tick
    }

    /// Host-derived deterministic random seed for this evaluation.
    #[must_use]
    pub const fn random_seed(self) -> u64 {
        self.random_seed
    }
}
