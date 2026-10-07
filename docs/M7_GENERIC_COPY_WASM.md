# Internal generic Copy core Wasm

This incomplete #416 continuation adds `zryna_backend_webassembly::generic_copy_v1::emit`.
Its sole input is the existing opaque `VerifiedCopyProgram`; raw claims, old M3 programs,
syntax DTOs and layout hashes cannot enter that boundary. It consumes the same program authority
as the generic JavaScript emitter and changes no older constructors, wire tags, scalar ABI,
runtime ABI, public CLI, protocol/profile selection or supported-platform acceptance.

## Representation and execution

Every used stored type derives a private scalar width from the retained, branded Linear32
layout. Bool and i32 occupy one i32 lane; an enum occupies a discriminant plus the maximum variant
payload width. Copy struct fields concatenate and fixed arrays repeat the derived element width.
These lanes are a private value representation, not a serialized layout or FFI representation.
Enum construction initializes the entire vector: the exact active payload and zero padding.
Only a selected match arm interprets its exact active payload. Edge transfers snapshot every
source lane before changing destination locals, preserving parallel transfer and lexical shadowing.

Each private function takes flattened i32 parameters and has no core result. A successful return
writes every lane of its exact result to private mutable i32 globals; the caller immediately
copies every returned lane into its own locals before its next operation or call. Unit has zero
lanes. Nested calls preserve live caller values in locals. The aggregate return convention avoids
multi-value, memory and allocation while retaining core Wasm 1.0. Globals begin at zero, are never
exported, and are not ownership/runtime state. No imports or callbacks permit host reentry during
an invocation. A trapped invocation grants no returned result; subsequent successful calls initialize
all lanes they read as results. This convention is confined to the sealed zero-loan, zero-drop lane
and cannot be extended to owned payloads by merely adding operations.

Public functions are separate scalar wrappers in the exact retained ABI declaration order.
Their logical names, i32 carriers and canonical 0/1 Boolean carriers retain scalar ABI v1.
Invalid raw Boolean carriers trap before the private body; results are checked without truthiness
normalization. A raw WebAssembly JavaScript call still applies the host API's coercions and does
not establish strict typed host validation. No production host wrapper is added by this backend.

## Bounds and final-byte authority

Before emitting any artifact, checked lane allocation bounds each type/result at 65,536 i32
lanes and each function's total parameters, values, parallel-edge scratch and state at 1,048,576
i32 slots. Layout dependency traversal inherits the 256-depth bound. Widths are memoized;
value/local allocation is fallible and parameters are visited linearly. These encoding limits
are distinct from source admission and engine-specific execution limits.

Every append to each section, body and final module is checked against 32 MiB before reservation.
The completed bytes pass the exactly pinned `wasmparser` validator with `WASM1`, then a separate
exhaustive audit authenticates the exact section inventory, function types/indexes, scalar export
names/indexes, private zero-initialized return global count, function-local counts/types and allowed
instruction/index set. Imports, memory, tables, start, data/element/custom sections, components,
GC, threads, SIMD, indirect calls, extra exports and unapproved instructions fail closed.
Only the successful final audit constructs the existing opaque `ValidatedWebAssemblyArtifact`.

| Diagnostic | Owning failure |
| --- | --- |
| `ZRYNA-W4001` | Lane/local/output amplification or failed temporary reservation |
| `ZRYNA-W4002` | Unexpected retained sealed-program/representation invariant |
| `ZRYNA-W4003` | Final-byte capability, inventory or permitted-instruction audit drift |
| `ZRYNA-W4004` | Invalid final core bytes or pinned validation/reader failure |

## Evidence and remaining work

The production full v5 authenticator reads the existing genuine single-module and exact-import
`.zry` fixtures, including Unicode source. Original opaque bodies, bounded instantiations, both
layouts and the successor runtime issuer precede wire encode/decode and independent Copy sealing.
Both programs then emit deterministic core modules and execute on pinned Node 22.22.1. Fixed
observations cover forwarding i32/Bool, aggregate Option argument/return, none/some, both Result
variants, active payload bindings, lexical shadowing, wrapping overflow and recovery after raw
Boolean rejection. Independent final-byte mutations reject capability/export/local/type/operator
drift and truncation; a pristine replay follows the rejections.

Exact/first-extra lane and writer tests are synthetic encoding/layout-boundary proofs, not source
admission or giant-module execution claims. The existing immutable Copy ownership proof remains
zero loans and zero drops. Full owned construction/move/borrow/drop replay and controlled runtime
fault traces, mutable/nominal source support, full owned native execution, provider parity, production
host adapters, driver/profile admission and the complete Linux/Windows generic acceptance matrix
remain unfinished. Required gates and exact-revision receipts govern each review candidate.
The separate [native Copy continuation](M7_GENERIC_COPY_NATIVE.md) executes the same immutable
program seal; its zero-loan/zero-drop evidence does not establish owned runtime conformance.
