# Compiler architecture

Current public M3 selection is `--profile data-ownership-v1`, using protocol v4 and manifest v3.
See [the public M3 surface](M3_PUBLIC_PROFILE.md) and [the beginner guide](M3_GETTING_STARTED.md).
The component checkpoints below retain their historical implementation boundaries; they do not
limit or independently expand the integrated public surface. M0–M2 remain unchanged.

## Permanent boundary

Verified Universal IR is the permanent contract. Frontend providers and output backends are replaceable around it.

```text
source files
    ↓
FrontendProvider
    ├── TypeScript 6 adapter (bootstrap)
    ├── TypeScript 7 IPC adapter (planned after a stable upstream API)
    └── native Zryna frontend (planned)
    ↓
RawProjectSyntaxSnapshot v1 (declarations), v2 (M1), v3 (M2 syntax),
or internal v4 (M3 data and ownership syntax)
    ↓ exact file-set, path, budget, graph, and span verification
ProjectSyntaxSnapshot v1, zryna_syntax::v2::ProjectSyntaxSnapshot,
zryna_syntax::v3::ProjectSyntaxSnapshot, or the internal
zryna_syntax::v4::ProjectSyntaxSnapshot
    ↓
Zryna name resolution and strict semantic checking
    ↓
unverified Universal IR
    ↓
mandatory IR verifier
    ↓
VerifiedProgram + sealed scalar ABI module
    ├── JavaScript IR and printer
    ├── WebAssembly lowering and binary emission
    └── raw native MIR → mandatory MIR verifier → VerifiedMirModule
                                         └── codegen, object emission, and linking
```

No provider-specific syntax-kind number, node identity, symbol identity, or type identity may cross
`zryna-frontend`. Protocol-v2 syntax is owned by the lower `zryna-syntax` foundation crate so
semantic lowering never depends on a replaceable provider.

## Authority of each phase

1. `zryna-architecture` proves that the repository can be inspected completely and matches its declared graph.
2. A frontend provider reads compatible syntax and produces an untrusted, provider-neutral raw
   snapshot.
3. `zryna-syntax` verifies protocol-v2, protocol-v3, or internal protocol-v4 file identity, budgets, source spans,
   lexical order, and canonical expression/block graphs before constructing opaque executable
   syntax.
4. Zryna semantics resolves names, modules, scopes, and exact types. The protocol-v2/M1 path
   returns raw legacy IR to its existing verifier call site; the isolated protocol-v3/M2 path keeps
   raw `ControlFlowV1` internal and returns only mandatory-verifier-sealed IR.
5. `zryna-abi` verifies scalar signatures, logical exports, target mappings, and typed host values.
6. `zryna-ir` represents exact operations such as `I32Add`; generic target-dependent arithmetic is forbidden.
7. The IR verifier is the only constructor of a backend-accepted verified program and embeds the
   matching sealed scalar ABI module by declaration index.
8. The JavaScript backend consumes sealed ABI export names and emits deterministic ESM with
   explicit scalar-boundary checks. The driver publishes complete `.mjs` files create-only through
   a validated capability for the workspace's declared `.zryna/out` directory.
9. The WebAssembly backend maps exact Zryna operations directly to deterministic core
   WebAssembly, validates and profile-audits complete bytes, and exposes only a sealed artifact.
   An explicit default-profile component build retains that exact core, canonically lifts the
   verified scalar functions, binds the empty authenticated browser capability-world identity,
   and independently audits the final component bytes. The driver publishes `.wasm` create-only.
   The explicit repository-local browser component profile adds deterministic generated scalar
   bindings and declarations from the sealed component interface. Its loader authenticates the
   complete component before using the retained import-free core in the browser. WASI capabilities
   remain a separate explicit host profile.
10. Native lowering creates explicit typed native claims; the native MIR verifier retains the
    sealed scalar ABI module and is the only constructor of the codegen-accepted
    `VerifiedMirModule`.
11. The native backend consumes that authority, emits one fixed-target ELF relocatable object,
    independently audits it, and exposes only sealed bytes.
12. The driver may combine that sealed object with one generated, ABI-validated invocation using
    a previously proved Linux toolchain capability; the backend never owns linking or execution.
13. The CLI runs architecture validation first, then asks the driver to select either the
    unchanged default M1 path or explicit `control-flow-v1`. M1 analyzes one entrypoint once; M2
    discovers one authenticated module graph and lowers it once. Each path dispatches its same
   verified authority to an explicit target selection. The driver stages and commits one complete
   versioned build or run bundle; the CLI only parses and renders.
   A standalone default-M1 request retains that same compiler-checkout gate separately from one
   explicit project root, authenticates the project through the frozen package graph, and publishes
   only below the project's reserved `.zryna` state directory.
14. The repository-owned documentation producer exports an explicit whitelist of reviewed
    Markdown with exact compiler provenance. The website validates and presents that bundle but
    never becomes a language, ABI, diagnostic, or support-status authority.

## Dependency direction

```text
source ───────────────→ diagnostics
  ├──────────────────────→ syntax
  └──────────────────────→ frontend contracts ──→ syntax
diagnostics ─────────────→ syntax
  └──────────────────────────────────────────────┐
source ──────────────────────────────────────────┤
syntax ──────────────────────────────────────────┤
scalar ABI ─────────────→ export-name preflight ─┤
                                                  ↓
                                            Zryna semantics
                                                  ↓
                                          unverified Zryna IR
scalar ABI ──────────────────────────────────────┤
                                                  ↓
                                   verified IR + sealed ABI
                  ┌────────────┼────────────┐
                  ↓            ↓            ↓
            JavaScript   WebAssembly    native MIR
                              ↓              ↓
                         validation       codegen
                                             ↓
                                      audited `.o`
                                             ↓
                          driver-owned sealed harness + GNU link
                                             ↓
                              audited/published `.elf` capability
                                             ↓
                              private sealed snapshot execution
```

