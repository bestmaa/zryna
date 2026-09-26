# Compiler status

Current public M3 selection is `--profile data-ownership-v1`, using protocol v4 and manifest v3.
See [the public M3 surface](M3_PUBLIC_PROFILE.md) and [the beginner guide](M3_GETTING_STARTED.md).
The component checkpoints below retain their historical implementation boundaries; they do not
limit or independently expand the integrated public surface. M0–M2 remain unchanged.

Status channel: `next`

Zryna v0.1.0 is an experimental, source-based Developer Preview. It is not
production-ready. It provides the documented repository-local M1–M3 workflows; it does not
include standalone binaries, package installation, or stable compatibility guarantees. The
`v0.1.0` tag and pre-release are not yet published. The [preview policy](DEVELOPER_PREVIEW.md)
freezes the proposed support matrix without publishing a release; the current executable profiles
remain intentionally narrow.

## Implemented M1 slice

- One workspace-relative `.zry` entrypoint is read through the pinned TypeScript 6 syntax provider,
  checked by Zryna semantics, and admitted through one verified Universal IR authority.
- The executable `I32V1` profile supports exported functions with explicit `i32` parameters and
  results, in-range decimal literals, parameter references, and signed wrapping `i32` addition.
- `zryna build` emits direct ECMAScript modules, import-free core WebAssembly modules, and audited
  Linux x86-64 ELF objects through explicit `javascript`, `webassembly`, `native`, or `all` target
  selection.
- Default-M1 `zryna build --target component` emits one deterministic audited Component Model
  artifact from the same verified program and unchanged scalar core. It binds the exact empty
  browser capability-world identity, rejects undeclared imports and malformed or excessive final
  bytes before publication, and records manifest target `component` with artifact kind
  `webassembly-component`. This default, unprofiled route is not part of `all`, cannot run, and
  remains outside the advertised [v0.1.0 preview support matrix](DEVELOPER_PREVIEW.md). The
  separate browser scalar profile below is the only admitted explicit component selection.
- Explicit repository-local `--profile browser-component-v1 --target component` builds the same
  audited scalar component with deterministic browser ESM bindings, declarations and an exact
  three-artifact manifest. The loader authenticates retained bytes and invokes only the sealed
  import-free core scalar functions in a browser. See the [browser quickstart](BROWSER_COMPONENT_V1.md).
  Installed package compatibility and guest host capabilities remain unavailable.
- `zryna run` executes JavaScript, core WebAssembly, and Linux x86-64 native artifacts for one typed
  scalar invocation and commits one complete create-only bundle.
- The M1 conformance suite observes `1 + 2`, `i32::MAX + 1`, and `i32::MIN - 1` through all three
  targets on Linux, compares fixed expected values and typed outcomes, and audits the manifest and
  exact artifact inventory.
- Linux and Windows both verify JavaScript/WebAssembly behavior, target-independent source
  rejection, Boolean scalar-carrier normalization, and repository portability. Windows native and
  `all` execution fail closed with `ZRYNA-N4002` and publish no bundle.

## Implemented M2 explicit profile

- The separate `zryna-syntax::v3` boundary defines exact provider-neutral DTOs for M2 syntax,
  verifies every graph, budget, spelling, and nested span against one authoritative source map,
  and exposes only opaque verified views. The pinned TypeScript 6 protocol-v3 worker and typed
  frontend transport provide the matching syntax-only implementation while retaining immutable
  protocol-v2 behavior.
- The isolated `zryna-ir::control_flow_v1` component verifies the frozen raw `ControlFlowV1`
  program model into opaque, source-map-bound views with bounded scalar operations, direct calls,
  explicit control-flow edges, dominance, reducibility, call-graph, ABI, and resource checks.
- The driver-owned [M2 module closure](M2_MODULE_CLOSURE.md) resolves only verified explicit
  relative `.zry` imports from retained no-follow workspace capabilities, authenticates exactly
  one final complete protocol-v3 source map, and seals canonical ordered modules, edges, source
  hashes, and a cross-platform graph identity under fixed resource limits.
