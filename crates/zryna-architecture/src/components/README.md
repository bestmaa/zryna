# Registered component inspection

`mod.rs` validates allowed root entries, required root shapes and exact registered
component containers. `paths.rs` owns `validate_paths` and
`validate_component_entries`, including exact allowed immediate-entry policies.
`manifests.rs` owns `validate_members`, `validate_adapters` and bounded TOML/JSON
parsing: root Cargo membership, member identities and the pinned adapter declaration.

Dependencies: contract models, portable/exact filesystem lookup, controlled reads,
TOML/JSON and shared diagnostics. Rust manifest source bytes become Cargo input
snapshots; adapter manifests never acquire compiler or graph authority.

Run `cargo test --locked -p zryna-architecture`. Root inventory, canonical component
spelling and container checks are in `src/tests/paths.rs`; bounded manifest parsing is
in `src/tests/controlled_read.rs`.
`tests::paths::current_repository_satisfies_the_contract` validates the actual checkout
through `validate_workspace`.
