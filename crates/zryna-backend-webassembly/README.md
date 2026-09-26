# Zryna WebAssembly backend

Direct core WebAssembly lowering from the M1 `VerifiedProgram` and the isolated M2
`control_flow_v1::VerifiedProgram`.

The separate private [command self-check](../../docs/WASI_COMMAND_SELF_CHECK_V1.md) retains
the exact scalar core inside an independently audited component. Its source and test fixtures
await the required execution lanes. Public WASI target selection remains unactivated.
`src/scalar_audit.rs` owns the existing core-only sealing and I32V1 instruction audit;
`src/component_command/` owns the distinct component artifact and final-byte audit.

`emit_scalar_component` is the public build-only Component Model boundary for the default scalar
profile. It retains the exact audited M1 core module, canonically lifts the verified `i32`
functions, and binds the artifact to the authenticated
`zryna:capability-profiles/browser@0.1.0` identity and complete pinned WIT-source digest. That
capability world has no imports or exports; the component's application exports are derived from
the verified program, not claimed as WIT world exports. Each public Component Model label is the
collision-free `zryna-export-<lowercase-hex>` encoding of the exact logical export's ASCII bytes;
logical and core WebAssembly names remain unchanged, and both identities are authenticated by the
component interface digest. The independent final-byte audit rejects
imports, nested components, starts, unsupported sections, changed aliases/types/exports, malformed
topology, substituted WIT identity, and the first byte above the 1 MiB ceiling before returning
artifact authority. Emission does not instantiate a host, generate a loader, or grant browser,
WASI, DOM, network, filesystem, clock, or random capabilities.
The separate driver browser profile consumes the sealed export view and retained artifact
identity; the backend still owns only component encoding and final-byte audit.
This repository-development boundary remains outside the advertised
[v0.1.0 preview support matrix](../../docs/DEVELOPER_PREVIEW.md).

The internal M3 `emit_data_ownership` entrypoint emits one validated, import-free core module with
private bounded Linear32 memory and sealed layout-derived address operations. It does not expose
memory or activate the M3 driver profile. Its private type-indexed helpers perform recursive
clone/drop, String/Vec operations, checked indexing and Shared/Weak transitions; scalar wrappers
bound allocation lifetime to one invocation. See `docs/M3_TARGET_BACKENDS.md`.
Requests within the universal 2,147,483,647-byte allocation/String limit that exceed the private
fixed arena report `ALLOCATION`; checked arithmetic and genuine universal/profile excess report
`CAPACITY`. The caller-owned 12-byte String result record consumes target arena space but does not
reduce the universal logical String-length maximum.

The backend consumes only sealed verified function views and exact scalar ABI WebAssembly export
names. It emits deterministic, import-free WebAssembly 1.0 modules for the current `I32V1`
profile and validates every completed binary with the exactly pinned validator before returning an
artifact. It does not depend on a frontend provider, JavaScript output, native MIR, LLVM, or a WAT
tool.

The current instruction surface is `local.get`, `i32.const`, `i32.add`, and function `end` over
pure `i32` parameters and results. Modules contain only type, function, export, and code sections;
they contain no imports, tables, memory, globals, start function, element/data segments, custom
sections, WASI, WIT, Component Model, GC, threads, SIMD, reference types, or ambient capabilities.
The optional scalar component wrapper leaves those core bytes unchanged.

Boolean core carriers remain specified and tested through the shared scalar ABI fixture, but this
does not enable Boolean source or IR. Source orchestration, artifact publication, runtime
execution, browser loaders, and public host wrappers do not belong to this backend.

The separate `audit_pinned_wit_worlds` prerequisite authenticates and resolves the accepted
`zryna:capability-profiles@0.1.0` source with the complete pinned WASI `0.2.12` dependency closure,
then independently compares all three resolved world interface sets. It uses exactly pinned
`wit-parser 0.258.0`, normalizes input order, applies pre-parse and resolved-graph budgets, and
creates fresh state per call. The returned observation is not component emission, bindings,
instantiation, profile activation, or a host-capability grant. The browser world remains empty.
See [WIT capability profiles v1](../../spec/wit/CAPABILITY_PROFILES_V1.md) and the
[source provenance](tests/wit-world-audit-v1/README.md).

The separate internal M2 entrypoint consumes only sealed `ControlFlowV1` views. It lowers every
current scalar operation, direct call, return, branch, jump, loop, and parallel block edge into a
deterministic core module with only type, function, export, and code sections. It validates the
complete module, exhaustively audits indexes, exports, locals, operators, and capabilities, and
caps incremental encoding at 32 MiB. This internal API does not activate protocol v3, manifest v2,
or the public M2 CLI. See [the M2 WebAssembly contract](../../docs/M2_WEBASSEMBLY_BACKEND.md).