- When invoked from the driver, the internal
  [M2 straight-line semantic boundary](M2_STRAIGHT_LINE_SEMANTICS.md) revalidates that exact final
  graph, owns modules, exports, scopes, exact scalar types, locals, assignment, and acyclic direct
  calls, preserves once-only left-to-right evaluation, and returns only mandatory-verifier-sealed
  `ControlFlowV1`. The internal [M2 control-flow boundary](M2_CONTROL_FLOW_SEMANTICS.md) adds
  canonical `if`/`while`, definite merge and loop state, reachability, and all-path return analysis.
  Independent callers must supply a complete source-map-bound verified snapshot.
- The internal [M2 deterministic JavaScript backend](M2_JAVASCRIPT_BACKEND.md) consumes only those
  opaque verified views. It exhaustively lowers exact scalar operations, private direct calls,
  returns, parallel jump edges, strict Boolean branches, and loops into bounded byte-deterministic
  ESM with typed `i32`/`bool` entry wrappers and sealed export aliases. Internal execution imports
  those public aliases through the existing pinned, deadline- and output-bounded Node capability.
- The internal [M2 direct core WebAssembly backend](M2_WEBASSEMBLY_BACKEND.md) consumes the same
  opaque verified views. It exhaustively lowers the exact scalar and CFG inventory into bounded,
  byte-deterministic WebAssembly 1.0 containing only type, function, export, and code sections,
  then validates and audits the complete bytes. Internal typed execution passes those same sealed
  bytes over standard input to an inline pinned Node host, with no staged-path reopen race.
- The internal [M2 verified native MIR profile](M2_NATIVE_MIR.md) lowers sealed `ControlFlowV1`
  one-for-one into deterministic target-specific modules, symbols, typed values, direct calls,
  blocks, parallel edges, and terminators, then independently verifies every raw claim before
  exposing opaque views and a rebuilt scalar ABI.
- The internal [M2 Linux x86-64 native backend](M2_NATIVE_BACKEND.md) consumes only that verified
  MIR, emits local typed bodies plus scalar-ABI wrappers, and closes the exact ELF section, symbol,
  and call-graph-bound relocation inventory before constructing an artifact. The driver prepares
  link/run only through the artifact-bound scalar ABI and retains the private staging identity.
- The public driver now composes those boundaries only when exact `--profile control-flow-v1` is
  present. It discovers and authenticates one complete module graph, lowers it once, dispatches the
  same opaque verified authority to selected targets, and commits one create-only atomic bundle
  with deterministic [`zryna-manifest-v2.json`](M2_MANIFEST_V2.md). Omitting `--profile` preserves
  protocol v2, `I32V1`, manifest v1, and every M1 command and bundle contract.
- Issue #55 makes individual explicit-profile M2 build/run requests available. Issue #56 adds the
  independent fixed-oracle three-target M2 conformance registry and aggregate required gate.
- The explicit `control-flow-v1` profile is implemented and covered by the required fixed-oracle
  Linux and Windows `m2` gate. Omitting `--profile` preserves the M1 syntax, semantics, CLI, and
  manifest-v1 contracts.
- Issue #57 records the separate authenticated website import, deployment, and live commit/digest
  evidence. This compiler status does not assert that an external website deployment has occurred.

## Specified M3 profile with internal syntax, layout, IR, semantics, and runtime ABI declarations

Issue #75 specifies the separate future `DataOwnershipV1` profile and exact CLI spelling
`data-ownership-v1`. The normative data/ownership, aggregate-layout, and ownership-runtime-ABI
documents plus the digest-pinned `tests/m3-contract-v1.json` registry freeze Issues #75–#90 and
their dependency graph.

The compiler-owned `zryna-layout` component now verifies complete source-map-bound raw type graphs,
assigns canonical dense TypeIds independent of discovery order, rejects by-value recursion and
unstorable borrows, computes checked `Linear32V1` and `LinuxX8664V1` structs, enums, fixed arrays,
and handle layouts, and seals exact SHA-256 layout documents behind opaque immutable views. Shared
machine-readable fixtures pin every normative layout row and the exact five-type Pair fingerprints.
The authority is not reachable from the M1/M2 driver or CLI, allocates no runtime memory, and emits
no target artifact.

