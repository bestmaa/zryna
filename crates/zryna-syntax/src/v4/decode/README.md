# Bounded JSON decoding

`mod.rs` owns the public `decode_snapshot` entrypoint and unchanged `SyntaxDecodeError`
codes/messages. Admission checks the response byte ceiling, traverses JSON for duplicate keys,
then deserializes the closed raw DTO grammar. `duplicate_keys.rs` also rejects trailing input;
`collections.rs` supplies the typed serde vector visitors, including one-extra-element rejection
without allocating beyond the declared collection ceiling.

Dependencies are serde/serde_json, raw DTOs, v4 limits and the source-file limit. Decoding returns
untrusted syntax and grants no verified constructor. Collection visitors are visible only
inside v4; duplicate-key traversal remains an internal implementation detail.

Exact tests: `cargo test --locked -p zryna-syntax v4::tests::wire` runs
`golden_decodes_verifies_and_remains_source_bound`,
`adapter_shorthand_fixture_decodes_and_verifies_end_to_end` and
`decoder_is_exact_closed_and_bounded`.