`zryna-driver` is the only library allowed to orchestrate all phases. The CLI calls the driver and architecture engine; individual backends do not call one another.

`zryna-windows-filesystem` is a leaf foundation beneath Windows transaction owners. It exposes only
atomic child-directory creation, same-source-handle no-replace rename, and exact-handle empty
deletion with retained-parent absence confirmation. The driver and applications retain
orchestration, names, inventories, diagnostics, and rollback policy; the foundation accepts no
ambient path or arbitrary source handle and grants no general filesystem authority. Its private
syscall module is the workspace's sole approved unsafe-Rust exception. The repository structure
gate enforces the exact exception manifest and module; all other components retain the workspace
forbid.

The permanent direction is `frontend -> syntax -> semantics -> IR`. `zryna-semantics` is a compiler
component and cannot depend on `zryna-frontend`; backends cannot depend on either provider layer.
The architecture engine has a negative graph fixture for both forbidden edges.

## Source and diagnostic authority

`zryna-source` is below diagnostics and every provider. One immutable bounded `SourceMap` owns
the exact UTF-8 text and assigns dense snapshot-local `FileId` values after normalized path
sorting. Source paths are portable workspace-relative ASCII with `/` separators and an
ASCII-case-folded uniqueness identity. Host filesystem path behavior is never used to interpret
provider paths.

All internal spans are zero-based half-open UTF-8 byte ranges. Opaque `FileId` and `Span` values
retain the issuing source-map identity. A span is authoritative only when constructed or resolved
through that exact `SourceMap`, which proves the file exists and both endpoints are ordered, in
bounds, and UTF-8 character boundaries. Source content is never normalized.

Diagnostics use one primary-location variant: source span, workspace path, or global. Source
diagnostics are resolved and sorted through the source map before stable text or versioned JSON
is emitted; a forged or mismatched span fails rendering rather than producing a misleading path.
IR verification also resolves every expression span with the compilation source map before it can
construct `VerifiedProgram`.

Verified protocol-v2 snapshots also retain the opaque identity of the issuing `SourceMap`, even
when the map contains no files. The semantic boundary requires that exact identity and rejects a
snapshot verified against any independently constructed map.

## Current strict semantic subset

The first source-to-IR gate accepts exactly one designated source file, conventionally ending in
`.zry`, with at least one exported function. Parameters and results require explicit `i32` or
`bool` annotations. Parameter names are
function-local and unique; bodies contain exactly one value return; expressions are parameter
references, in-range decimal `i32` literals, Boolean literals, and `i32 + i32`. Export names are
preflighted by the scalar ABI authority before IR verification.

The semantic phase owns rejection of missing annotations, `any`, unknown types, duplicate or
unresolved names, invalid export identities, out-of-range integers, invalid addition, mismatched
returns, and unsupported entrypoint shape. Diagnostics are source-located where a source token
exists, deterministic, and capped at 256 entries. Protocol-v2 resource limits are compile-time
constrained to the corresponding IR limits.

Semantic validity is not backend availability. Raw IR can represent `BoolLiteral`, but the current
`I32V1` verifier rejects every `bool` signature or expression before constructing
`VerifiedProgram`. A future universal Boolean profile must be implemented by every active backend
before that gate can be enabled.

## Verified Universal IR trust contract

`Program`, `Function`, and `Expr` are untrusted compiler claims. `zryna_ir::verify` is the only
constructor of `VerifiedProgram`; backends iterate opaque `VerifiedFunction` views and cannot
recover the raw program. The verifier delegates logical-name, collision, scalar-signature, and
target-mapping authority to `zryna-abi`, then embeds the sealed module beside the private program.
The current `I32V1` profile admits only `i32` parameters, results, literals, and signed wrapping
addition. Scalar ABI v1 specifies `bool`, but `bool` and `unit` remain profile-gated until every
active universal backend implements their specified representation and behavior.

A verified function proves all of the following:

- its scalar ABI export has a bounded logical name matching `[A-Za-z_][A-Za-z0-9_]*`, is unique
  exactly and under ASCII case folding, and carries deterministic JavaScript, WebAssembly, and
  Linux x86-64 target names;
- its body is in the same arena and has the declared result type;
- every operand is a distinct earlier entry, every entry has exactly one owner, and the complete
  tree is stored in exact left-to-right postorder without shared or orphan entries;
- its maximum expression depth is 128 and every expression span resolves in the exact compilation
  `SourceMap`; and
- program, parameter, expression, export-byte, and diagnostic budgets remain within the public
  constants in `zryna-ir`.

Verification and current JavaScript and WebAssembly emission are iterative and bounded. The normative
[scalar ABI v1](../spec/abi/SCALAR_V1.md) defines target names, carriers, invocation, and typed
observation. Both emitters implement the sealed export mapping for the executable `I32V1` profile.
The JavaScript emitter also implements its strict public carrier checks. JavaScript and core
WebAssembly carrier tests consume the shared ABI fixture, but do not admit Boolean source or
Boolean IR. A strict WebAssembly host wrapper and native public wrapper remain later gates.

## Current JavaScript artifact path

`zryna-driver::compile_javascript` is the source-connected JavaScript build boundary. It compiles
an authenticated source map through semantics and verified IR, emits one deterministic ECMAScript
module, and publishes `<stem>.mjs` only when the destination does not already exist. The caller
must first derive an `ArtifactOutputRoot` capability (also exposed by its JavaScript compatibility
name) from an absolute workspace path. That
capability resolves only the declared `.zryna/out` location and rejects any persistent path
component that is missing, non-directory, a symbolic link, or a Windows reparse point. The full
chain is revalidated immediately before publication. The artifact stem is one portable ASCII
filename component.

