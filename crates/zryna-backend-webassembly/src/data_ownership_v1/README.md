# Data and ownership core WebAssembly emission

[`emit_data_ownership`](mod.rs), re-exported by the backend, consumes sealed M3 IR and matching
ownership-runtime ABI. It checks type/layout/helper authority, emits bounded deterministic bytes,
validates them with the selected WebAssembly feature set, and independently audits the completed
core module before returning `ValidatedWebAssemblyArtifact`.

## Private areas and boundaries

| Area | Responsibility |
| --- | --- |
| [mod.rs](mod.rs) | Authority binding, complete byte bound, validation and closed capability audit |
| [encode.rs](encode.rs) | Deterministic module inventory and function encoding |
| [encode/operations.rs](encode/operations.rs), [encode/control.rs](encode/control.rs) | Verified operations and CFG edges |
| [encode/places.rs](encode/places.rs), [encode/values.rs](encode/values.rs) | Storage and value carriers |
| [encode/memory.rs](encode/memory.rs), [encode/memory/clone.rs](encode/memory/clone.rs) | Fixed private Linear32 memory and runtime helpers |
| [encode/failure.rs](encode/failure.rs), [encode/observation.rs](encode/observation.rs) | Sealed failure/cleanup behavior and observation |

The backend depends downward on verified IR/layout/runtime ABI; it must not call providers,
semantics, driver orchestration or other backends. This is a memory-bearing core module, not a
Component Model arrangement. Host selection, imports/grants, filesystem publication, and public
profile selection belong elsewhere. See [M3 public profile](../../../../docs/M3_PUBLIC_PROFILE.md).

## Existing focused tests

```sh
cargo test --locked -p zryna-backend-webassembly allocator_tests
cargo test --locked -p zryna-driver ownership_commands::conformance
```

[Allocator tests](encode/allocator_tests.rs) distinguish capacity/arena exhaustion and bound String
concatenation. The driver's [M3 conformance suite](../../../zryna-driver/src/ownership_commands/conformance.rs)
executes real target artifacts and faults using pinned hosts; `pnpm m3:check` is the complete
registered cross-target gate. Byte validation alone does not establish executed behavior.