The separate `zryna-syntax::v4` boundary now decodes a closed, bounded M3 syntax contract and
authenticates it against one exact final `SourceMap`. Its module-flat type arena and function
arenas preserve source order and exact UTF-8 spans for nominal struct/enum declarations,
compiler-known containers and references, aggregate construction, projection, matching, and
weak-upgrade syntax. The pinned TypeScript 6 protocol-v4 worker is syntax-only and advertises no
module-resolution or semantic authority. A typed frontend process boundary requires the exact v4
capability tuple and fails closed before exposing an opaque source-bound snapshot. Protocol v2 and
v3 behavior remains unchanged.

The isolated `zryna-ir::data_ownership_v1` boundary now accepts an untrusted M3 program only with
the exact final source map, independently selected entry file, and verified `Linear32V1` and
`LinuxX8664V1` layout snapshots. It proves the complete source and CFG structure, branded layout
types and projections, ownership transitions, borrow state, and exact cleanup plans before
constructing opaque immutable views. The verified program retains both layout authorities, scalar
ABI v1 for entry-module scalar exports, and the closed `OwnershipRuntimeV1` contract identity.

The internal `zryna-semantics::data_ownership_v1` boundary now consumes an exact verified
protocol-v4 source authority, owns nominal and exact type resolution, verifies the semantic type
graph for both admitted layout targets, and lowers recursively Copy structs, enums, and fixed
arrays into that sealed IR. Fixed-array projection is constant-only and checked statically. The
Pair results are observed by a test-only scalar evaluator over opaque verified views; this is not
a production interpreter or target execution path.

The separate internal ownership-runtime ABI v1 authority now verifies the exact 17-operation
declaration set, target symbols and signatures, authenticated `Linear32V1` and `LinuxX8664V1`
layout-derived records, checked C-header evidence, operation-bound atomic-failure status, sealed
element-layout Vec stride and checked byte amplification, and pure logical transitions before
exposing opaque immutable views. It does not allocate, mutate runtime state, implement a helper,
compile or link an object, lower a backend, or activate a driver or CLI route.