Publication writes, flushes, and synchronizes a create-new sibling temporary file before using a
create-only hard link for the final name. It never replaces an existing file, directory, or link.
A failed source, backend, or publication phase does not report a new artifact; an existing
destination is preserved byte-for-byte. Concurrent hostile replacement of filesystem ancestors
after validation is outside this process-local publication proof, and directory-entry crash
durability is not claimed. Temporary-name cleanup failure after successful publication is returned
as a warning without hiding the successfully published artifact.

The public run command imports and executes generated modules with an explicitly validated, exact
Node.js 22.22.1 runtime. The same engine remains the pinned integration-test harness. Generated
modules are self-contained; the Node process is a host, not a bundled Zryna runtime.

## Current WebAssembly artifact path

`zryna-driver::compile_webassembly` independently connects authenticated source to verified IR,
the direct WebAssembly backend, and `<stem>.wasm` publication. It does not call the JavaScript or
native backends. The backend emits deterministic core modules in type, function, export, and code
section order, using only `local.get`, `i32.const`, `i32.add`, and `end`; empty programs are the
eight-byte core-module header. Exports use only the sealed WebAssembly ABI name.

Completed bytes must pass `wasmparser` with explicit `WasmFeatures::WASM1` and then a fail-closed
profile audit. The audit permits no imports, tables, memory, globals, start function, elements,
data, tags, custom sections, non-function exports, locals, or instructions outside `I32V1`.
Only after both checks does the backend construct `ValidatedWebAssemblyArtifact`; the public
publisher accepts that sealed type rather than arbitrary bytes. Publication reuses the same
validated `.zryna/out` capability and create-only atomic writer as JavaScript, so same-stem `.mjs`
and `.wasm` artifacts can coexist and existing destinations remain untouched.

Node.js 22.22.1 validates, instantiates, inspects, and executes the real artifact through the
standard WebAssembly API in conformance tests and the public run command. That API is browser-
compatible, but this is not a browser or DOM execution claim. Raw JavaScript calls to WebAssembly
perform host coercion; the CLI validates the typed `I32V1` invocation before execution, while a
general strict public host wrapper remains outside this slice.

Native MIR has its own consumed raw-to-verified boundary. Raw functions claim logical names, a
convention, typed signatures, dense typed values, operations, and results. The iterative bounded
verifier proves the MIR invariants and independently seals scalar ABI v1 before constructing
`VerifiedMirModule`. Each function view therefore carries the authoritative
`zryna_v1_e_<logical>` symbol and Linux x86-64 System V convention; codegen does not invent names.

`compile_native_object` selects exactly `x86_64-unknown-linux-gnu`, lowers verified source through
native MIR, emits with pinned pure-Rust Cranelift, parses and fail-closed audits the ELF64
little-endian relocatable object, then publishes `<stem>.o` through the shared create-only output
capability. The audit requires the exact sealed global text symbols, no undefined symbols, and no
relocations for the current leaf-function profile. It exposes bytes only as
`ValidatedNativeObjectArtifact`.

The driver owns the distinct native executable path. It discovers and pins canonical
`/usr/bin/gcc`, its exact GNU x86-64 target, supported version, and canonical GNU linker into an
opaque capability. A typed invocation must first pass Universal IR's embedded scalar ABI authority.
The driver then writes the sealed object and one generated C11 harness into a private staging
directory, launches the compiler driver directly with a cleared environment and fixed hardening
arguments, and audits the resulting ELF executable before create-only `.elf` publication.
Execution accepts only that published capability, uses a bounded process group, and returns a
typed outcome from an exact four-byte channel. The capability retains the audited bytes and runs a
fresh private staged copy, so replacing the public distribution path cannot change executed code.
The CLI composes this boundary only for a previously verified, invocation-specific `I32V1`
request. It is not arbitrary startup or a general native runner. Control flow, calls, Windows
native output, FFI, and Boolean source/IR remain later gates.

## Public CLI orchestration and transaction

`zryna build` and `zryna run` accept one validated workspace-relative `.zry` entrypoint and one
explicit target. Both accept `javascript`, `webassembly`, `native`, or `all`; default-M1 `build`
also accepts `component`, and exact `--profile browser-component-v1` admits only
`build --target component`. Architecture validation is always
first. Omitting `--profile` preserves the exact M1 protocol-v2/`I32V1` path. Exact
`--profile control-flow-v1` selects protocol v3, driver-owned deterministic module discovery, and
the separate M2 semantic/IR path. The driver constructs one verified authority per request and
dispatches it in fixed JavaScript, WebAssembly, native order. Run requests also validate one exact
export and typed argument vector once before any target executes.

Individual library publishers retain their create-only artifact contracts. The public CLI adds a
coarser transaction boundary: selected target artifacts and the profile-specific manifest are written
and synchronized in one directory adjacent to the final bundle. Unix sets transaction directories
to mode `0700`; Windows inherits ACLs from the validated compiler-owned output root and therefore
requires that root to be private to the invoking principal. After containment is revalidated, one
create-only same-filesystem directory rename commits either
`.zryna/out/<stem>.build` or `.zryna/out/<stem>.run`. Only selected target subdirectories exist.
The default, unprofiled component selection publishes only `component/<stem>.wasm` plus manifest
v1. The separate `browser-component-v1` selection publishes the same audited component together
with deterministic ESM bindings, declarations, and `zryna-browser-manifest-v1.json` in one
create-only bundle. Neither component selection belongs to `all` or has a CLI run route. Both
remain outside the advertised [v0.1.0 preview support matrix](DEVELOPER_PREVIEW.md).
Any preparation, execution, audit, publication, or cleanup failure before commit leaves no final
bundle, and an existing bundle is never replaced.

