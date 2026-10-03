# Internal generic Copy native execution

This #416 continuation adds `zryna_native_mir::generic_copy_v1::lower` and
`zryna_backend_native::generic_copy_v1::emit_object` above the preserved JS/Wasm chain.
The native MIR issuer retains the exact opaque `VerifiedCopyProgram` object, including source,
entry, layout, runtime and scalar ABI authorities. Raw graphs and older M3 programs cannot enter
this boundary. Only the exact existing Linux x86-64 target capability selects object emission.
No older MIR entrypoint, runtime ABI, scalar ABI, dependency, lockfile, profile, CLI or driver
admission changes. Full #416 completion and public generic support remain unaccepted.

## Representation and execution

Private values use i32 lanes derived from the retained Linux layout: one scalar lane, enum
discriminant plus maximum payload lanes, concatenated struct fields and repeated fixed arrays.
This is a private Copy representation, not a physical aggregate layout or FFI contract.
Every construction initializes the complete result vector, including inactive zero padding.
Only the selected exhaustive arm interprets its exact active payload. Ordinary edges carry
simultaneous SSA arguments, preserving shadowing and avoiding sequential overwrite.

Each local body takes a private result pointer followed by flattened i32 arguments and returns
void. The caller owns bounded stack storage, and copies every returned lane into SSA values
immediately after the call. Every successful return stores the complete result vector. The
result pointer cannot enter any source value, escape through a return, or reach a foreign call.
Nested calls use separate frames; no allocator, global transport state, memory imports, ownership
runtime symbol or host callback is present. Emission uses the pinned Cranelift verifier/codegen.

Public wrappers use exact scalar ABI v1 symbols, declaration order and i32 carriers. Bool input
and output guards trap carriers outside 0/1 without normalization. Typed invocation validation
retains its existing arity/type/unknown-export diagnostics. A C caller's arbitrary signature is
not typed host admission; the checked-in harness is test-only and activates no driver profile.

## Budgets and audit

The separate MIR preflight bounds each represented type at 256 i32 lanes, each private signature
at 256 parameters including its result pointer, each complete function arena at 65,536 lanes,
and complete code generation work at 1,000,000 charged units. The layout traversal limit is
inherited unchanged. These are native encoding bounds, not new source admission promises.
Temporary MIR reservations are fallible; checked arithmetic precedes private shape construction.
Each callee result slot is at most 1,024 bytes. Whole invocation stack depth and Cranelift's
additional spill storage are not qualified by these per-frame figures.

The final object retains the unchanged 8 MiB ceiling. An independent closed ELF audit checks
Linux x86-64/little-endian/relocatable identity, exact sections/flags, nonexecutable GNU stack,
exact local body and global wrapper symbols/extents, no undefined/dynamic symbols, and every
ordered direct-call relocation's caller, target, opcode, kind, width and addend. No foreign or
runtime import is admitted. The public `validate_object_inventory` utility grants no artifact
authority and does not authenticate arbitrary machine instructions. Only emission constructs
the opaque `ValidatedNativeObjectArtifact` after the closed audit.

| Diagnostic | Owning failure |
| --- | --- |
| `ZRYNA-N7001` | Checked private lane/signature/arena/work budget or MIR reservation |
| `ZRYNA-N7002` | Unexpected sealed native MIR/representation invariant |
| `ZRYNA-N7102` | Native body/code generation failure |
| `ZRYNA-N7103` | Final object target/inventory/extent/call relocation audit |

## Real conformance and remaining ownership work

[`tests/m7-generic-copy-native`](../tests/m7-generic-copy-native) contains a Linux-only explicit
runner. It authenticates both existing genuine single-module and exact-import v5 fixtures,
including Unicode, through original symbolic bodies, complete five-instance discovery, both
layouts, the separate runtime issuer, frozen wire encode/decode and independent Copy sealing.
Each fixture emits JS, core Wasm and native ELF from the same program object. A C caller and
pinned Node compare 36 fixed observations across targets: private forwarding, Option argument
and return, both Option/Result variants, active payloads, lexical shadowing and wrapping overflow.
Eight invalid raw native Bool carriers trap in isolated child processes; the parent verifies
subsequent valid calls. Typed ABI failures and hostile target/symbol/section/relocation/object
mutations have independent controls and pristine recovery. Accepted JS/Wasm artifact SHA256
values are fixed assertions. A controlled own-backend addition-to-subtraction mutation must fail
the unchanged C oracle, establishing actual downstream semantic sensitivity.

Run with pinned tools, an isolated target, `CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=2` and an
explicit evidence directory:
`python3 tests/m7-generic-copy-native/run.py <evidence-directory>`.
The runner is an internal direct proof, not a registered full-profile/platform acceptance gate.
GCC follows the inherited 12..15 Linux policy; receipts record the actual compiler version.

This seal still proves zero loans and zero drops. Full owned execution first needs a separately
sealed successor ownership plan: original opaque affinity, per-path live/moved/partial state,
selected enum payload transfer/refinement, shared/exclusive loan exclusion and lifetimes, and
complete reverse-order cleanup at scope, edge, return and failure sites. The current wire v1 has
no owner/cleanup-plan claims, and current source replay rejects loan and owned runtime operations.
Adding owned lanes to this private calling convention would therefore be insufficient.
Ownership runtime allocation/clone/fault cleanup, supported-platform conformance, provider parity,
driver/profile activation and #416 completion remain unfinished. Further existing shared native
MIR/runtime interfaces need named coordination with #417 before editing; this slice changes only
new modules and their registrations and imports no #417 implementation.
