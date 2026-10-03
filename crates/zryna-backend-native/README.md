# Zryna native backend boundary

The object emitter accepts only `VerifiedMirModule` and the capability returned by exact target
selection. It uses `cranelift-codegen`, `cranelift-frontend`, `cranelift-module`, and
`cranelift-object` 0.135.1 with `target-lexicon` 0.13.5. Independent inspection uses `object`
0.39.0. All versions are exact workspace pins; no installed LLVM, compiler, assembler, or linker
is a dependency of this backend or object emission.

The only target is `x86_64-unknown-linux-gnu`: ELF64, little-endian, relocatable, baseline x86-64,
non-PIC, no optimization, no unwind output, and no per-function sections. Exported `i32`
functions use the System V AMD64 convention, external linkage, and the exact sealed
`zryna_v1_e_<logical>` symbol. The backend never constructs this mapping.

After encoding, the closed audit checks size, format, architecture, endianness, object kind,
nonzero global text symbols, exact declaration order, undefined symbols, and relocations. The
current pure leaf profile permits neither undefined symbols nor relocations. Only then is
`ValidatedNativeObjectArtifact` constructed. Stable codes distinguish unsupported target
(`ZRYNA-N3001`), code generation (`ZRYNA-N3002`), and object audit (`ZRYNA-N3003`).

Textual LLVM output remains a compatibility proof, not the object implementation. Driver-owned
linking and execution consume only the sealed object under a separate contract and do not expand
this backend boundary. Startup, calls, runtime helpers, FFI, Windows/macOS output, and Boolean
source/IR are outside this slice.

The separate internal M2 entrypoint consumes only the independently verified
`control_flow_v1::VerifiedProgram`. It declares every `zryna_m2_i_*` body locally, adds global
scalar-ABI wrappers only for entry exports, uses typed `i8` Boolean bodies with exact `i32` carrier
validation, and lowers the complete arithmetic, comparison, call, branch, jump, loop, and parallel
block-argument inventory.

Its distinct artifact binds audited bytes to the exact scalar ABI. The M2 audit admits only the
observed `R_X86_64_PLT32`/`X86Branch`/32-bit/addend-`-4` relocation at the expected caller and to
the exact verified local callee, one-for-one with the MIR call graph and wrapper inventory. See
[M2 Linux x86-64 native backend](../../docs/M2_NATIVE_BACKEND.md). This internal evidence does not
activate the public M2 CLI or alter the M1 emitter and hash.

## Internal DataOwnershipV1 boundary

The M3 path consumes only independently verified `DataOwnershipV1` native MIR. It lowers the
closed instruction inventory to deterministic, non-PIC Linux x86-64 ELF objects and admits only
the MIR's global program symbols, local per-type clone/drop helpers, exact runtime imports, and
relocations to those definitions or imports. Aggregate and owned values use verified Linux layout
records; scalar public exports remain `bool`/`i32` only. A checked one-million-unit preflight
accounts for functions, parameters, places, blocks, operations, cleanup actions, type members, and
expanded fixed arrays before Cranelift allocation; exact limit succeeds and the first extra reports
`ZRYNA-N3304`.

The driver compiles the repository-owned C11 ownership runtime with sealed flags and the validated
GNU toolchain. A second audit requires the exact 17 runtime definitions, the closed ambient
`malloc`/`free`/`memcpy`/`memset` import set, approved ELF sections, and symbol-bound relocations.
It then links that runtime object, one audited program object, and one typed invocation harness.
The completed ELF must be 64-bit little-endian x86-64, non-writable/executable, contain a nonzero
entrypoint, define the selected MIR function and every runtime symbol, and use only the sealed C
startup and result-channel imports. Object and executable publication are independently
create-only through the validated `.zryna/out` capability.

Verified exit cleanup is executable, not documentary: `Return` and explicit `Trap` run their exact
cleanup plan, while an unexpected Weak upgrade status releases its prepared result and runs the
same failure plan before trapping. Runtime-status traps are terminal process failures; the runtime
never publishes an output pointer or handle on a rejected allocation, growth, clone, concat,
reserve, or transition.

The named evidence is:

- `zryna-native-mir/tests/data_ownership_v1.rs` for exact MIR retention and hostile independent
  verification;
- `zryna-backend-native/tests/data_ownership_v1.rs` for deterministic object bytes, closed symbols
  and all semantically admitted repository fixtures;
- `ownership_runtime_v1::tests` for strict C, sanitizer execution, exact exports, hostile pointers,
  injected allocation failure, and atomic output state;
- `native::ownership::tests` for the fixed Pair oracle from published ELF files, deterministic
  relinking, typed execution, runtime allocation-ledger cleanup, hostile invocation/executable
  rejection, target rejection, and create-only collisions;