Default M1 writes only `zryna-manifest-v1.json`; the browser scalar component profile writes only
`zryna-browser-manifest-v1.json`; M2 writes only `zryna-manifest-v2.json`. Build bundles
contain `.mjs`, `.wasm`, and/or the native `.o`. Run bundles contain `.mjs`, `.wasm`,
and/or the invocation-specific native `.elf`, plus stable ordered typed observations in the
manifest. Manifest v2 additionally authenticates the canonical entrypoint, path-ordered source
hashes, named-binding module edges, and module-graph digest sealed by discovery. The
[M1 conformance suite](M1_CONFORMANCE.md) compares the public M1 `all` observations with
fixed expected values and the committed manifest; the runtime command does not define a second
comparison semantics. The [M2 conformance gate](M2_CONFORMANCE.md) independently performs the
fixed-oracle aggregate M2 comparison without adding a runtime semantics authority.
See the [CLI reference](CLI.md) and [manifest-v2 contract](M2_MANIFEST_V2.md) for the exact command, layout, manifest,
exit-status, runtime, and platform contracts.

The internal M3 candidate uses a separate protocol-v4 and `DataOwnershipV1` route. The driver
authenticates one final module closure, lowers one verifier-sealed program retaining exact layout
and ownership-runtime ABI identities, and dispatches selected JavaScript, WebAssembly, and Linux
x86-64 native targets in fixed order. Run requests validate one typed invocation before any target
execution. Execution, strict manifest-v3 construction, artifact audit, and create-only whole-bundle
publication share one private transaction, so failure cannot advertise a partial result. Exact
`--profile data-ownership-v1` activates this same composition; see the
[candidate driver contract](M3_CANDIDATE_DRIVER.md).

## Initial numeric contract

The first vertical slice defines signed 32-bit wrapping addition:

```text
Zryna IR:      I32Add(a, b)
JavaScript:  (a + b) | 0
WebAssembly:   i32.add
LLVM IR:     add i32 %a, %b
```

Future integer operations must specify width, signedness, overflow, conversion, comparison, and JavaScript representation before implementation.

## Isolated `ControlFlowV1` boundary

M2 is governed by the normative
[scalar control-flow and modules v1](../spec/language/CONTROL_FLOW_MODULES_V1.md) specification.
It is a separate verified profile selected only by exact `--profile control-flow-v1`. Protocol v2,
the `I32V1` expression-tree verifier, the default M1 CLI, and manifest v1 remain unchanged.

Protocol v3 carries source-faithful import, local, call, branch, and loop syntax without giving the
provider name, type, module, filesystem, or IR authority. The driver now exposes an internal
[bounded deterministic module closure](M2_MODULE_CLOSURE.md) that safely resolves an explicit
relative `.zry` graph to fixed point through retained no-follow capabilities, authenticates one
final source map, and seals its canonical graph identity. The internal
[straight-line M2 semantic boundary](M2_STRAIGHT_LINE_SEMANTICS.md) revalidates the final graph and
lowers exact types, locals, lexical scopes, arithmetic, comparisons, assignment, and acyclic direct
calls. The internal [control-flow semantic boundary](M2_CONTROL_FLOW_SEMANTICS.md) extends that
authority with canonical branches, loops, merge/header parameters, definite state, reachability,
and return analysis. The internal [M2 JavaScript backend](M2_JAVASCRIPT_BACKEND.md) and
[direct core WebAssembly backend](M2_WEBASSEMBLY_BACKEND.md) consume only the resulting opaque
views and sealed ABI mappings. They emit canonical private-ID functions,
explicit parallel CFG-edge transfers, exact scalar operations, and typed entry wrappers without
source names, paths, dynamic loading, or ambient capabilities. WebAssembly emission additionally
validates and exhaustively audits the exact completed core bytes. The isolated
`zryna-ir::control_flow_v1`
component now implements the mandatory verifier for types, dominance, edges, reachability,
reducibility, return completeness, acyclic calls, source authority, and budgets before constructing
opaque M2 views. The closure connects only to these internal semantic gates and sealed backend
entrypoints. The separately versioned [M2 native MIR profile](M2_NATIVE_MIR.md) lowers the same
sealed whole-program authority into explicit target-specific block, call, symbol, Boolean, and
terminator claims, then independently verifies them into opaque views. The separate
[M2 native object and typed link/run boundary](M2_NATIVE_BACKEND.md) emits local typed bodies and
public scalar wrappers, audits exact call-graph-bound ELF relocations, and retains artifact-bound
invocation authority. The driver composes these components through one explicit multi-file request
and one [manifest-v2 transaction](M2_MANIFEST_V2.md). Individual M2 target requests and the
independent [fixed-oracle three-target conformance gate](M2_CONFORMANCE.md) are implemented. Issue
#57 records authenticated website import, deployment, and live provenance externally; the compiler
architecture does not infer deployment state or broaden its capability set from that evidence.

Entry-module exports alone retain scalar ABI v1 public mappings. Dependency exports and unexported
functions receive sealed target-internal identities. JavaScript and core WebAssembly now consume
that authority internally; the M2 native MIR lowering now consumes and independently reseals the
same verified whole-program authority without trusting source-selected symbols.
No backend may activate M2 independently. The public command requires
`--profile control-flow-v1` and publishes a distinct canonical `zryna-manifest-v2.json` so no M1
artifact contract is reinterpreted.

## Isolated `DataOwnershipV1` boundary

M3 is specified as another separately selected and separately verified profile. Its syntax,
layout, and Universal IR authorities are implemented internally, and the complete conformant route is publicly
selectable through exact `--profile data-ownership-v1`. It may not widen `I32V1`, mutate `ControlFlowV1`, or expose a partial public command.
Its remaining authority chain is:

