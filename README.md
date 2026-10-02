# Zryna

Current public M3 selection is `--profile data-ownership-v1`, using protocol v4 and manifest v3.
See [the public M3 surface](docs/M3_PUBLIC_PROFILE.md) and [the beginner guide](docs/M3_GETTING_STARTED.md).

The separate [bounded WASI command candidate](docs/WASI_COMMAND_GETTING_STARTED.md) implements
one explicit environment lookup and empty-grant controls for review. Its complete acceptance
gates and independent final review remain pending; this does not change release support claims.
The component checkpoints below retain their historical implementation boundaries; they do not
limit or independently expand the integrated public surface. M0–M2 remain unchanged.

**Zryna** (pronounced *ZREE-na*) is developed at [zryna.com](https://zryna.com).

Zryna v0.1.0 is an experimental, source-based Developer Preview. It is not
production-ready. It provides the documented repository-local M1–M3 workflows; it does not
include standalone binaries, package installation, or stable compatibility guarantees.
The `v0.1.0` tag and pre-release are not yet published.

Its source syntax begins as a deliberately restricted TypeScript-compatible subset, while its
semantics, typed intermediate representation, JavaScript output, WebAssembly output, and native
output belong to Zryna. See the [preview scope and release policy](docs/DEVELOPER_PREVIEW.md).

## Goal

One checked source program should produce three artifacts with specified matching behavior:

```text
.zry source
    ↓
replaceable frontend reader
    ↓
Zryna syntax snapshot
    ↓
Zryna strict semantics
    ↓
verified Universal IR
    ├── direct JavaScript backend → .mjs
    ├── direct WebAssembly backend → .wasm
    └── native lowering → MIR → codegen → executable
```

The TypeScript 6 adapter is a bootstrap reader, not the language authority. It does not define Zryna types, build Zryna IR, or emit JavaScript. A future TypeScript 7 provider and a native Zryna frontend must implement the same versioned frontend contract.

## Non-negotiable rules

- `any` and implicit `any` are not part of the Zryna universal profile.
- Exact numeric types have target-independent specified behavior.
- JavaScript, WebAssembly, and native backends consume only verified Zryna IR.
- Backend crates never depend on a TypeScript adapter.
- The TypeScript adapter never constructs Zryna IR.
- Architecture validation is required to fail closed; the M0 conformance registry and required
  Linux and Windows checks prove the scanner, manifest, dependency graph, and compiler trust
  boundaries before later compiler work can merge.
- Build, test, package, and release flows may not provide a skip-architecture switch.
- Generated output stays inside declared output roots.
- Symlinks are forbidden inside controlled source components.
- Unsupported functionality must produce a stable diagnostic, never a silent fallback.

## Current vertical slice

The repository currently establishes and tests:

- an authoritative `zryna.workspace.json` contract;
- an isolated Windows filesystem foundation for atomic handle-returning directory creation,
  same-handle no-replace rename, and exact-handle empty cleanup; it grants no public filesystem API
  or platform support by itself;
- a Rust architecture validator and `zryna architecture check` command;
- an exact provider-neutral frontend handshake and fail-closed protocol-v2 executable syntax;
- a fail-closed executable syntax protocol v2 with a shared JSON Schema, bounded flat
  expression arenas, and source-map-backed verified Rust types;
- an isolated TypeScript 6 syntax adapter with its compiler implementation locked to `6.0.3`;
- a separate internal protocol-v4 M3 syntax contract with bounded nominal data declarations,
  flat type and expression arenas, exact UTF-8 spans, a syntax-only TypeScript 6 worker, and an
  opaque source-map-bound Rust verifier; this does not activate `data-ownership-v1` or change
  protocol v2/v3;
- an internal aggregate-layout authority, a Copy struct/enum/fixed-array semantic lowerer, and a
  separate `DataOwnershipV1` Universal IR verifier that bind one exact source, target-neutral type
  universe, both admitted storage layouts, scalar ABI, and the declared ownership-runtime contract
  identity behind opaque read-only views; the completed internal Issue #81 boundary additionally lowers
  bounded private functions with String literals, explicit clone, checked
  concatenation, moves, return cleanup, and mutable root-local replacement, plus canonical Vec
  construction, explicit clone for exact `Vec<bool>`, `Vec<i32>`, and `Vec<String>`, moves, return,
  push, checked Copy-element indexing, and supported root-local replacement. String/Vec producers and
  one-argument owned identity calls are supported internally,
  together with one canonical top-level no-phi `if`/`else`; branch-local owners drop in reverse and
  incoming Vec mutation fails before right-hand-side evaluation. A separate bounded terminal
  String/exact-Vec `if`/`else` uses a canonical three-block entry/then/else graph; each arm returns
  its owned-producing result directly with no join block or block parameter and excludes that
  returned owner from cleanup. One bounded top-level no-carried-owner
  `while` reevaluates its bool condition in a canonical header, reverse-drops iteration-local
  owners before its backedge, restores the exact incoming state on both loop edges, and then
  reaches one final return. Its stable-place subset may replace one mutable outer String after
  complete RHS preparation or push a Copy element into one mutable outer exact Vec; the outer
  place identity remains unchanged and no owned header phi is introduced. The same parameter-free private
  straight-line aggregate gate now constructs, moves, explicitly clones, returns,
  and deterministically drops bounded owned Struct, FixedArray, and root Enum graphs containing
  Copy or String leaves. A fallible structural clone derives the exact String-leaf count and active
  root-enum variant from sealed layout and verifier state, retains its source, and reverse-drops only
  the initialized result prefix before pre-existing roots. Mutable whole-root assignment for those
  supported aggregates prepares a distinct replacement while preserving the old root, then commits
  with sealed recursive old-value cleanup. Canonical static struct-field and constant fixed-array
  projections support Copy reads and exact String-leaf moves; disjoint leaves remain available,
  the enclosing root retains its cleanup obligation, and repeated, overlapping, or subsequent
  whole-root consumption fails closed. Mutable available String projections additionally support
  prepare-before-commit assignment: preparation retains the enclosing root and commit drops only
  the exact old leaf without disturbing sibling masks. Initialized available String leaves under
  those same paths also support explicit clone: clone preparation retains the enclosing root,
  creates a distinct temporary owner, and preserves existing moved/initialized sibling masks.
  At most one initialized available non-Copy Struct or FixedArray projection under those static
  paths may likewise be cloned into the immediately following exact same-type direct local. The
  source remains owned by its enclosing root, recursive String-leaf failure cleanup is derived
  from sealed layout, and the verifier rejects alternate, public, CFG, or second-site contexts.
  At most one complete available non-Copy Struct or FixedArray projection under those same paths
  may instead move into the final exact-type return of a parameter-free private straight-line
  function. The producer materializes and masks the complete source subtree, returns its unique
  temporary owner, and atomically reserves return cleanup before any source mutation.
  At most one projected aggregate assignment site may move or explicitly clone a complete static
  Struct/FixedArray subobject between distinct local aggregate roots, move a distinct whole root,
  or explicitly clone that whole root into a mutable available exact same-type static projection.
  Each subobject form is an immediate source operation -> sole-use typed temporary ->
  `ReplacePlace` sequence. Move masks the complete source subtree beneath the still-pending source
  root. Clone retains the source subtree without requiring materialized descendant places and
  derives recursive failure cleanup from sealed layout. Both forms recursively drop only the old
  target subtree and preserve both roots' sibling masks and pending order; clone prepare and
  initialized-prefix failure cleanup retain both source and destination.
  One canonical private one-parameter function may additionally extract the complete supported
  Struct/FixedArray payload of a single-variant Enum through an exhaustive one-arm `match`, move it
  into an exact direct local, drop the emptied Enum root, and return that local through a
  zero-argument continuation.
  It proves one exact non-Copy owner while
  preserving addressable Copy storage and binds every cleanup plan to one exact site and role. The
  private String route reports moved
  bindings as M3011; the bounded aggregate and enum route reports moved owners as M3014;
  unresolved binding names report M3002;
  cumulative 8 MiB String-literal limits fail closed. A bounded internal fault/drop-trace oracle
  authenticates every admitted implemented String/Vec/aggregate-clone failure disposition and trap identity,
  preserves pre-commit owners, excludes uncommitted results, and pins reverse cleanup without
  executing an allocator or target runtime. General structural Vec clone beyond String elements,
  nested aggregate clone graphs containing Enum, Vec, Shared, or Weak values, aggregate-subobject
  moves outside the exact direct-local forms, broader enum-payload moves, dynamic or Vec-element
  projections, projected aggregate clone outside the exact direct-local or distinct-root
  static-projection forms, and projected aggregate assignment outside the one static-subobject-
  move-or-clone-or-whole-root-move-or-clone-to-static-projection checkpoint,
  whole-partial-owner transfer, general owned phi joins, owned loop-carried phi joins, and public
  owned values remain deliberately unavailable future extensions; Vec loop replacement and
  owned-element Vec loop push remain excluded. The #271 route separately admits nested/repeated
  branches and loops, lexical scope exits, loop-body return and post-loop continuation; `break`
  and `continue` remain excluded;
- a first internal shared-borrow semantic checkpoint for one private parameter-free literal-
  initialized `bool` or `i32` root: const aliases in one nested block lower to dense shared begin
  and Copy-read authority, end in reverse lexical order, and leave the root readable afterward.
  The producer preflights exact resources before raw-IR construction, the existing mandatory IR
  verifier remains authoritative. The same root-only gate now admits one exclusive alias over a
  mutable `bool` or `i32` root: const-alias assignment is write-through, exact Copy reads and
  writes lower to verified borrow authority, owner access is hidden until reverse lexical end, and
  shared-from-shared reborrow resolves to the same root. The complete shared/exclusive conflict
  matrix and resources are preflighted before IR construction. One canonical private bool-root
  conditional now discharges complete shared, exclusive, or mixed arm-local lexical scopes before
  four fixed branch/jump edges and rejoins without borrow block parameters or edge arguments. A
  complete borrow in only one arm is valid; arm-local value and transition costs sum while
  active-borrow capacity uses the larger arm. Static recursively Copy Struct fields and constant
  fixed-array elements now retain exact canonical borrow paths: siblings are disjoint while the
  same path and ancestor/descendant pairs overlap. Projected value/place/transition resources are
  preflighted exactly; dynamic/Vec/enum/non-Copy projections fail closed. One bounded private
  straight-line exception now passes active whole-root shared or exclusive authority through exact
  recursively Copy direct-call signatures. Arguments evaluate left to right, borrow parameters
  retain source signature order, forwarding preserves the same authority, the caller owns lexical
  end, and the mandatory verifier retains acyclic-call and static-depth authority. Projected
  forwarding, repeated calls, CFG crossing, recursion, public borrow signatures, and retained or
  escaping authority remain excluded. Mutable-from-shared, every reborrow from exclusive,
  nested/repeated conditionals, lifetime shortening, and every
  runtime, ABI, backend, driver, CLI, artifact, and public-profile capability remain unavailable;
  one canonical private bool-root loop additionally reverse-ends its body-local authorities before
  a fixed backedge, restores the exact root owner/initialization state at the header, and carries
  no borrow authority, value block parameter, or edge argument. The completed Issue #116
  implementation also admits one private parameter-free whole non-Copy root with one
  const shared alias in one lexical block: String clone/checked concat, exact `Vec<bool>` or
  `Vec<i32>` Copy indexing, and supported whole Struct/root-Enum/fixed-array clone reuse the
  existing owned operations and cleanup/fault authorities. The source remains initialized and is
  returned after lexical end; every owned read result is distinct. `BorrowRead` remains Copy-only,
  and projections, mutation, moves, calls, runtime/backend work, and public activation remain
  excluded. Issue #116 is complete after independent verification and required merge gates;
- completed internal explicit Shared/Weak construction, clone, downgrade, deterministic release
  cleanup, and a sealed success/expired/overflow Weak-upgrade contract across the frozen payload
  and control-flow matrix. Mandatory verified IR and non-executable control/fault/resource/cycle
  evidence close the compiler boundary without an allocator, runtime, backend, driver, CLI,
  public M3 profile, or target execution;
- an internal generic owned-composition closure candidate covering nested aggregate/container
  construction and transfer, structural clone, ordinary Vec observation/replacement/push,
  handle-containing static subobjects, and finite Vec-indirection recursion. Exact source,
  hostile-IR and resource/replay bindings are checked without claiming runtime execution,
  backend support or public activation; #271–#273 and #275 are separately checked and their
  combined source/IR authority is closed by the #269 integration matrix;
- an internal ownership-runtime ABI v1 authority that verifies the exact 17-operation declaration
  set, target symbols and signatures, authenticated layout-derived records, checked header evidence,
  Vec allocation/reserve rules, and all 12 canonical Shared/Weak control transitions behind opaque
  immutable views; raw-storage and String behavior remain sealed declarations rather than executed
  state models, and it supplies no allocator, runtime, backend, artifact, driver route, CLI selector,
  or public aggregate ABI;
- internal deterministic `DataOwnershipV1` JavaScript and memory-bearing core WebAssembly
  emission, plus independently verified Linux x86-64 native MIR, deterministic audited objects,
  an exact C11 ownership runtime, sealed linking, create-only artifact publication, and typed
  execution. These consume only exact sealed IR/layout/runtime authorities and do not activate a
  public driver profile;
- an internal authenticated `DataOwnershipV1` driver route that shares one final protocol-v4
  closure and verified program across selected targets, executes runs inside one private
  transaction, and publishes a strict manifest-v3 bundle with one create-only commit. Exact public
  `--profile data-ownership-v1` uses this same conformant route;
- Zryna-owned name resolution, strict source checking, and deterministic lowering from a verified
  protocol-v2 snapshot to unverified Universal IR;
- a driver-owned authenticated source-to-verified-IR path that preserves provider warnings and
  stops provider, semantic, or IR errors before any backend;
- a bounded, sealed `I32V1` Universal IR trust boundary for `i32` parameters, literals, and
  wrapping addition;
- a verified scalar ABI v1 authority for `i32` and `bool` signatures, deterministic JavaScript,
  core WebAssembly, and Linux x86-64 export mappings, and typed host observations;
- deterministic direct ECMAScript-module emission from verified IR, with strict scalar ABI
  argument and result validation for the current `i32` execution profile;
- driver-owned, create-only publication of complete `.mjs` artifacts through a validated
  capability for the workspace's declared `.zryna/out` directory;
- deterministic direct core WebAssembly 1.0 emission from verified `I32V1` IR, with sealed export
  names, an import-free capability audit, pinned binary validation, and create-only `.wasm`
  publication through the same validated output capability;
- an isolated M4 prerequisite that authenticates the accepted WIT source closure, resolves all
  exact WASI `0.2.12` dependencies with pinned `wit-parser 0.258.0`, and independently audits the
  existing browser, command, and server interfaces;
- an explicit default-M1 `component` build that preserves the audited scalar core, canonically
  lifts verified exports, binds the exact empty browser capability-world identity, independently
  maps every logical export to a collision-free `zryna-export-<lowercase-hex>` component label,
  audits the final Component Model bytes, and publishes one create-only manifest-v1 bundle without
  generating bindings, instantiating a host, or granting capabilities; this repository-development
  surface remains outside the advertised [v0.1.0 preview support matrix](docs/DEVELOPER_PREVIEW.md);
- native MIR lowering through an independent `VerifiedMirModule` gate that retains scalar ABI v1
  authority, plus deterministic Linux x86-64 ELF relocatable-object emission and create-only
  `.o` publication;
- driver-owned discovery of one canonical Linux GNU link toolchain, sealed one-invocation harness
  generation, audited create-only `.elf` publication, bounded execution, and full-width typed
  `i32` result observation;
- public `zryna build` and `zryna run` commands for explicit `javascript`, `webassembly`, `native`,
  and `all` selections, with mandatory architecture validation, one shared verified program,
  typed `i32` invocations, and create-only atomic output bundles.
- a deterministic M2 ECMAScript backend that consumes only sealed `ControlFlowV1`
  views, lowers exact scalar operations, direct calls, branches, loops, and parallel block edges,
  and enforces typed `i32`/`bool` entry wrappers.
- a direct M2 core WebAssembly backend that consumes the same sealed views, emits only
  capability-minimal audited core bytes, and executes typed `i32`/`bool` exports from those exact
  validated bytes.
- an M2 Linux x86-64 native backend that consumes independently verified native MIR,
  emits local typed bodies plus scalar-ABI wrappers, audits exact call-graph-bound ELF relocations,
  and links/runs typed `i32`/`bool` invocations.
- an explicit `--profile control-flow-v1` public path that authenticates one bounded multi-file
  module graph, lowers it once, dispatches the same sealed authority to selected targets, and
  commits one deterministic `zryna-manifest-v2.json` atomic bundle. Omitting `--profile` preserves
  every M1 CLI and manifest-v1 contract.

The internal #278 mixed-data route additionally supports repeated whole-root replacement of
mutable fully initialized mixed Struct/Enum/FixedArray/Vec locals. It prepares the RHS before
dropping the old value and retains the destination on rejected preparation. This is a bounded
private straight-line source/verified-IR capability, not public M3 or target execution; see the
[composition evidence](docs/M3_OWNERSHIP_COMPOSITION_EVIDENCE.md).

### Default M1 profile

When `--profile` is omitted, the TypeScript adapter emits protocol v2 and rejects parse errors or unsupported syntax without
silently producing a smaller program. The first strict semantic subset requires one source file,
one or more exported functions, explicit `i32` or `bool` annotations, parameter references,
literals, one return, and `i32` addition. `any`, missing annotations, invalid names, unresolved
references, unsupported types, invalid arithmetic, and unsupported entrypoint shapes produce
stable diagnostics. Source-level `bool` is checked and lowered, but the current `I32V1` verifier
intentionally rejects it until every backend implements the same profile; only the documented
`i32` subset currently reaches `VerifiedProgram`.

The JavaScript and WebAssembly backends consume the sealed scalar ABI mapping for the current
`I32V1` profile. JavaScript enforces canonical host carriers and exact arity. WebAssembly emits
only type, function, export, and code sections; it has no imports or ambient capabilities. Every
module passes an explicitly pinned WebAssembly 1.0 validator and narrower structural/operator
audit before publication. Both targets preserve wrapping `i32` addition and run the shared source
fixture under conformance tests. The native backend uses pinned pure-Rust Cranelift, selects only
`x86_64-unknown-linux-gnu`, and audits the encoded ELF object without invoking an installed tool.
Separately, the driver can prove canonical `/usr/bin/gcc` and GNU ld capabilities, link one sealed
typed invocation through a generated C11 harness, audit and create-only publish an executable, and
observe its exact four-byte result under bounded process controls. The public CLI composes these
library boundaries for the current `I32V1` slice. The repository-owned
[M1 conformance suite](docs/M1_CONFORMANCE.md) runs its ordered observations against fixed expected
values and each other on JavaScript, core WebAssembly, and Linux x86-64 native. Source-level `bool`
remains verifier-gated; Boolean evidence is limited to the typed scalar-ABI carrier contract.

The explicit M2 `--profile control-flow-v1` path is separate from that preserved default. It
accepts the documented multi-file scalar control-flow subset, including typed `bool`, and reaches
the sealed M2 verified program consumed by all three backends. See
[M2 conformance](docs/M2_CONFORMANCE.md) for the exact supported and rejected boundaries.

## Build and run

Start with [your first Zryna programs](docs/GETTING_STARTED.md) for a setup, build, run and rerun
walkthrough using the checked-in addition and control-flow examples.

The public CLI accepts exactly one workspace-relative `.zry` entrypoint and requires an explicit
target. That is the complete M1 source set when `--profile` is omitted and the root of one explicit
relative-import module graph under `--profile control-flow-v1`. These examples use an exact Node.js
22.22.1 executable:

```bash
cargo run --locked -p zryna -- build examples/universal/add.zry --target all --name add-all --node /absolute/path/to/node
cargo run --locked -p zryna -- build examples/universal/add.zry --target component --name add-component --node /absolute/path/to/node
cargo run --locked -p zryna -- run examples/universal/add.zry --target all --name add-all --export add --arg=i32:20 --arg=i32:22 --node /absolute/path/to/node
cargo run --locked -p zryna -- build src/main.zry --profile control-flow-v1 --target all --name app-m2 --node /absolute/path/to/node
```

Selected M1 builds publish `.mjs`, core `.wasm`, `.o`, or component `.wasm`; `all` remains the
original three core targets. Run publishes `.mjs`, `.wasm`, and an invocation-
specific `.elf`. The selected artifacts and deterministic manifest appear together only after one
create-only atomic bundle commit below `.zryna/out`. M1 writes manifest v1; the explicit M2 profile
writes [manifest v2](docs/M2_MANIFEST_V2.md). See the [CLI reference](docs/CLI.md) for exact
syntax, single-target commands, typed argument grammar, output layout, JSON, exit statuses,
security properties, and platform limits. The repository-owned
[M2 conformance gate](docs/M2_CONFORMANCE.md) now authenticates the fixed-oracle aggregate
three-target evidence. The `next` compiler status therefore includes the explicit
`control-flow-v1` profile while omission of `--profile` preserves M1. Issue #57 records the
separate authenticated website import, deployment, and live-provenance evidence; that external
publication boundary does not broaden the compiler's implemented surface.

## Run the foundation gate

Requirements:

- Rust 1.97.1
- exact Node.js 22.22.1 for the authenticated frontend and CLI JavaScript/WebAssembly host
- pnpm 11.18.0
- Linux `/usr/bin/gcc` targeting `x86_64-linux-gnu` (GCC 12–15) with GNU ld 2.38–2.46 for native
  executable conformance; pure object generation does not require it

```bash
pnpm install --frozen-lockfile
pnpm m3:quick
pnpm m3:registry
pnpm m3:check
pnpm m3:runtime-abi:quick
pnpm m3:owned:quick
pnpm m3:data:quick
pnpm m3:syntax:quick
pnpm m2:quick
pnpm preflight
pnpm m0:check
```

`pnpm m3:data:quick` is the narrowest Issue #79 edit-loop check. It authenticates the protocol-v4
fixtures, derives both aggregate layouts, verifies the Copy-only data IR, and runs the internal
Pair oracle without building public backends or waiting for remote CI. It skips only the large
exact/first-extra authenticated value-budget proof; `pnpm preflight` retains that boundary test for
the final local gate.

`pnpm m3:owned:quick` is the seconds-scale Issue #81 foundation check. It runs the focused
DataOwnershipV1 IR and semantic tests plus both modules' opacity doctests, including bounded UTF-8
String creation, clone and checked concatenation, Vec construction/exact Copy- or String-element
clone/push/Copy indexing, owner
transfer, private root-local replacement, site-bound cleanup roles, partial drop metadata, and the
retained runtime-ABI authority. It also covers canonical static Struct/FixedArray Copy projection
reads, String-leaf moves, initialized String-leaf clone, exact direct-local static aggregate clone,
one parameter-free final-return static aggregate move, prepare-before-commit String-leaf
assignment, and one private straight-line complete static
Struct/FixedArray subobject move or clone between distinct local roots, whole-root move, or explicit
whole-root clone into a mutable available same-type static aggregate projection. Subobject move
masks its source subtree; subobject clone retains its source without descendant place expansion and
seals layout-derived prefix failure cleanup. Both recursively drop only the old target subtree and
preserve roots, pending order, and sibling masks; whole-root clone retains its source and both clone
failure paths retain source plus destination; whole-root move consumes its source and retains the
destination. It does not claim projected aggregate clone outside
the direct-local or distinct-root static-replacement forms, same-root/overlapping, partial or already-moved subobject sources, dynamic
or Vec/Enum projections, broader aggregate/enum subobject moves, parameterized projected returns,
owned calls/CFG, general scope
exits, a runtime, a
backend, a CLI route, or a public profile. The full `pnpm preflight` gate
retains ignored proportional boundaries and the complete repository regression surface.

`pnpm m3:runtime-abi:quick` is the focused Issue #80 declaration-authority check. It verifies the
exact operation vocabulary, canonical JSON fixture, target declarations, layout binding, checked
header evidence, operation-bound atomic-failure claims, sealed-layout Vec stride and checked byte
amplification, Shared/Weak transitions, opacity, and ordinary negative cases without claiming or
executing a runtime. It runs 25 ordinary unit tests and two compile-fail doctests while leaving
three proportional boundary tests ignored. The full `pnpm preflight` gate includes all 28 unit
tests and both compile-fail doctests.

`pnpm m2:quick` is the narrowest edit-loop check for deterministic M2 JavaScript, WebAssembly, and
native objects; native MIR; module closure; retained workspace and native-stage security; and
internal M2 semantics. On Linux x86-64 it also links and runs the typed M2 call/branch/loop fixture;
other hosts prove the closed unsupported-host boundary. The native MIR command runs both the
unchanged M1 proof and the separate M2 verifier. This command avoids the broad driver/runtime
integration suite and is suitable on both Linux and Windows before running the broader gate.

`pnpm preflight` is the fast edit-loop gate. It stops on the first portable contract, formatting,
M2 driver-security, workspace-check, frontend, or syntax failure. Driver tests run before the
broader workspace check so closure and retained-filesystem regressions fail early. The complete
`pnpm m0:check` command remains the cross-platform merge authority.

The canonical command validates and executes every registered Rust, protocol, and adapter proof
suite without a skip-architecture mode. See [M0 conformance](docs/M0_CONFORMANCE.md) for the exact
coverage and unsupported status.

See [M3 fixed-oracle conformance](docs/M3_CONFORMANCE.md) for candidate evidence,
resource gates and the exact public-profile corpus.

## Repository map

```text
apps/        user-facing commands
crates/      Rust compiler contracts and phases
adapters/    isolated replaceable frontend providers
runtime/     target runtimes behind stable ABI contracts
spec/        normative language and target semantics
docs/        architecture, workflows, and roadmap
examples/    small conformance-oriented source programs
schemas/     editor-facing schemas; runtime validation remains authoritative
tests/       cross-component and cross-target suites
toolchains/  pinned upstream toolchain metadata, never patched source
editors/     future thin editor integrations
```

See [CLI reference](docs/CLI.md), [Architecture](docs/ARCHITECTURE.md), [Syntax protocol v2](docs/SYNTAX_PROTOCOL_V2.md),
[Strict workspace contract](docs/STRICT_WORKSPACE.md), [Frontend providers](docs/FRONTENDS.md), and
[M2 deterministic module closure](docs/M2_MODULE_CLOSURE.md),
[M2 control-flow semantics](docs/M2_CONTROL_FLOW_SEMANTICS.md),
[M2 deterministic JavaScript backend](docs/M2_JAVASCRIPT_BACKEND.md),
[M2 direct core WebAssembly backend](docs/M2_WEBASSEMBLY_BACKEND.md), [Roadmap](docs/ROADMAP.md),
[M3 ownership runtime ABI authority](docs/M3_OWNERSHIP_RUNTIME_ABI.md),
[M3 bounded borrowing implementation contract](docs/M3_BORROWING_SEMANTICS.md),
[M3 Shared/Weak compiler authority and evidence](docs/M3_SHARED_WEAK_AUTHORITY.md),
[M3 Shared/Weak evidence ledger](docs/M3_SHARED_WEAK_EVIDENCE.md),
[M3 planned ownership composition](docs/M3_OWNERSHIP_COMPOSITION.md),
[M3 generic owned composition closure matrix](docs/M3_GENERIC_OWNED_COMPOSITION_MATRIX.md),
[M3 structured owned control-flow closure matrix](docs/M3_STRUCTURED_OWNED_CONTROL_FLOW_MATRIX.md),
[M3 private generic function operations](docs/M3_GENERIC_FUNCTION_OPERATIONS.md),
[M3 canonical structural clone](docs/M3_GENERIC_CLONE_CORE.md),
[M3 complete static subobjects](docs/M3_GENERIC_STATIC_PLACES.md),
[M3 ordinary generic Vec operations](docs/M3_GENERIC_VEC_OPERATIONS.md),
[M3 opaque typed handle slots](docs/M3_OPAQUE_HANDLE_SLOTS.md),
[M3 Copy aggregate semantics](docs/M3_COPY_AGGREGATE_SEMANTICS.md),
[M3 owned-data semantic design contract](docs/M3_OWNED_DATA_SEMANTICS.md),
[M3 candidate driver and manifest](docs/M3_CANDIDATE_DRIVER.md),
[M0 conformance](docs/M0_CONFORMANCE.md), and
[compiler documentation bundles](docs/DOCUMENTATION_BUNDLES.md).

## Project identity

Zryna is an independent project and is not affiliated with, endorsed by, or sponsored by Microsoft. TypeScript is a trademark of the Microsoft group of companies. Compatibility references describe technical interoperability only.

- Website: [zryna.com](https://zryna.com)
- Source: [github.com/zryna/zryna](https://github.com/zryna/zryna)

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