- the existing DataOwnershipV1 IR/semantic exact-limit and first-extra suites for type, function,
  block, edge, value, place, cleanup, borrow, and diagnostic budgets.

This remains an internal candidate capability. Public `--profile data-ownership-v1`, manifest v3,
multi-target transactions, non-Linux execution, dynamic libraries, general FFI, raw pointers,
custom linkers, performance claims, and three-target M3 conformance remain outside this boundary.

## Internal native C scalar exports

`native_c_v0::emit_scalar_exports` accepts only the independently verified native C MIR and the
existing exact Linux x86-64 target capability. It emits every admitted total public scalar export,
retains the complete original program authority, and preserves distinct `c-i32`, `c-int` and
`c-bool32` header spelling. Boolean carriers outside 0/1 terminate before the source body runs.
The generated C11 header checks the target and carrier size/alignment. The independent audit
requires readable, size-matched section payloads, the closed ELF section inventory, exact public
symbol set, fixed file metadata,
non-overlapping text definitions and no undefined symbols or relocations.

This separate artifact emits no imported operation, private entry, foreign dispatcher, resource
ledger or safe wrapper. Focused evidence is `cargo test --locked -p zryna-backend-native --lib
native_c_v0`. Raw-to-verified MIR rejection remains independently tested by the MIR component.
Driver linking and reverse C execution are a separate boundary. Neither this API nor its tests
activate a public CLI profile or complete the #417 foreign-resource matrix.

## Internal native C handle execution

`native_c_v0::resources::emit_handle_entries` separately consumes the same immutable MIR seal.
It selects exact private entry symbols and admits scalar inputs/results plus handle and i32 output
slots. Private String/Vec storage, foreign byte copies and multi-owner creators reject before
emission. Original unselected bodies and declaration/header/policy authorities remain retained.
The finite aggregate emission inventory and existing 8 MiB object ceiling remain enforced.

Generated entries execute original foreign calls, classify status before output reads, reserve
capacity before C entry and share one 64-obligation compiler-private context. Successful non-null
acquisitions register nominal owner/release identity before metadata exposure. Conditional reverse
cleanup uses one checked same-library release body per operation; it marks a record as releasing
before C entry and clears it only after confirmed return. Unknown or malformed outcomes poison
the context and report unresolved live/reserved obligations. A process fault cannot produce a
private outcome or establish cleanup. Private caller storage must be valid owned memory; pointer
alignment and nominal checks do not make arbitrary in-process C memory corruption safe.

An independent audit closes ELF sections, hidden entry/dispatcher definitions, local helper
definitions, exact imported symbols and bounded non-overlapping relocation fields. Malformed
ELF tests mutate sections, visibility, imports and relocations independently of the producer.
Driver tests link the unchanged reviewed C observation fixture to generated bodies and check
success, declared failures, reverse cleanup, 64/65 capacity, alias and release identity refusal,
unknown status, null success and process-fault classification. Recoverable output snapshots detect
changed bits; they cannot detect an identical-bit write or a missing write of zero. The captured
library's reviewed initialization and failure-atomicity promises remain required.

This artifact provides no foreign library acquisition, native recipe permission, OS containment,
public CLI or Windows C target. The full #417 library and host proof matrix remains
unfinished. Focus with `cargo test --locked -p zryna-backend-native --lib native_c_v0` and the
driver's separate execution tests.


## Internal native C byte execution

`native_c_v0::resources::emit_byte_entries` is a separate private selection over the original MIR
seal. The handle-only API and its header ABI remain closed. The byte channel has its own context
magic and physical storage records; it accepts compiler-private owned String/Vec inputs and moves
an owned result only after the current terminal edge's exact reverse cleanup succeeds. Structural
input checks do not establish backing-memory provenance for arbitrary C callers.

Generated bodies execute the sealed preparation order: complete UTF-8 validation for String
loans, bounded Vec length and 0..255 element checks before packed allocation, canonical empty
loans, signed count checks and loan bounds before C entry. Foreign byte owners register in the
shared 64-obligation ledger before metadata exposure. Captured null/count/maximum/expected-length
and encoding policy governs validation and malformed cleanup. Successful copies allocate distinct
private Vec storage, zero-extend bytes to i32 and commit length after initialization; foreign
allocation is never adopted. Exact private runtime imports come from the retained runtime issuer.

Private allocation traps retain their declared AllocationV1/CapacityV1 identity. Unknown statuses,
changed failure outputs and failed releases report host failure with unresolved obligations;
release is never retried and a prepared result is withheld. The ELF audit closes the added private
imports and UTF-8 helper. Driver execution tests include empty and 4096/4097 lengths, invalid bytes,
UTF-8 boundaries, failed allocation/release, malformed-policy branches and independent fault
wrappers. Their manual disposal of deliberately unresolved test allocations is not generated
recovery evidence. Multi-owner creators and creators taking an existing handle still reject.