```text
verified protocol-v4 syntax
    ↓
compiler-owned nominal/type/ownership semantics
    ├── implemented Copy aggregate lowering
    ├── private straight-line String/Vec ownership checkpoint
    ├── explicit Shared/Weak producers and sealed WeakUpgrade control flow
    ├── verified aggregate-layout authority
    └── retained sealed ownership-runtime ABI declaration authority
    ↓
raw DataOwnershipV1 IR
    ↓ independent exhaustive verifier
opaque verified DataOwnershipV1 views
    ├── symbolic control/fault/resource proof only
    ├── deterministic JavaScript + private helpers
    ├── audited memory-bearing core WebAssembly
    └── independently verified native MIR → audited Linux x86-64 artifact
```

The isolated [`DataOwnershipV1` IR contract](M3_DATA_OWNERSHIP_IR.md) verifies one raw program
against an independently supplied final `SourceMap`, expected entry file, and owned verified
`Linear32V1` and `LinuxX8664V1` layout snapshots. The two snapshots must carry the exact targets,
same source-map and type-universe identities, and fingerprints claimed by the raw authority tuple.
Success retains both sealed layout authorities and scalar ABI v1 while exposing only opaque
module, function, block, value, place, projection, ownership, borrow, and cleanup views. Its
`OwnershipRuntimeV1` value is a closed contract-identity enum that later authorities bind exactly.
The Issue #81 verifier foundations add bounded immutable `StringFromUtf8` bytes, one exact owner
place for each non-Copy value while allowing addressable Copy storage without a cleanup obligation,
and site-bound cleanup authority. Every cleanup plan belongs to one exact block instruction or
terminator and one closed `PrepareFailure`, `VecCloneElementFailure`,
`AggregateCloneElementFailure`, `CallTrap`, `Return`, or `ControlledTrap` role;
`ReplacePlace` is the infallible commit after preparation and carries no cleanup plan. The verified
instruction nevertheless derives one exact pre-commit recursive drop action for the old root or
static-projection destination. Projection replay transplants the prepared source subtree's state
and active enum variants while retaining the enclosing owner's pending obligation and sibling
masks. The semantic producer now uses that authority for mutable available static String leaves and
for at most one combined private straight-line aggregate site. That narrow site immediately moves
or clones one complete same-type static Struct/FixedArray subobject between distinct local roots, moves one
distinct fully initialized same-type supported non-Copy whole local, or explicitly clones such a
whole root into a mutable available `StructField` or `FixedArrayConstant` projection. Each form is
immediately consumed from one unique typed temporary by `ReplacePlace`. Projected move masks the
complete source subtree; projected clone retains its source without descendant place expansion and
seals layout-derived prepare/prefix failure cleanup; whole-root clone retains its source and its prepare and initialized-prefix
failure cleanup retain source plus destination. Commit recursively drops only the exact old target.
Projected-subobject move or clone retains both pending roots plus every sibling mask; whole-root clone
retains source and destination; whole-root move consumes its source and retains the destination.
Fresh, same-root/overlapping,
partial/moved projected sources, broader projected clone contexts, dynamic/Vec/Enum targets, calls, CFG/public
contexts, and second sites remain later checkpoints.
The same ownership replay admits one separate final-return move of a complete static
Struct/FixedArray subobject. It is sealed to a parameter-free private one-block function, one local
source root with exact descendant topology, a final `MoveFromPlace`, and that unique temporary as
the sole returned value. Return cleanup masks the complete source subtree, excludes the returned
owner, and is capacity-checked before semantic materialization or mutation.
Those same initialized available static String leaves admit explicit clone. Its fallible
preparation reads without consuming the leaf, retains the enclosing root with its current
partial-state masks, and creates one distinct temporary owner before any later assignment commit.
The separate [`ownership-runtime ABI authority`](M3_OWNERSHIP_RUNTIME_ABI.md) now verifies the exact
17-operation declaration vocabulary, target symbols and signatures, authenticated layout-derived
records, checked header evidence, and pure transition evidence behind opaque immutable views. It is
not an allocator or runtime implementation and supplies no target object, backend, driver, CLI, or
public aggregate ABI.

The completed internal #83 compiler boundary covers the frozen Shared/Weak payload categories,
exact handle transitions and owner identities, indivisible success/expired/overflow upgrade
outcomes, failure-prefix cleanup and replay, and explicit Weak cycle breaking without tracing.
Its ABI evidence is symbolic and non-executable: it performs no allocation, concrete target count
mutation or outcome selection and enables no backend, driver, CLI, or public profile.

The internal #270 closure candidate composes that handle authority with the generic owned operation
core. A checked matrix binds nested aggregate/container construction and transfer, structural clone,
ordinary Vec observation/replacement/push, handle-containing static subobjects and finite values
through legal Vec-indirection recursion to authenticated source, mandatory verified IR, hostile IR,
resource and replay evidence. This is compiler authority only; runtime execution, backends and
public activation remain open.

