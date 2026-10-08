# Private scalar self-check command component

The public library entry [`emit_command_self_check`](mod.rs) is re-exported by the backend.
It combines one sealed M1 `VerifiedProgram`, authenticated WIT sources, logical export, typed
`i32` arguments and expected result into an independently audited `ValidatedCommandComponent`.
This library proof does not activate a public command target or grant host capabilities.

## Contracts and private areas

Success retains complete component bytes, the unchanged scalar core, authenticated command world,
invocation and both digests. `revalidate` checks the same verified program and full final-byte
audit again. Unsupported invocations, world substitutions, exceeded envelopes and component
mismatches reject before returning authority.

| Area | Responsibility |
| --- | --- |
| [bridge.rs](bridge.rs), [bridge_audit.rs](bridge_audit.rs) | Closed self-check invocation encoding and independent bridge checks |
| [shell.rs](shell.rs) | Component arrangement around the scalar core and command world |
| [audit.rs](audit.rs) | Independent whole-component authority checks |
| [graph_budget.rs](graph_budget.rs), [type_budget.rs](type_budget.rs) | Bound binary graph/type work before decoding |
| [type_graph.rs](type_graph.rs), [type_indices.rs](type_indices.rs) | Canonical type/reference topology |

Backend → verified IR/ABI and authenticated WIT audit is the dependency direction. Frontend and
semantic lowering, driver composition/approval policy, WASI host callbacks, execution and bundle
publication belong outside this module. Component bytes alone grant no filesystem, network or
environment access. See [WIT capability profiles](../../../../spec/wit/CAPABILITY_PROFILES_V1.md).

## Existing focused tests

```sh
cargo test --locked -p zryna-backend-webassembly component_command::tests
```

The [test root](tests/mod.rs) covers [real component/substituted bridge bytes](tests/component.rs),
[byte limits](tests/bytes.rs), [predecode limits](tests/predecode.rs),
[graph/type budgets](tests/budgets.rs), and [resource identity attacks](tests/resources.rs).
