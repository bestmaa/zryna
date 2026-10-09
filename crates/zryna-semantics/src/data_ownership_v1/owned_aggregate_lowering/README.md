# Private owned aggregate lowering

This substantial subarea produces raw M3 function claims for admitted aggregate source shapes.
The enclosing public entry remains [`data_ownership_v1::lower`](../../data_ownership_v1.rs).
[`mod.rs`](mod.rs) keeps the lowerer private; [`driver.rs`](driver.rs) exposes
`lower_private_owned_aggregate_function` only within the enclosing feature.

## Contracts and private areas

Inputs are authenticated source syntax, resolved declarations/type graph, verified layouts,
function catalog and source owner state. Output is raw function/place/cleanup data or semantic
diagnostics. The enclosing semantic entry must still pass those claims through the independent IR
verifier. Preparing a fallible value precedes committing its destination; cleanup and transferred
subtrees must retain their exact program-point state.

| Area | Responsibility |
| --- | --- |
| [shape.rs](shape.rs), [mixed_shape.rs](mixed_shape.rs), [structured_shape.rs](structured_shape.rs) | Decide which closed source shapes this producer admits |
| [constructors.rs](constructors.rs), [constructor_preparation.rs](constructor_preparation.rs), [constructor_resources.rs](constructor_resources.rs) | Aggregate construction and preflight charges |
| [preparation_plan.rs](preparation_plan.rs), [preparation_operations.rs](preparation_operations.rs), [preparation_state.rs](preparation_state.rs) | Plan ordered value preparation, consumption and commit |
| [projection_resolution.rs](projection_resolution.rs), [projection_topology.rs](projection_topology.rs), [partial_transfers.rs](partial_transfers.rs) | Exact static paths, descendant topology and moved masks |
| [assignment_planning.rs](assignment_planning.rs), [assignments.rs](assignments.rs), [clone.rs](clone.rs) | Replacement and clone ownership/cleanup |
| [structured_cfg.rs](structured_cfg.rs), [structured_state.rs](structured_state.rs), [structured_match.rs](structured_match.rs) | Structured branches, joins and active payload state |
| [lexical_indexed_scope.rs](lexical_indexed_scope.rs), [structured_indexed.rs](structured_indexed.rs) | Indexed authority through admitted lexical and CFG shapes |

This remains private semantics → syntax/layout/raw IR, with no frontend or backend dependency.
Independent IR authority, runtime allocation/drop implementation, public ABI/export policy and
filesystem discovery belong elsewhere. Helper names do not authorize arbitrary projection,
generic, borrow or control-flow shapes; admission is determined by the enclosing producer and
verifier. See [the feature map](../README.md) and
[structured ownership contract](../../../../../docs/M3_STRUCTURED_OWNED_CONTROL_FLOW_MATRIX.md).

## Existing focused tests

```sh
cargo test --locked -p zryna-semantics data_ownership_v1::tests::projected_aggregate_assignments
cargo test --locked -p zryna-semantics data_ownership_v1::tests::structured_owned_source
```

The tests live with the enclosing feature: [aggregate assignment](../tests/projected_aggregate_assignments.rs),
[private aggregates](../tests/private_aggregate_lowering.rs),
[structured source](../tests/structured_owned_source.rs), and
[projection checks](../tests/projection_resolution_checks.rs). The complete feature command is
`cargo test --locked -p zryna-semantics data_ownership_v1`.
