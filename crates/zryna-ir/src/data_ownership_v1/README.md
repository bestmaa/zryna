# Data and ownership IR authority

[`data_ownership_v1::verify`](../data_ownership_v1.rs) is the public raw-to-verified boundary.
It accepts untrusted `raw::Program` claims together with independently supplied final source,
entry and `Linear32V1`/`LinuxX8664V1` layout authorities. Success alone constructs the opaque
`VerifiedProgram` and immutable backend views; rejection returns bounded diagnostics.

## Contracts and private areas

Verification binds source identity, spans, type universe, target layouts and scalar ABI, then
checks dense arenas, CFG/type rules, ownership transitions, borrow conflicts and site-bound
cleanup. It derives state from instructions and edges rather than trusting a caller's replay.
The retained runtime contract identity is distinct from the separately verified runtime ABI.

| Area | Responsibility |
| --- | --- |
| [backend_view.rs](backend_view.rs) | Immutable typed views for executable consumers |
| [state_replay.rs](state_replay.rs), [cleanup_references.rs](cleanup_references.rs) | Program-point ownership state and exact cleanup-site references |
| [indexed_borrows.rs](indexed_borrows.rs), [indexed_access.rs](indexed_access.rs), [indexed_binding.rs](indexed_binding.rs) | Closed indexed authority and conflict checks |
| [transient_edges.rs](transient_edges.rs), [weak_upgrade_shape.rs](weak_upgrade_shape.rs) | Continued indexed identities and admitted upgrade shapes |
| [generic_clone.rs](generic_clone.rs), [generic_static_places.rs](generic_static_places.rs), [handle_aware_clone.rs](handle_aware_clone.rs) | Finite clone/static-place authority and handle-aware recipes |

IR depends on source, diagnostics, scalar ABI and layout; semantics produces its raw claims and
backends consume only sealed views. Provider parsing, semantic source-shape admission, runtime
allocation/drop execution, linking and CLI profiles do not belong here. A verifier vocabulary is
not evidence that every source form or target implements it. See
[the ownership IR contract](../../../../docs/M3_DATA_OWNERSHIP_IR.md) and
[structured owned control-flow matrix](../../../../docs/M3_STRUCTURED_OWNED_CONTROL_FLOW_MATRIX.md).

## Existing focused tests

```sh
cargo test --locked -p zryna-ir data_ownership_v1
cargo test --locked -p zryna-ir --doc data_ownership_v1
```

The [test root](tests.rs) includes [hostile CFG authority](tests/cfg_authority_hostile.rs),
[indexed borrow forgery](tests/indexed_borrow_hostile.rs),
[cleanup-bearing calls](tests/indexed_borrow_call_cleanup.rs),
[structured enum rejection](tests/structured_enum_match_hostile.rs), and
[handle-aware clone](tests/handle_aware_clone.rs). Ordinary Cargo runs exclude ignored resource
cases; use their explicitly registered gate when changing those budgets.