Issue #81 is complete at its bounded internal private compiler boundary. That boundary
supports String literals, explicit clone, checked concatenation, moves, return cleanup, and
root-local replacement, plus Vec construction, explicit clone for exact `Vec<bool>`, `Vec<i32>`,
and `Vec<String>`, moves, return, push, checked Copy-element indexing,
and replacement of supported exact Vec roots. Private zero-argument producers and one-argument
owned identity calls are available internally. String/Vec functions also admit one canonical
top-level no-phi `if`/`else` from a bool literal or Copy bool parameter; branch-local owners drop in
reverse, incoming owners are restored exactly, and mutation of an incoming Vec fails before its
right-hand side. Private String and exact Vec result functions additionally admit one bounded
terminal `if`/`else` as exactly three entry/then/else blocks: each arm returns one owned-producing
expression directly, no join block or parameter is created, and cleanup excludes the returned
owner. One bounded top-level
no-carried-owner `while` evaluates its bool condition in a canonical header, reverse-drops
iteration-local owners before the backedge, restores incoming ownership state on the backedge and
false exit, and permits only the final return afterward. Its stable-place subset supports prepared
replacement of one mutable outer String and Copy-element push into one mutable outer exact Vec
without an owned header phi; Vec replacement and owned-element Vec push remain excluded. Vec construction, push,
and calls reserve parent resources before child ownership changes. The aggregate route constructs,
moves, explicitly clones, returns, and drops bounded parameter-free private straight-line owned
Struct, FixedArray, and root Enum graphs with Copy/String leaves. Structural clone retains its
source, creates a distinct owner, derives its fallible String-leaf count and root-enum active variant
from sealed authorities, and reverse-drops only the initialized result prefix on element failure.
Whole-root assignment for the same graphs is prepare-before-commit, rejects direct
self-consumption, and preserves sealed recursive cleanup for the old destination.
The verified IR now also seals projected replacement's old-subobject traversal and transfers the
prepared subtree's masks and enum refinement without disturbing siblings. The semantic producer
uses it for prepare-before-commit replacement of mutable available static String leaves and for at
most one combined private straight-line aggregate site. That aggregate site moves or explicitly
clones either a complete static Struct/FixedArray subobject rooted in a distinct local or a distinct
fully initialized exact same-type supported non-Copy whole root into a mutable available
`StructField`/`FixedArrayConstant` projection. Move consumes the selected source; clone retains it,
and both clone failure paths retain source and destination. Commit
recursively drops only the old target and retains the destination root and sibling masks. The producer also
resolves canonical static StructField and
FixedArrayConstant source places for Copy reads, exact String-leaf moves, and at most one supported
Struct/FixedArray subobject move into an exact directly initialized same-type local. It materializes
the selected subobject's complete descendants, preserves the enclosing root's masked cleanup
obligation and disjoint siblings, and rejects repeated, overlapping, or later whole-root consumption
outside the exact direct-local, final-return, and whole-root assignment transfers described below.
One complete available static Struct/FixedArray subobject may now also move directly into the final
exact-type return of a parameter-free private straight-line function. The producer materializes
and masks its complete subtree, returns its unique temporary owner, excludes that owner from
reverse cleanup, and preflights the return cleanup plan and all pending survivor actions before
source mutation.
Initialized available String leaves under those same paths now admit explicit clone into a distinct
temporary owner; failure cleanup retains the enclosing root's exact partial-state masks, and cloning
a moved or overlapping leaf fails closed. One initialized available non-Copy Struct or FixedArray
projection under the same static paths may also be cloned into the immediately following exact
same-type local. The source root and masks are retained, the result has a distinct temporary owner,
and the verifier seals one private straight-line site with layout-derived prefix failure cleanup.
An exact-type direct local declaration now transfers one
partially moved supported Struct or FixedArray root through its move-result temporary into the new
local, materializing the complete static topology and migrating exact masks at both owner renames.
One final exact-reference return now transfers the same partial root into an exact-topology
temporary before cleanup; the verifier excludes the returned owner and reverse-drops every
survivor, while missing, extra, wrong, or unsupported topology fails closed.
One distinct mutable fully initialized same-type whole-root destination now accepts that partial
Struct or FixedArray from an exact-reference source. Complete source, temporary, and destination
topology plus value/place/transition capacity are preflighted before mutation; `ReplacePlace` drops
the old destination once, installs the exact mask, and invalidates source and temporary.
One combined private straight-line projected-assignment site now also moves or explicitly clones a
complete static Struct/FixedArray subobject between distinct local roots. The immediate source
operation -> sole-use typed temporary -> `ReplacePlace` shape drops only the old target subtree and
preserves both pending roots and sibling masks. Move materializes and masks the source subtree and
preflights one value, `S + D + T + 1` places, and two transitions. Clone retains the source without
descendant places and preflights one value, `S + T + 1` places, two transitions, two cleanup plans,
and `2P + 1` cleanup actions before any mutation.
One canonical private one-parameter route additionally accepts a single-variant enum whose complete
non-Copy Struct/FixedArray payload is bound by an exhaustive one-arm `match`. The arm moves the
active payload into an exact direct local, drops the emptied enum root, and jumps without owner
arguments to the final local return. Its checked model is three blocks, two edges, three values,
four ownership transitions, one zero-action cleanup plan, and `D + 5` places for `D` payload
descendants.
The private String route reports moved uses as M3011, the aggregate/enum route reports them
as M3014, and unresolved
binding names report M3002. The gate enforces one-plan/one-site cleanup roles and the cumulative
8 MiB String-literal limit, and returns sealed semantics retaining verified IR plus the exact
runtime ABI authority. An internal bounded fault/drop-trace oracle now covers every ABI-admitted
failure of the implemented String, Vec, and aggregate-clone allocation-bearing operations plus the
separate checked Vec bounds trap; it authenticates status disposition/trap identity, pre-commit operand retention,
uncommitted-result exclusion, reverse cleanup, deterministic replay, and event limits without
executing an allocator or target runtime. General structural Vec clone beyond String elements,
nested aggregate clone graphs containing Enum, Vec, Shared, or Weak values,
aggregate-subobject moves outside that direct-local or parameter-free final-return exception, the one distinct-root static
projection replacement, or the single-variant match-local enum payload extraction, broader
enum-payload moves, dynamic or Vec-element projections, projected aggregate assignment outside the
exact static-subobject-move-or-clone-or-whole-root-move-or-clone-to-static-projection site, projected aggregate
clone outside the direct-local or distinct-root static-replacement exceptions, partial Enum
transfer or partial-root transfer in call/CFG contexts, direct projected-clone returns, public
contexts, or non-final/non-reference returns, general owned phi joins and owned loop-carried phi
joins remain deliberately unavailable future extensions. The checked #271 route separately admits
repeated/nested branches and loops, general lexical scope exits, loop-body return and post-loop
continuation; `break` and `continue` remain excluded.
Issue #82 is complete at its bounded internal lexical-borrowing boundary.
Issues #113 through #117, #119, #120, and #121 freeze the bounded borrowing contract, retain the independent verified-IR
authority, and implement one internal private parameter-free literal-initialized `bool`/`i32` root
with shared or exclusive aliases. Straight-line aliases use one nested lexical block, conditional
aliases use one nested lexical scope per arm, and the loop body itself is #121's sole scope. Alias reads lower to `BorrowRead`;
assignment to const `BorrowMut<T>` is write-through and lowers to `BorrowWrite`; shared-from-shared
reborrow resolves to the same root. The full root conflict matrix, exclusive owner hiding, reverse
`EndBorrow` restoration, and exact read/write resources are preflighted before IR construction.
One canonical bool-root `if`/`else` additionally discharges every arm-local authority before its
jump and rejoins through four fixed blocks and four edges without borrow parameters, edge
arguments, or ownership-flow authority. Both arms are preflighted atomically; arm-local value and
transition costs sum while active capacity is the larger arm, and a complete borrow in only one
arm is valid. Issue #120 additionally admits static recursively Copy Struct-field and constant
fixed-array borrows. Canonical prefix paths make distinct static siblings disjoint while the same
path and ancestor/descendant pairs overlap; overlapping shared authority coexists, overlapping
exclusive authority fails, and disjoint exclusive siblings coexist. Dynamic indices, Vec/enum
roots, non-Copy roots, and unsupported projections fail before raw IR construction. Exact
projected resource formulas, complete replay traces, and independent IR move/replace/drop/call
tests retain the verifier as the final authority. One canonical bool-root loop now uses fixed preheader/header/body/exit blocks and discharges its
body-local authority before every backedge and restores exact root owner/initialization state at
the header. It carries no borrow authority, value block parameter, or edge argument. This adds no runtime,
ABI, backend, driver, CLI, artifact, website-support, or public-profile capability. The
completed Issue #116 implementation additionally admits exactly one
private parameter-free whole non-Copy root and one const shared alias in one lexical block. Reads
are limited to String clone/checked concat, exact `Vec<bool>`/`Vec<i32>` Copy indexing, and
supported whole Struct/root-Enum/fixed-array clone. Existing owned instructions and cleanup/fault
authorities retain the source and produce distinct owned results where applicable; `BorrowRead`
remains Copy-only. Projections, mutation, moves, calls, new runtime/backend behavior, and public
activation remain excluded. Issue #116 passed independent verification and required merge gates.
Issue #119 is complete at merged-main provenance
`32e3f0607389dd1274c21770088456c765ee4fb7`. Its bounded internal checkpoint admits private,
straight-line, exact-signature direct calls with recursively Copy results and value parameters,
one or more exact shared/exclusive whole-root borrow parameters in source order, left-to-right
argument evaluation, same-authority forwarding, caller-owned lexical `EndBorrow`, and call-trap
cleanup. The mandatory verifier retains final authority over exact access/referents, nonescape,
acyclic calls, and the 128-accepts/129-rejects static-depth boundary. The
`tests/m3-contract-v1.json` registry authenticates exactly 36 source/snapshot files, 5 accepted
cases, and 13 exclusions. Its merged registry SHA-256 is
`d61d1ec50005bbed7d86f029fa6ece5efa7517d495b6aed6e9b0f1c15f69e20f`; its canonical borrow-call
section SHA-256 is `ca7ca013771f8ebb0ddc3f7791bc46db6378892e89f3e8e570a44e42e687fc20`.
The current registry additionally tracks normative indexed-borrow prerequisites #254–#256 and
the now-integrated source-completion #269 authority before target support; its SHA-256 is
`dfa23281785a225042f32c082abef0f1cb61dd5971bb713625995d5ee0f51d22`.
This preserves the dependency identities; #269 closure is documented in
`M3_SOURCE_COMPLETION.md`. The #119 commit and digest above
remain immutable historical provenance; the borrow-call section and fixture bytes are unchanged.
The completed #277 integration contract, #278 non-handle owned core and #279 ownership CFG core
provided the reusable foundation for the completed #259–#264/#83 internal Shared/Weak compiler
chain. The current #270 closure candidate composes nested Struct/Enum/FixedArray/Vec ownership,
finite Vec-indirection recursion, structural clone, handle-containing static transfers, and
ordinary handle-aware Vec observation/replacement/push through mandatory verified IR. Its checked
matrix binds exact source, hostile-IR, resource and replay tests from #320–#323. Issue #271 is now
a checked compiler-only closure candidate: its exact #325 source/lexical, #326 hostile-IR, and #327
payload/fault/resource bindings are recorded in `M3_STRUCTURED_OWNED_CONTROL_FLOW_MATRIX.md`.
Issue #272 is a checked compiler-only closure candidate through #329 signature/identity,
#330 structured transfer/cleanup and #331 hostile-IR/resource evidence. Its exact bindings are in
`M3_OWNED_CALL_CLOSURE_MATRIX.md`. Issue #273 is now a checked compiler-only closure candidate
through #333 source/exhaustiveness, #334 independent refinement/ownership verification and #335
resource/overflow evidence; `M3_COMPLETE_ENUM_MATCHING_MATRIX.md` binds its exact rows. Issue #275
is now a checked compiler-only closure candidate through #337 owned roots/static projections, #338
active enum payloads, #339 lexical calls, and #340 hostile/resource evidence. Its exact enabled
bindings and retained exclusions are in `M3_NONINDEXED_OWNED_BORROWING_MATRIX.md`; #274 remains
separately completed and #269 now reconciles the complete compiler source/IR authority. Verified
instruction, cleanup and fault traces alone are not target execution. The internal #84/#85/#86
boundaries add deterministic JavaScript, audited memory-bearing core WebAssembly and independently
verified Linux x86-64 native MIR as documented in `M3_TARGET_BACKENDS.md`. Issue #87 now adds the
audited native object, exact ownership runtime, sealed link and real executable boundary. Issue #88
adds one authenticated internal driver route, strict manifest v3, actual three-target candidate
execution, and create-only whole-bundle publication. Aggregate conformance and public activation
remain open as #89 and #90.
Protocol v4 is unchanged; projected forwarding, call recursion, public borrow signatures and
retained borrow authority remain later or unavailable child work. The source-completion closure
itself added no driver, CLI, published artifact, website-support or public-profile capability.

