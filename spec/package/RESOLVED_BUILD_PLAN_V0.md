# Resolved build plan v0

Status: integrated source-only contract with a **proposed D2 native alignment** for
[#361](https://github.com/zryna/zryna/issues/361), within M5. The schema and checker validate
synthetic records only. They do not resolve packages, read a workspace, acquire inputs, execute a
tool, compile, link, populate a cache, or publish an artifact. Existing compiler/runtime behavior,
public profiles, registries, selectors, output manifests, and M3 gates remain unchanged.

## Authority and dependency boundaries

Package and provenance authority remains with #168. Its accepted
[`zryna.package.v1` and `zryna.lock.v1` contract](PACKAGE_RELEASE_V1.md) supplies canonical
manifest, `source-files`, and lock digests. In this plan, `package.id` is #168's manifest digest,
`package.sourceSha256` is #168's `source-files` digest, and `packageLockSha256` is #168's lock
digest. The fixture checker first runs #168's complete validator over canonical bytes and then
projects its authenticated lock digest, compatibility compiler/profile/target coverage, root,
exact package pairs, aliases, and edges. A caller
cannot supply an unvalidated lookalike projection. This document imports #168's checker and
serializer helpers and does not redefine their serialization or digest domains. Release subjects,
SBOM, signed notes, rollback and provenance remain entirely #168-owned.

[#360](https://github.com/zryna/zryna/issues/360) owns exact package-instance identity and
host/target dependency-graph decisions. The package identity is the exact ordered pair
`(id, sourceSha256)` from one validated #168 lock package record; there is no additional shorthand
identity field. #360 defines the distinct `target/runtime` and `host/build` graph roles, but the
source-only v0 record admits only `target/runtime` occurrences. It has one authenticated root and
exactly the complete #168 lock graph. A `host/build` package, source, or edge therefore rejects;
admitting that role requires a later version with its own authenticated root and edge authority.
A #168 lock edge selects the package record by `id`; its `sourceSha256` is then recovered from that
record. Because #168 has no build-edge kind, its edges remain `target/runtime` and must not be
reinterpreted as a mixed graph.

The accepted [cross-target composition contract](../language/CROSS_TARGET_PROFILES_V1.md) supplies
identity `zryna.cross-target-profiles.v1`, exact language profiles, output axes and composition row
IDs. A resolved plan expands public `all` before serialization and binds each exact #168 target to
one compatible #357 row plus authenticated host-policy and approved-request digests. The plan does
not grant a capability, reinterpret WIT identities, or introduce a selector.

The package layer may produce a resolved input description after validating the package authorities.
It does not receive compilation, linking, execution, cache-write, or publication authority. The
driver alone owns compilation orchestration, tool validation, linking, cache materialization, and
output publication. Backends still consume only verified IR or MIR. Existing workspace manifests
and output manifests retain their distinct meanings; a resolved plan cannot replace either one.

[#362](https://github.com/zryna/zryna/issues/362) owns acquisition trust and build-execution policy.
The source plan binds an opaque `executionPolicy` identity, its version and configuration digest,
plus every output-relevant environment entry. This makes a policy or admitted-environment change a
cache miss without allowing this contract to decide whether a source, tool, or execution context is
trusted. It also does not authorize acquisition, grant a trust exception, or define isolation.
#362 remains the authority for those decisions and their evidence. No undeclared environment
variable or executable is available to a plan consumer.

The legacy native appendix stays `provisional-pending-364`. [PR #393](https://github.com/zryna/zryna/pull/393)
accepted the source-only plan and source policy, explicitly leaving the native appendix a draft.
The subsequently integrated [#364 review](../abi/NATIVE_C_INTEROP_V0_REVIEW.md#exact-identity-and-importexport-layout)
specifies the exact C ABI tuple; it does not retroactively accept #361's appendix or prove FFI.
Source-only v0 acceptance does not wait for this appendix or the rest of M7.

The prospective D2 successor below defines two explicit states for the same specified tuple:
`proposed-specified-native-c-interop-v0` and specification-only `specified-native-c-interop-v0`.
Approval and integration of this successor remain pending in the separate decision record. Neither
state supplies acquisition, runtime artifact, execution, cache-reuse, linking, publication or grant
authority. The driver and #362 remain their owners; no existing record is automatically upgraded.

## Closed source-only record

The closed JSON Schema is
[`schemas/zryna-resolved-build-plan-v0.schema.json`](../../schemas/zryna-resolved-build-plan-v0.schema.json).
The root record has format `zryna.resolved-build-plan.v0`, numeric version `0`, status
`specified-only`, one `sourcePlan`, an optional `nativeAppendix`, and a derived `cacheKey`. Unknown,
missing, null, or mistyped fields reject. The checker compiles this repository-owned schema at
module initialization; callers cannot substitute a relaxed schema. The source-only plan omits
`nativeAppendix` completely.

The source plan binds the following inputs:

| Field | Bound identity | Owner or purpose |
| --- | --- | --- |
| `packageLockSha256` | #168 `lock` digest | exact verified package-lock authority |
| `rootPackage`, `packages` | exact #360 `(id, sourceSha256)` pairs and #168 aliases/edges | exact authenticated target/runtime graph |
| `sources` | graph role, exact package pair, portable path, exact byte size and SHA-256 | complete qualified source snapshot used by the driver |
| `compiler` | declared host-tool name, exact version, executable SHA-256 and protocol | compiler implementation identity |
| `profile` | exact #357 language-profile id and configuration SHA-256 | semantic/build profile, not a selector |
| `executionPolicy` | policy id, version and configuration SHA-256 | opaque accepted #362 decision input |
| `host` | host triple and explicit output-relevant environment | no ambient host state |
| `targets` | #168 target id, triple, ABI/runtime identity and #357 composition reference | every resolved output axis; v0 features stay empty |
| `hostTools` | name, version, executable SHA-256, host triple and admitted targets | executable tools only; never target libraries |
| `outputs` | target-qualified portable paths | requested outputs, not evidence of publication |

Every dependency must reference a listed package in the same graph role. Every source must reference
a listed package in its graph role. The root pair must be present in `target/runtime`. Starting at
that root, the complete graph must be acyclic and must reach every listed package. Cycle selection
is deterministic: choose the fewest edges, normalize each cycle to its least
`(graph role, package instance, alias)` edge, then compare the complete normalized edge sequence as
unsigned ASCII bytes. The first unreachable package is likewise selected in canonical package
order. After structural graph validation, the lock digest, root, ordered package pairs, aliases,
and selected edges must exactly equal the branded projection from the validated #168 envelope.
Source-only v0 rejects every `host/build` occurrence rather than inventing an unauthenticated root.
The compiler version and profile must match #168 compatibility, and each selected target must be
in its target set. The exact compiler tuple must match a declared host tool, and that compiler tool
must admit every selected target. Each host tool
must run on the declared host and may produce only for listed targets. Every output must name a
listed target. These are closed references: discovery from `PATH`, a current working directory,
an ambient registry, an inherited environment, or a neighboring cache entry is forbidden.

The source list is a complete material inventory, not a filesystem glob. Before compilation the
driver must open each source as a bounded regular non-link file beneath the accepted package root,
rehash the bytes, compare size and SHA-256, and reject missing, extra, replaced, or unstable input.
The fixture checker accepts an explicit in-memory material map only to test this rule; it proves no
filesystem-race property. For each role-scoped package occurrence, the checker projects its
path-ordered file rows through #168's `source-files` digest and requires exact equality with the
pair's `sourceSha256`.

The admitted language-profile vocabulary is exactly `i32-v1`, `control-flow-v1`, and
`data-ownership-v1`. Resolved package targets are exactly `javascript`, `webassembly`, and
`native-linux-x86_64`; public `native` maps to the last value and public `all` must already be
expanded. Compatible #357 composition rows are:

| Resolved target | Accepted rows |
| --- | --- |
| `javascript` | `U-JS`, `JS-BROWSER`, `JS-NODE` |
| `webassembly` | `U-WASM`, `WIT-BROWSER`, `WIT-COMMAND`, `WIT-SERVER` |
| `native-linux-x86_64` | `U-NATIVE`, `NATIVE-HOST` |

The composition reference binds the exact contract id, row, host-policy digest and approved-request
digest. A row from another target rejects before compilation. Its presence grants no authority;
#357 composition validation and host enforcement remain independently required.

## Canonical serialization and identity

Canonical plan bytes are strict UTF-8 JSON without BOM, recursively sorted object keys, no
insignificant whitespace, JSON's shortest ordinary spelling for admitted scalar values, and one
terminal LF. Consumers reject noncanonical bytes rather than normalize them. The maximum plan is
262,144 bytes. A source path is at most 96 lowercase portable ASCII bytes, matching #168 exactly;
byte 97 rejects before source hashing. The source-only projection reuses #168's fixture bounds: 16 packages, eight
dependencies per package, 16 files per package (256 sources total), three targets, eight host tools
and environment entries, 16 outputs, and 1,024 bytes per source. The optional native collections
retain their separately provisional bounds.

Arrays have these deterministic rules:

| Collection | Required ordering |
| --- | --- |
| packages | `target/runtime`, then `package.id` and `package.sourceSha256` ascending |
| package dependencies | alias ascending |
| sources | graph role, `package.id`, `package.sourceSha256`, then path ascending |
| targets, target features | id/value ascending |
| host tools, environment entries | name ascending |
| tool target sets | target id ascending |
| outputs | target id, then path ascending |
| native acquired collections and compile steps | id ascending |
| linker inputs | declared execution order; repetitions allowed |

All comparisons use unsigned ASCII bytes. Set-like arrays are unique. Source paths use lowercase
portable relative syntax and reject empty, dot, parent, device-name, repeated-separator, absolute,
backslash, and trailing-separator forms. Paths are logical plan identities; the driver maps them to
private capabilities and never interpolates them into shell text.

The cache projection removes only the root `cacheKey`; it retains format, version, status,
`sourcePlan`, and the native appendix when present. `cacheKey` is #361's domain-separated digest of
the complete canonical plan closure. It is neither a #168 manifest digest nor a per-package
`sourceSha256`. The four digest roles are deliberately separate:

| Digest | Meaning and owner |
| --- | --- |
| `packageLockSha256` | #168 canonical lock digest |
| `package.id` | #168 canonical package-manifest digest |
| `package.sourceSha256` | #168 canonical `source-files` digest for that package |
| root `cacheKey` | #361 canonical plan-closure/cache identity |

The plan cache key is:

```text
SHA256(
  UTF8("ZRYNA-RESOLVED-BUILD-PLAN-V0\0plan\0") ||
  canonical_bytes(cache_projection)
)
```

Thus the #168 lock/manifest/source digests, graph roles, source bytes, dependency pairs/edges,
compiler/runtime versions and hashes, #357 profile/composition inputs, execution-policy
configuration, host/environment inputs, targets, ABI identities, tools, outputs, and every native
input all invalidate the cache. Host
absolute paths, mtimes, acquisition order, process ids, wall-clock time, random values, and
undeclared environment do not enter the identity and cannot be consulted.

A target entry uses a second domain and the exact target id:

```text
SHA256(
  UTF8("ZRYNA-RESOLVED-BUILD-PLAN-V0\0target\0") ||
  canonical_bytes({"planKey": <plan-cache-key>, "target": <target-id>})
)
```

A cache lookup by a missing target key is an ordinary cache miss and may lead the driver to build.
A present entry is never trusted by key alone: its closed metadata record has at most 16 unique
outputs, and its target, complete output paths, byte sizes and SHA-256 values must be revalidated
before use. Malformed metadata, a wrong target, stale output, incomplete inventory,
or incompatible plan key is corrupt/incompatible input and rejects; it is not silently treated as a
hit, repaired, relabeled, or published. Cache storage is private intermediate state, not a release
or provenance record. #168 remains the authority for recording any resulting release material.

## Optional native-input appendix

The legacy appendix makes host and target roles unambiguous while leaving ABI inputs provisional;
the prospective variant below fixes one tuple without granting native admission:

| Section | Responsibility | Contents |
| --- | --- | --- |
| source plan `hostTools` | host execution inventory | compiler, archiver, linker, generators; each runs on the host and declares its target set |
| `acquisition.targetLibraries` | logical target library selection | exact name/version, target, linkage and artifact reference |
| `acquisition.sysroots` | target system headers/libraries root | target id and content digest; never a host filesystem default |
| `acquisition.staticArtifacts` | exact link-time archives | target id, byte size and digest |
| `acquisition.sharedArtifacts` | exact link/load artifacts | target id, byte size and digest |
| `acquisition.runtimeDependencies` | artifacts required after link | target id and an exact shared-artifact reference |
| `compilation.steps` | driver-requested compilation | declared host tool, target, typed versioned invocation adapter, inputs and object output identity |
| `abi` | native boundary descriptions | target triple plus versioned ABI, calling-convention, carrier, ownership and runtime identities from relevant #364 decisions |
| `linking` | driver-owned final link | declared host linker, target, ordered typed linker inputs, typed versioned invocation adapter and exact declared output path |

The appendix records acquisition identities; their presence never authorizes retrieval or execution.
Acquisition may return verified bytes and metadata only under #362 policy and evidence, and it cannot
run a recipe. Compilation consumes acquired/source inputs and produces unpublished object identities.
Linking consumes explicitly typed object, static, shared, and sysroot references in recorded order.
Runtime dependencies are not linker inputs by implication and linker inputs are not deployed by
implication. The driver checks every reference, target, digest and tool before starting a process and
alone owns compilation, linking, cache materialization, and publication.

Compilation and linking use closed, versioned invocation-adapter records rather than raw argument
arrays. Source paths, response-file references, and ambient `-L`/`-l` selections therefore cannot
be smuggled into process arguments. The linker output must exactly match a target-qualified output
already declared by the source plan.

All acquired records and compilation/link sections must name the appendix target. A target library
must reference an artifact of its declared static/shared kind. Every runtime dependency must name a
declared shared artifact. Every compilation or linking executable must be an exact `hostTools`
entry that runs on the plan host and admits the appendix target. Missing libraries, sysroots,
artifacts, runtime dependencies, objects, or tools reject before process execution. No `-lfoo`,
`PATH` search, system-default sysroot, loader search path, shell expansion, or host library fallback
may satisfy an absent declaration.

The appendix's target triple and runtime tuple must exactly match its selected source-plan target.
The legacy calling-convention, carrier and ownership identities are opaque versioned inputs. The
prospective variant checks only the specified #364 literals; it cannot derive or widen those ABI
rules, authenticate runtime bytes, or replace independently sealed language/layout authorities.
Acquisition authorization remains a #362 policy decision. Compilation, linking and publication
remain driver operations and cannot be authorized by the presence of these identities.

### Proposed D2 native C v0 alignment

This specification-only successor is **prospective**, pending independent exact-revision review
and normal integration with a factual [D2 decision record](../abi/NATIVE_C_INTEROP_V0_REVIEW.md#prospective-d2-native-appendix-contract-decision).
It defines the explicit `specified-native-c-interop-v0` state alongside the retained proposed state;
recognizing either grammar records no reviewer identity, approval, integration or execution.
Passing tests, integrating #364, closing a parent issue or supplying an approval Boolean cannot
replace that decision. Implemented/conformance/public-support and capability claims still reject.
Legacy and proposed records retain their original status, canonical bytes, receipts and cache keys;
no validator, normalization or cache operation relabels or upgrades them.

For both `proposed-specified-native-c-interop-v0` and `specified-native-c-interop-v0`, the same closed schema requires:

| Field | Exact value |
| --- | --- |
| `target` | `native-linux-x86_64` |
| `abi.identity` | `zryna-native-c-interop-v0` |
| `abi.version` | string `"0"`, retaining #361's existing version-string wire type |
| `abi.targetTriple` | `x86_64-unknown-linux-gnu` |
| `abi.callingConvention` | `sysv-amd64-c-v0` |
| `abi.carrierModel` | `native-c-interop-v0-carriers` |
| `abi.ownershipModel` | `native-c-interop-v0-resources` |
| `abi.decisionIssue` | integer `364` |

The historical illustrative `system-v-amd64-c` spelling is not an alias for the specified
convention and rejects in both D2 states. Legacy `provisional-pending-364` records remain
structurally valid drafts with their existing opaque tuple grammar; their presence never becomes
acceptance. Missing/unknown fields, other versions/targets/carriers/owners, and raw arguments still
reject. The source target's language ABI remains a distinct input; #364 does not replace scalar
ABI v1 or expose compiler-private ownership-runtime layouts.

`abi.runtime` must exactly match the selected source target's runtime name/version/SHA-256 tuple.
This equality binds a claimed input, not independently authenticated artifact bytes. The synthetic
`zryna-native-runtime`/`1` fixture with SHA-256 `"3"` repeated 64 times is illustrative only; no
runtime digest is fixed as artifact authority by this schema. Changing a matching tuple changes
the plan/cache identity but still proves no runtime availability, ABI execution or isolation.
The actual accepted language-runtime contract/version and retained target bytes require separate
authentic evidence under their owning runtime/driver/#168 boundaries. A C allocator/library
identity cannot satisfy that obligation, and a matching checksum cannot issue it.

The fixture receipt's existing `native` flag describes appendix presence only. A proposed record
returns `nativeAdmission: "denied-proposed"`; an explicit specification-only record returns
`nativeAdmission: "denied-specified"`. Both are descriptive, frozen receipts and issue no capability
for execution, acquisition, cache reuse, linking, publication or grants. The native-recipe preflight
continues its existing fail-closed denial even with exact tuple/material/digest comparisons and
caller-supplied acceptance/FFI/runtime flags; its acceptance prerequisite concerns execution policy,
not this specification grammar. Source-only bytes, receipt shape, cache domains, miss/hit validation,
package graph and publication observations are unchanged. Each explicit state remains in the
complete-plan cache projection, so state changes require a distinct key; no key or receipt upgrades
a historical draft or grants native cache reuse.

## Rejection phases

The checked fixture API returns one stable category and no partial accepted plan:

| Category | Rejected condition |
| --- | --- |
| `P361-BUDGET` | wire/canonical-plan byte bound or structural collection bound exceeded |
| `P361-WIRE` | invalid UTF-8/JSON or noncanonical bytes |
| `P361-SCHEMA` | missing, unknown, mistyped, or malformed field |
| `P361-ORDER` | noncanonical or duplicate set-like collection |
| `P361-IDENTITY` | absent/unvalidated #168 authority; changed lock/root/pair/alias/edge; missing reference; cycle; orphan; unsupported graph role |
| `P361-SOURCE` | missing package for a source, missing bytes, or size/hash drift |
| `P361-TOOLCHAIN` | #168 compatibility mismatch, compiler target gap, wrong-host tool, or undeclared compile/link tool |
| `P361-TARGET` | absent, incompatible, non-native, or cross-target input |
| `P361-NATIVE` | unresolved or kind-confused library/sysroot/artifact/object reference |
| `P361-CACHE` | forged plan key, incompatible entry, incomplete inventory, or stale bytes |
| `P361-PUBLICATION` | partial visibility or replacement at the existing commit boundary |

The phase order is wire-byte budget, UTF-8/JSON/canonical-wire checks, canonical-plan and collection
budgets, schema, ordering/reference/graph/#168 identity, source/tool/target/native semantics, then
cache verification. Collection first-extra cases therefore report `P361-BUDGET`, not a schema
error; malformed values within an admitted collection report `P361-SCHEMA`. These fixture
categories are not public compiler diagnostics.
A later driver integration must map failures through its separately reviewed diagnostic surface.

## Representative outcomes

### Repeat build and cache miss

`tests/resolved-build-plan-v0/source-only.json` is a fixed two-package source-only example committed
as canonical single-line JSON with a terminal LF and validated through the byte API. Its cache key
is checked against a fixed expected value and an independent canonicalization/domain-hash oracle.
Parsing the same canonical bytes twice yields the same plan cache key. Rehashing the same two source files
twice succeeds. A target-cache entry with the derived target key and complete rehashed output is a
hit. With no entry, the result is `miss`; no error, cache mutation, or publication has occurred.

Changing any source hash, package pair or graph role, dependency edge, compiler, runtime, profile,
execution policy, relevant environment, target, or host-tool digest changes the plan key. Reusing
the prior entry then fails `P361-CACHE`. This is a cache miss only when the new key is absent; a
present but mismatched entry is rejected as incompatible data.

### Wrong target, missing native input, and undeclared tool

The native test draft adds `native-linux-x86_64` with #357 row `NATIVE-HOST` to a Linux host.
Relabeling its static archive as
`javascript` fails `P361-TARGET`; the driver never calls a linker. Referencing `missing-a` from a
target library fails `P361-NATIVE`. Naming an ambient `cc` that is absent from `hostTools` fails
`P361-TOOLCHAIN`. A host tool and target library can share a human name but never a role or lookup
namespace.

### Stale input

If `src/main.zry` bytes no longer match the recorded size and SHA-256, source verification fails
`P361-SOURCE` before compilation and before cache lookup can authorize output. If cached artifact
bytes no longer match their entry, validation fails `P361-CACHE`; the corrupt entry is not returned
and no output bundle is published.

### Interrupted and create-only publication

The driver prepares a complete bundle in a private sibling, synchronizes and independently checks
its output manifest, then performs the existing single create-only commit. If interrupted before
that commit, the final destination remains absent and staging is not observable as output. If the
destination already exists, the prior destination remains byte-for-byte intact and no commit is
attempted. Visibility without a completed commit and complete manifest fails `P361-PUBLICATION`.
This plan adds no alternate publication path and does not change the existing manifest-v1/v2/v3
transactions.

## Implementation slices and measurable gates

Contract state is tracked as `specified → implemented → conformance-passed → publicly supported`.
Passing this fixture suite establishes only the first state.

### Source-only implementation slice

Prerequisites are the merged #168 serialization/source identities, merged #357 composition
vocabulary, and accepted #360 exact package pair and graph-role contract. #362 policy enforcement is
required before running acquired code, but pure local
source-plan validation and cache-key calculation can be implemented against an opaque accepted
policy identity. Native artifacts, registry service, C ABI, and M7 are not prerequisites.

The later slice is implementation-ready only when the alignment checklist below is closed. Its
clean-build gate must, in isolated roots with empty caches, resolve one fixed source graph, rehash
every source, produce byte-identical canonical plans/cache keys twice, build through the existing
driver, compare every output byte/hash, and commit only a complete create-only bundle. Negative
gates must independently exercise missing/stale source, changed dependency/profile/tool/runtime/
policy/environment, cache absence, corrupt/incompatible cache entries, undeclared tools, output
collision, and interrupted staging. Linux and Windows contract checks must pass; target execution
claims require their existing platform-specific suites.

### Native implementation slice

Prerequisites are the source-only slice, #362 acquisition/execution policy, and only the relevant
accepted #364 ABI decisions listed above. Its clean-build gate additionally materializes a pinned
sysroot and exact static/shared inputs in two isolated roots, compiles and links through declared
host tools, verifies target/object/link audits, reproduces the final native bytes, and records
runtime dependencies separately. Negative gates cover wrong architecture/ABI, missing library or
sysroot, substituted archive/shared object, undeclared compiler/linker, wrong-host tool, shuffled
ordered linker inputs, loader dependency mismatch, process failure, stale cache, and interrupted
publication. No native registry, large library pilot, binding generator, or new public selector is
part of that slice.

## Alignment checklist

- Preserve #360's exact `(id, sourceSha256)` pair in every package reference. Source-only v0 admits
  only the authenticated `target/runtime` root and closure; reject `host/build` until a later
  version defines its root and edge authority. Never reinterpret #168 lock edges as build edges.
- Verify `package.id`, `package.sourceSha256`, and `packageLockSha256` against #168's accepted
  records, while keeping the #361 plan-closure `cacheKey` in its separate digest domain; do not copy
  release/provenance fields.
- Preserve #357's exact language-profile, target-axis and composition-row vocabulary; a later
  expansion requires its owning reviewed version rather than a local alias or feature field.
- Confirm that the compiler, runtime, profile, tool, environment and execution-policy projections
  contain every output-relevant accepted field and no authorization, isolation, acquisition, or
  trust decision owned by #362.
- Confirm whether #168 records this plan key as a provenance material/reference; #168 remains the
  record owner and must domain-separate any such digest in its own versioned extension.
- Independently review this exact successor's #364 tuple mapping, specification-only receipt and
  runtime-artifact evidence separation before completing the prospective D2 contract decision;
  retain legacy/proposed/source-only validity without relabeling any state. Only real review and
  integration outcomes may replace pending decision fields; runtime artifact authority stays separate.
- Review any renamed field or changed projection as a cache-format change. Do not silently accept
  both spellings, infer defaults, or rewrite historical records.

## Verification

The fixture checker is [`scripts/build-plan/validate.mjs`](../../scripts/build-plan/validate.mjs).
The focused test covers canonical replay, cache invalidation dimensions, exact/first-extra source
bounds, missing/stale sources, cache miss/hit/incompatibility, wrong-target native input, missing
libraries, undeclared tools, and interrupted/create-only publication observations:

```bash
pnpm build-plan:contract
pnpm docs:check
pnpm structure:check
```

Repository-required preflight and M0 remain mandatory before merge. Hosted checks, dependency
alignment and maintainer review govern acceptance. This repository-only document is not implicitly
added to the public website bundle and makes no current support or release claim.
