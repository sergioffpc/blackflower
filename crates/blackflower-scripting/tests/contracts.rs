use std::convert::Infallible;

use blackflower_scripting::{ProposedIntents, ScriptContext, ScriptInvocation, ScriptRuntime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Observation {
    actor: u64,
    score: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Capabilities {
    minimum_score: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Intent {
    actor: u64,
    accepted: bool,
    tick: u64,
    random_seed: u64,
}

struct ThresholdRuntime;

impl ScriptRuntime for ThresholdRuntime {
    type Observation = Observation;
    type Capabilities = Capabilities;
    type Intent = Intent;
    type Error = Infallible;

    fn evaluate(
        &mut self,
        invocation: ScriptInvocation<'_, Self::Observation, Self::Capabilities>,
    ) -> Result<ProposedIntents<Self::Intent>, Self::Error> {
        let context = invocation.context();
        Ok(ProposedIntents::new(vec![Intent {
            actor: invocation.observation().actor,
            accepted: invocation.observation().score >= invocation.capabilities().minimum_score,
            tick: context.authoritative_tick(),
            random_seed: context.random_seed(),
        }]))
    }
}

#[test]
fn runtime_maps_immutable_authority_inputs_to_proposed_intents() {
    let observation = Observation {
        actor: 17,
        score: 9,
    };
    let capabilities = Capabilities { minimum_score: 8 };
    let invocation = ScriptInvocation::new(
        ScriptContext::new(42, 0xDEAD_BEEF),
        &observation,
        &capabilities,
    );

    let intents = match ThresholdRuntime.evaluate(invocation) {
        Ok(intents) => intents,
        Err(error) => match error {},
    };

    assert_eq!(
        intents.as_slice(),
        &[Intent {
            actor: 17,
            accepted: true,
            tick: 42,
            random_seed: 0xDEAD_BEEF,
        }]
    );
}

#[test]
fn proposed_intents_preserve_runtime_order() {
    let intents = ProposedIntents::new(vec![3_u8, 1, 2]);

    assert_eq!(intents.len(), 3);
    assert!(!intents.is_empty());
    assert_eq!(intents.into_vec(), vec![3, 1, 2]);
}

#[test]
fn empty_proposals_are_explicit() {
    let intents = ProposedIntents::<u8>::default();

    assert!(intents.is_empty());
    assert_eq!(intents.as_slice(), &[]);
}
