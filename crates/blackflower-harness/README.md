# blackflower-harness

`blackflower-harness` is reserved for the common client-side binding between
canonical inputs, networking, and prediction. A future frontend client will
provide human inputs and a future headless client will provide bot inputs
through the same contract.

The crate deliberately exposes no API yet. Client orchestration, input-source
traits, networking handles, prediction coordination, and client executables
will be designed in a separate change.
