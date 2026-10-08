# Data and ownership semantics

The public library entry is [`data_ownership_v1::lower`](../data_ownership_v1.rs).
`SemanticInput::try_new` binds verified protocol-v4 syntax, the exact final `SourceMap`, and an
entry `FileId`. Lowering resolves source names/types and ownership operations, derives both target
layout authorities, then returns a semantic `VerifiedProgram` containing mandatory-verifier-sealed
IR and matching ownership-runtime ABI, or bounded source diagnostics.

## Private areas

| Area | Responsibility |
| --- | --- |
| [import_resolution.rs](import_resolution.rs), [function_catalog.rs](function_catalog.rs) | Module bindings and callable identities |
| [type_model.rs](type_model.rs), [layout_graph.rs](layout_graph.rs) | Source types and independently verified storage layouts |
| [function_dispatch.rs](function_dispatch.rs), [copy_function_lowering.rs](copy_function_lowering.rs), [copy_lowering/](copy_lowering/) | Select closed lowering shapes and emit Copy operations |
| [owned_string_lowering/](owned_string_lowering/), [owned_vec_lowering/](owned_vec_lowering/) | String/Vec source producers and resource checks |
| [owned_aggregate_lowering/](owned_aggregate_lowering/README.md) | Aggregate preparation, projections, transfers and structured ownership |
| [owner_state.rs](owner_state.rs), [owned_cfg_state.rs](owned_cfg_state.rs), [owned_root_borrow_planning.rs](owned_root_borrow_planning.rs) | Ownership and borrow state at source program points |
| [global_resource_limits.rs](global_resource_limits.rs), [diagnostics.rs](diagnostics.rs) | Preflight budgets and deterministic rejection |

Semantics depends on source, syntax, diagnostics, layout, ABI and IR authority; it must not depend
on replaceable frontend providers or backends. Filesystem closure discovery, runtime execution,
target emission, public profile selection and raw-layout acceptance do not belong here. An IR
instruction or private source checkpoint alone does not expand the public profile. Consult the
[M3 public contract](../../../../docs/M3_PUBLIC_PROFILE.md) and
[ownership IR contract](../../../../docs/M3_DATA_OWNERSHIP_IR.md) for their separate boundaries.

## Existing focused tests

```sh
cargo test --locked -p zryna-semantics data_ownership_v1
cargo test --locked -p zryna-semantics --doc data_ownership_v1
```

The [test root](tests.rs) registers feature-specific source fixtures, including
[struct validation](tests/struct_validation.rs), [String behavior](tests/string_core.rs),
[Vec validation](tests/vec_validation.rs), [structured owned source](tests/structured_owned_source.rs),
and [indexed borrow rejection](tests/explicit_indexed_rejections.rs). Ignored proportional tests
require their explicit gate; the ordinary command above does not execute ignored tests.