The public compiler selects protocol v4 through exact `--profile data-ownership-v1`.
It executes the #89-conformant driver and publishes strict manifest-v3 bundles. Default M1
and explicit M2 remain separate public profiles. Website deployment is recorded by #90.

The internal #83 compiler boundary now provides explicit Shared/Weak construction, clone,
downgrade, deterministic release cleanup and one indivisible success/expired/overflow upgrade
contract across the frozen payload/control-flow matrix. Mandatory verified IR preserves source
ownership, original traps and deterministic replay. The internal target/runtime chain now executes
the supported bounded ownership surface; general aggregate ABI and public-profile claims remain
excluded.

The first executable slice remains an internal scalarizable `Pair` struct observed through scalar
ABI v1. Its candidate route now executes JavaScript, WebAssembly, and Linux x86-64 native artifacts
and publishes one atomic manifest-v3 bundle. The completed bounded owned String/Vec and lexical-borrow compiler boundaries remain internal;
#122 consolidates borrowing closure evidence.
Issue #83 internal compile-time semantics are complete. Later issues add fixed-oracle conformance
and authenticated public/website activation. Tracing GC,
public aggregate ABI,
raw pointers, unsafe, FFI, threads, WASI component hosts, component execution, custom allocators,
and freestanding targets
remain outside M3.

