# blackflower-scripting

Backend-neutral policy scripting contracts for Blackflower.

The crate defines the semantic boundary between authoritative Rust systems and
a scripting runtime. A runtime receives an immutable filtered observation,
explicit host capabilities, the authoritative tick, and a host-derived random
seed. It can only return proposed intents. Rust remains responsible for
validating those intents, resolving conflicts, applying authoritative state,
and recording the accepted decisions for replay.

`ScriptRuntime` deliberately does not standardize source text, bytecode,
compiler options, dynamic values, debugger protocols, or resource-budget
units. Those concepts belong to concrete backends such as
`blackflower-scripting-luau`. A role-specific adapter owns the mapping between
typed observations and intents and a backend's value model.

Scripts must not receive Flecs, physics, networking, filesystem, or other
backend objects. Policy evaluation is scheduled at an intentional cadence;
the contract is not an invitation to invoke a VM once per entity at the
authoritative 240 Hz simulation rate.