The checked [non-indexed owned borrowing matrix](M3_NONINDEXED_OWNED_BORROWING_MATRIX.md)
integrates #337 owned roots and static Struct/FixedArray projections, #338 refined active-enum
payloads, #339 nested lexical and direct-call use, and #340 hostile/resource closure. Source
lowering emits only the existing borrow, owned-clone and prepare-before-replace operations; the
mandatory IR verifier independently seals exact place, mode, region, refinement, overlap, cleanup,
nonescape and resource authority. This compiler-only #275 candidate adds no runtime alias checks,
backend, driver, CLI, artifact, public ABI/profile, or target execution. The combined compiler
authority is reconciled by the [#269 source-completion proof](M3_SOURCE_COMPLETION.md).

The checked [internal owned-call closure matrix](M3_OWNED_CALL_CLOSURE_MATRIX.md) integrates #329
imported signature and canonical identity resolution, #330 structured transfer and exact cleanup,
and #331 independent hostile-IR and resource proofs. Internal same-module and named-import calls
admit the sealed by-value ownership graph without adding nominal type-import syntax. Borrowed
imports, indirect calls, recursion, runtime execution, backends and public activation remain open.

The checked [complete enum matching closure matrix](M3_COMPLETE_ENUM_MATCHING_MATRIX.md) integrates
#333 authenticated exhaustive source lowering, #334 independently constructed refinement and
payload ownership verification, and #335 exact resource/overflow evidence. It makes #273 a
compiler-only closure candidate for sealed multi-variant and nested matches with exact active-
payload cleanup and Copy or owned result continuations. Terminating and wildcard arms and inactive
payload access remain outside that matrix; #275 is completed separately at its compiler boundary.
Runtime, backends and public activation remain open.

The checked [structured owned control-flow matrix](M3_STRUCTURED_OWNED_CONTROL_FLOW_MATRIX.md)
integrates #325 source routing and lexical state, #326 independent hostile-IR authority, and #327
payload/fault/resource evidence. It makes #271 a compiler-only closure candidate: nested and
repeated Block/If/While/WeakUpgrade and admitted Match occupants lower to mandatory verified IR,
with exact joins, backedges, cleanup and checked graph budgets. These verified traces do not
execute allocation, refcount, drop, calls, upgrades, or faults. #275 is completed separately at its
compiler boundary and is included in the #269 source-completion proof. Break/continue, exceptions,
driver integration, CLI and public activation remain open.

The [internal M3 target backends](M3_TARGET_BACKENDS.md) consume the resulting closed operand and
terminator views. JavaScript emits bounded self-contained ESM, core WebAssembly emits validated
import-free bytes with private bounded memory, and native lowering independently reseals Linux
x86-64 layout-bound MIR. Object/link execution and public routing remain downstream boundaries.

The internal [`M3 Copy aggregate semantic boundary`](M3_COPY_AGGREGATE_SEMANTICS.md) consumes the
exact source-map-bound protocol-v4 authority, resolves canonical nominal identities and exact
types, verifies one type graph for both layout targets, and lowers recursively Copy structs, enums,
and fixed arrays into the independently verified IR. Fixed-array projection is limited to a
compile-time in-range constant. The internal Pair oracle is observed only through a test evaluator
over opaque verified views. The same private boundary now recognizes canonical String and
`Vec<T>` type graphs. Its private owned-data checkpoint lowers String literals, explicit clone,
checked concatenation, moves, return cleanup, and mutable root-local replacement; it also lowers
Vec construction, explicit clone for exact `Vec<bool>`, `Vec<i32>`, and `Vec<String>`, moves,
return, push, checked Copy-element indexing, and supported root-local replacement. Bounded private owned functions
support exact owned parameters and internal calls,
one top-level branch or while loop, terminal owned `if` as exactly three entry/then/else blocks
with direct returns from both arms and no join block or parameter, reverse-order cleanup of
branch/iteration locals, and a single stable mutable String or Vec root across the admitted loop
mutation shape. A separate parameter-free private straight-line route constructs, moves,
explicitly clones, returns, and reverse-order drops bounded owned Struct, FixedArray, and root Enum
graphs with Copy/String leaves. Structural clone retains its source, creates a distinct result owner,
derives the fallible String-leaf count and active root-enum variant from sealed authorities, and
reverse-drops only the initialized result prefix on element failure. Mutable whole-root assignment
for these supported aggregate graphs prepares a distinct replacement before committing it and
retains the old root across preparation failure. These operations use
`InitializePlace`, `MoveFromPlace`, and `ClonePlace`; String/Vec/aggregate replacement uses the
infallible `ReplacePlace` commit after right-hand-side preparation.
Canonical static `StructField` and `FixedArrayConstant` places also carry Copy reads and exact
String-leaf moves from source syntax through verified IR. One exact direct local may additionally
move a supported acyclic Struct or FixedArray subobject from either canonical static projection.
The producer materializes the moved projection's complete descendant topology before emission;
verified IR marks that projection and every descendant moved under the enclosing root while the
new local receives the complete subobject owner. Projection identity is root-relative and stable,
moved subobjects refine the enclosing root's cleanup mask, and disjoint siblings remain
available; repeated and overlapping consumption is rejected. At most one initialized available
non-Copy Struct or FixedArray projection under those static paths may also be explicitly cloned
into the immediately following exact same-type root local. That clone leaves the enclosing owner
and all masks unchanged, derives recursive String-leaf failure cleanup from sealed layout, and is
independently rejected outside a private one-block context or when its unique temporary result has
an alternate use. For one exact-type direct local
declaration, a partially moved supported Struct or FixedArray root now transfers through its
move-result temporary into the new local. The producer derives and materializes one complete static
topology for all three roots, then migrates the exact root-relative mask at both owner renames; the
old source and temporary retain no cleanup authority.
A final exact-reference return uses the same sealed topology to migrate a partial Struct/FixedArray
mask into the returned temporary before cleanup. The verifier transfers that temporary out, then
requires exact reverse cleanup of only the surviving owners; forged paths, unsupported graphs, or
a returned-owner drop are rejected.
One distinct mutable fully initialized same-type whole-root assignment destination admits that
partial source. The producer preflights and materializes complete source, temporary, and
destination topology before mutation; `ReplacePlace` drops the old destination exactly once and
installs the transferred mask. The verifier rejects incomplete or forged topology, a partial
destination, and any partial owner on a CFG edge.
The same projected-replacement boundary now admits one complete static Struct/FixedArray subobject
move or clone between distinct local aggregate roots. It seals the immediate `MoveFromPlace` or
`ClonePlace` -> sole-use typed temporary -> `ReplacePlace` sequence and a canonical target path.
Move requires complete source projection topology and masks that subtree beneath its still-pending
root. Clone retains its available source, needs no descendant places, and authenticates layout-
derived prepare and initialized-prefix cleanup. Commit derives a target-only recursive old-subtree
drop. Both enclosing roots, all disjoint sibling masks, and pending-root order are preserved.
Same-root overlap, partial or already-moved source subtrees, alternate uses/order, CFG/public
functions, and a second move/clone or shared projected-clone site fail closed.
One separate source-faithful exception accepts a private one-parameter function whose single-
variant enum is exhaustively matched into an exact direct local. The refined arm moves the complete
supported Struct/FixedArray payload, initializes the local, drops the emptied enum root, and jumps
without owner arguments to one final local return. The verifier seals the exact three-block graph,
active ordinal, complete payload topology, zero-action return cleanup, and the absence of a second
site or alternate escape.
The boundary reports moved
bindings in the private String route as M3011, moved aggregate/enum bindings as M3014, unresolved binding names as
M3002, and preflights cumulative String-literal bytes at 8 MiB. General structural Vec clone beyond
String elements, nested aggregate clone graphs containing Enum, Vec, Shared, or Weak values,
aggregate-subobject moves outside at most one exact direct local, one parameter-free exact final
return, one distinct-root static
projection replacement, or the single-variant match-local enum-payload exception, dynamic or
Vec-element projections, projected aggregate assignment outside the exact static-subobject-move-
or-clone-or-whole-root-move-or-clone-to-static-projection exception, projected aggregate clone outside the exact
direct-local or distinct-root static-replacement exceptions, partial
Enum transfer or partial-root transfer outside the exact
direct-local, final-return, or whole-root assignment forms and loop-carried owned phi values are
not admitted. The #271 route separately admits general nested/repeated owned control flow, body
returns and scope-drop insertion; `break` and `continue` remain excluded. The assignment exception does not extend to fresh sources, partial/moved projected
subtrees, broader projected clone contexts, same-root/overlapping paths, or dynamic/Vec/Enum targets,
calls,
direct projected-clone or payload returns, owner-carrying CFG transfer, public functions,
multi-variant Enum payloads,
or dynamic/Vec projections. Its sealed
semantic result retains both the verified IR and the exact verified
ownership-runtime declaration authority without exposing either raw input. This creates no
runtime, backend, driver, CLI, manifest, target artifact, or public aggregate ABI capability.

The normative planning authorities are
[`DATA_OWNERSHIP_V1.md`](../spec/language/DATA_OWNERSHIP_V1.md),
[`AGGREGATE_LAYOUT_V1.md`](../spec/memory-model/AGGREGATE_LAYOUT_V1.md), and
[`OWNERSHIP_RUNTIME_V1.md`](../spec/abi/OWNERSHIP_RUNTIME_V1.md). The digest-pinned
`tests/m3-contract-v1.json` registry binds the real Issues #75–#90 and requires M0, M1, and M2 as
unchanged regression authorities.

The [bounded borrowing implementation contract](M3_BORROWING_SEMANTICS.md) freezes Issue #82 as
`#113 -> #114 -> #115 -> {#116, #117, #119, #120, #121} -> #122`. Issue #113 audits and tightens
the existing verified-IR prerequisite only: dense shared/exclusive authority, real parameter use,
exact lexical end, overlap, direct-call matching, edge nonescape, and the 16,384-active-borrow
limit. Issue #114 adds one internal private straight-line shared-root semantic shape for literal-
initialized `bool`/`i32`: const aliases lower to dense shared begin/read authority, compatible Copy
owner reads lower directly from the still-initialized root, aliases end in reverse at the exact
nested-block exit, and the root remains readable afterward. It preflights all derived resources
before raw-IR construction and adds no runtime, ABI, backend, driver, CLI, or public profile
capability. Issue #115 generalizes that same root-only plan to exclusive Copy read/write authority
and one shared-from-shared reborrow. Assignment to a const `BorrowMut<T>` alias is write-through,
not rebinding; the syntax worker remains syntax-only. The full root conflict matrix, exact literal
write type, owner exclusion, reverse restoration, and all read/write resource counts are checked
before raw-IR construction. Issue #117 adds one fixed four-block/four-edge private bool-root
conditional: each arm contains one explicit lexical scope, reverse-ends all of its authorities
before its jump, and joins with no borrow block parameter, edge argument, or ownership-flow state.
Both arms are planned before materialization; identities follow then/else source order, arm-local
value and transition costs sum, and peak active authority is the larger arm. The independent verifier uses canonical
dense block order for join diagnostics without changing edge-indexed enum refinement. Mutable-from-
shared, reborrow from exclusive, and nested/repeated conditional borrowing remain later slices.
Issue #119 adds one bounded private straight-line whole-root direct-call boundary with exact
recursively Copy signatures, source-ordered shared/exclusive borrow parameters, left-to-right
argument evaluation, same-authority forwarding, caller-owned lexical end, and verified call-trap
cleanup. The mandatory IR verifier remains final authority for exact access/referent matching,
nonescape, acyclic calls, and static call depth; protocol v4 is consumed unchanged. This adds no
runtime lifetime state, ABI, JavaScript/WebAssembly/native backend path, driver/CLI route, artifact,
website support claim, or public-profile activation. Issue #120 adds recursively Copy
Struct/fixed-array projection plans whose
canonical static prefixes are materialized once: same/ancestor/descendant paths overlap, distinct
static siblings are disjoint, and dynamic/Vec/enum/non-Copy cases fail before raw IR construction.
The independent verifier rechecks exact projected authority and owner move/replace/drop/call use.
Issue #121 adds one fixed preheader/header/body/exit bool-root loop whose body
reverse-ends one static dense authority plan before every backedge, restores the exact root
owner/initialization state at the header, and carries no borrow authority, value block parameter,
or edge argument. The completed Issue #116 implementation admits one private
parameter-free whole owned root, one const shared alias in one lexical block, and only String
clone/checked concat, exact `Vec<bool>`/`Vec<i32>` Copy indexing, or supported whole
Struct/root-Enum/fixed-array clone. It brackets the existing owned operations with shared lexical
authority, retains the source, gives each owned result a distinct owner, and reuses existing
cleanup/fault plans; `BorrowRead` remains Copy-only. It adds no projection, mutation, move,
runtime, backend, or public capability. Issue #116 passed independent verification and required
merge gates. Issue #122 consolidates the complete bounded internal borrowing boundary, its named
resource/negative evidence and integrated regressions. The #83 internal Shared/Weak compiler and
proof boundary is complete; the #254–#256 indexed-borrowing chain remains separately tracked. This
enables no public profile or runtime.

`zryna-syntax::v4` is the provider-neutral M3 syntax boundary. Its closed JSON schema, bounded raw
DTOs, pinned TypeScript 6 syntax-only worker, strict process handshake, and Rust verifier preserve
source-faithful declarations and operations in canonical arenas. The verifier authenticates every
file, span, token, edge, owner, depth, and count against one exact final `SourceMap` before exposing
an opaque bound snapshot. It assigns no nominal identity, type, field or variant ordinal, move,
borrow, layout, ownership state, IR instruction, ABI, runtime operation, or backend capability.
Protocol v4 is internal and does not change protocol v2 or v3. Its exact contract is documented in
[`SYNTAX_PROTOCOL_V4.md`](SYNTAX_PROTOCOL_V4.md).

`zryna-layout` accepts no syntax or backend types. It binds
the complete module/type graph to one exact `SourceMap`, assigns canonical TypeIds from frozen
binary keys, rejects malformed identities, orphan claims, borrows, by-value recursion, overflow,
and resource excess, and exposes only immutable `Linear32V1` or `LinuxX8664V1` views plus a sealed
fingerprint. Its raw graph IDs never become TypeIds, and neither host `usize` nor Rust/C layout,
paths, addresses, allocation state, or compiler version text enters a layout document. The
machine-readable `crates/zryna-layout/src/fixtures/layout-v1.json` oracle is shared with later IR, backend,
runtime, and conformance work.

Issues #75 through #80 add no executable capability. Later components must keep syntax providers
free of semantics, make every backend consume these opaque verified views, depend on ABI
declarations rather than runtime implementations, and never recompute host layouts. The driver
alone composes audited target runtimes and publishes the explicit `data-ownership-v1`
manifest-v3 transaction after all target gates pass.

## WebAssembly profiles

The current WebAssembly backend emits a core module directly from `VerifiedProgram`; it does not
translate JavaScript or native output. The implemented slice consumes `I32V1`, exports pure
functions over `i32`, validates and profile-audits every complete binary, and executes conformance
fixtures in a pinned runtime. `bool` will be enabled only by a later universal profile implemented
by every active backend.

The default-M1 `component` build wraps the unchanged audited core in a deterministic
Component Model artifact with exact canonical scalar lifts and metadata binding the authenticated
`zryna:capability-profiles/browser@0.1.0` WIT source closure. That capability world is empty; the
application scalar exports come from verified IR. Component external labels use the total,
reversible `zryna-export-<lowercase-hex-of-logical-name-bytes>` mapping because scalar ABI logical
names admit case and underscores that Component Model kebab labels do not. This mapping cannot
collide; the exact logical name and mapped label are both covered by the interface digest, while
existing JavaScript, core WebAssembly, and native ABI names remain unchanged. The wrapper rejects imports, starts, nesting,
wrong topology or identity, and resource excess before publication. It does not instantiate a
component, generate bindings, or provide browser, DOM, WASI, filesystem, network, clock, random,
or environment capabilities on the default, unprofiled route. The explicit
`browser-component-v1` route instead generates a bounded browser loader and declarations from
the sealed scalar interface. The loader authenticates the component bytes and instantiates only
the retained, import-free core module; it does not instantiate a Component Model artifact or
grant DOM, network, filesystem, clock, random, or environment access to guest code. General
browser component hosting and WASI host execution remain later integration boundaries. Component
artifact emission remains outside the advertised v0.1.0 preview support matrix.

## Language-server transport

The registered `zryna-language-server` application is a thin stdio transport over the driver's
revision-bound diagnostic session and scalar definition authority. It owns framing, in-memory
document lifecycle, URI/version correlation, negotiated coordinate conversion and cancellation;
it depends only on the driver orchestrator and source foundation. Protocol input cannot select a
frontend process or gain filesystem, build, runtime, network or mutation authority. The exact
supported methods and omissions are documented in [Language server protocol v1](LANGUAGE_SERVER.md).
The initial scalar editor client in `editors/vscode-zryna` is registered by the pnpm workspace;
it is not a Rust compiler component and adds no compiler dependency edge. It consumes diagnostics,
definitions and presentation-only edits without defining compiler rules. The driver derives bounded
formatting plans from verified, semantically accepted scalar snapshots. M2/M3 formatting remains
unsupported. The client VSIX carries no compiler/runtime binary or download mechanism. A separate
portable setup candidate binds the server, VSIX and unchanged signed compiler installation.
Installed tooling authenticates fixed runtime/provider bytes through driver-owned capture and
private staging, without a compiler checkout or additional application dependency edge.