## Runtime and toolchain boundary

- JavaScript and WebAssembly execution require an absolute direct Node.js `22.22.1` executable.
- Native object emission is fixed to `x86_64-unknown-linux-gnu` and uses pinned pure-Rust Cranelift.
- Native executable linking and execution require canonical `/usr/bin/gcc` and GNU ld versions
  documented in the CLI and native executable specifications.
- Successful build and run output is published only below `.zryna/out` in atomic, create-only
  bundles with a deterministic profile-specific manifest. Windows native and `all` run reject
  before publication with `ZRYNA-N4002`; there is no partial-target fallback.

## Deliberately unsupported

Public source-level Boolean execution requires explicit `control-flow-v1` or `data-ownership-v1`; it remains rejected by
the default M1 path. The compiler-owned M2 gate checks three-target equivalence for one fixed
source control-flow and module oracle; it is not a claim of general language completeness. The
M1/M2 scalar profiles do not claim heap values, an allocator or a tracing-GC profile. Their
scalar-only surface is not a general zero-runtime or GC-free guarantee for data profiles.
Explicit M3 uses bounded owned allocation and deterministic cleanup under its
[public profile](M3_PUBLIC_PROFILE.md). The browser scalar profile above runs only its sealed,
import-free core through the generated browser loader; it does not instantiate a Component Model
artifact in a general component runtime. No current profile claims tracing GC, WASI/component
host execution, Windows or macOS native execution, static native executables, package resolution,
watch mode, incremental builds, or production readiness.

