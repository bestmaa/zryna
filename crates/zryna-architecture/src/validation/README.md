# Workspace validation

`validate_workspace` composes the architecture check in its original order: canonical
root, contract identity/paths, complete bounded scan, component inventory/manifests,
frozen Cargo metadata and input revalidation, phase graph, then contract revalidation.
Every existing early return and halted-diagnostic check is retained.

It depends on the other private crate modules and constructs the existing public
`ValidationReport`. It grants no filesystem or compiler authority to callers.

Run `cargo test --locked -p zryna-architecture`. The unit test
`tests::paths::current_repository_satisfies_the_contract` exercises the public entrypoint. Diagnostic-budget and source-snapshot cases are in
`src/tests/scan.rs`; this module introduces no separate producer or test runner.
