# Untrusted wire models

`mod.rs` gathers the original public DTO names for re-export through `crate::v4`.
`project.rs` owns source units, identifiers and imports; `types.rs` owns flat type nodes and
nominal data declarations; `body.rs` owns function, block and statement claims;
`expressions.rs` owns expression, initializer and match-arm claims;
`diagnostics.rs` owns provider diagnostics and their untrusted locations.

The models depend on the v4 limits and `decode::collections` bounded serde visitors, serde,
source `UntrustedSpan` and diagnostic `Severity`. Every original serde tag, field, closed-object
annotation and bounded vector hook is retained. Constructing or deserializing these public
values does not authenticate a span, arena edge, source identity or semantic type.

Exact tests: `cargo test --locked -p zryna-syntax v4::tests::wire` covers wire round trips,
closed/duplicate JSON and the existing adapter shorthand fixture;
`cargo test --locked -p zryna-syntax v4::tests::resources` exercises exact raw-vector limits
and the first extra element even when callers bypass decoding.