## Evidence and reference

- [CLI contract](CLI.md)
- [Compiler architecture](ARCHITECTURE.md)
- [M1 conformance evidence](M1_CONFORMANCE.md)
- [M2 deterministic module closure](M2_MODULE_CLOSURE.md)
- [M2 three-target conformance](M2_CONFORMANCE.md)
- [M2 manifest and atomic bundle contract](M2_MANIFEST_V2.md)
- [M2 straight-line semantics](M2_STRAIGHT_LINE_SEMANTICS.md)
- [M2 control-flow semantics](M2_CONTROL_FLOW_SEMANTICS.md)
- [M2 deterministic JavaScript backend](M2_JAVASCRIPT_BACKEND.md)
- [M2 direct core WebAssembly backend](M2_WEBASSEMBLY_BACKEND.md)
- [M2 verified native MIR](M2_NATIVE_MIR.md)
- [M3 Copy aggregate semantics](M3_COPY_AGGREGATE_SEMANTICS.md)
- [M3 verified data and ownership IR](M3_DATA_OWNERSHIP_IR.md)
- [M3 bounded borrowing implementation contract](M3_BORROWING_SEMANTICS.md)
- [M3 ownership runtime ABI authority](M3_OWNERSHIP_RUNTIME_ABI.md)
- [Roadmap](ROADMAP.md)
- [Aggregate layout v1](../spec/memory-model/AGGREGATE_LAYOUT_V1.md)
- [Syntax protocol v4](SYNTAX_PROTOCOL_V4.md)
- [Scalar ABI v1](../spec/abi/SCALAR_V1.md)
- [Language overview](../spec/language/OVERVIEW.md)
