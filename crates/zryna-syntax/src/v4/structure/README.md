# Claim containment and source order

The private `verify_file_structure` entrypoint complements token authentication. `claims.rs`
owns same-file containment, ordered child sequences and checked arena lookup;
`declarations.rs` checks type/data children; `body.rs` checks function, block and statement
children; `expressions.rs` checks expression, initializer and match-arm children.

Dependencies are raw DTOs, `UntrustedSpan`, normalized paths and the facade's diagnostic
accumulator. These functions compare already claimed ranges; source-map authentication remains
in `verify/`. Missing/out-of-range edges are handled by checked lookups while the owning arena
verifier independently rejects them. Every retained sequence checks its owner containment and
lexical order, preventing fabricated calls/types from using unrelated authentic tokens.

Exact tests: `cargo test --locked -p zryna-syntax v4::tests::source` includes
`source_structure_rejects_fabricated_call_and_type_claims` and source-identity rejection;
`cargo test --locked -p zryna-syntax v4::tests::arenas` includes
`top_level_arrays_allow_source_interleaving_but_reject_duplicate_names`.
