# Authoritative workspace contract

`mod.rs` owns the public contract models and the private `load_contract`,
`validate_contract_identity`, `validate_contract_paths` and
`validate_contract_unchanged` entrypoints. It preserves strict deserialization,
canonical identity/output roots, bounded registration, portable component roots and
exact retained-source comparison.

Dependencies: Serde/JSON, controlled reads and portable path predicates from
`filesystem`, and the shared diagnostic accumulator. The public model names remain
reexported from the crate facade; this implementation module is private.

Run `cargo test --locked -p zryna-architecture`. Path/identity tests are in
`src/tests/paths.rs`; oversized input, registration limits and changed-contract tests
are in `src/tests/{controlled_read,scan}.rs`. The path tests exercise canonical
contract roots, identity and required workspace shapes.
