# Cross-target profile composition v1

Contract identity: `zryna.cross-target-profiles.v1`. State: **specified-only**.
This is the compiler-owned composition decision for [#357](https://github.com/zryna/zryna/issues/357)
in [M4](https://github.com/zryna/zryna/milestone/5). It specifies future validation, not an
implemented package resolver, host interface, public selector, or execution capability.

## Authorities and dependencies

The [WIT capability contract](../wit/CAPABILITY_PROFILES_V1.md), owned by
[#167](https://github.com/zryna/zryna/issues/167), is the sole authority for pinned WIT worlds,
WASI versions, capability interfaces, per-instance limits, and granted/denied fixtures. Acceptance
of this document's WIT mapping requires that contract. Composition neither substitutes a world
nor increases its limits. The compiler owns admissibility; package resolution carries exact
declared requirements; the driver composes verified inputs; host adapters enforce permissions.
Backends consume verified IR and cannot widen source or host authority.

| Consumer / milestone | Exact prerequisite edge | Boundary |
| --- | --- | --- |
| #357 / M4 | #167 -> #357 WIT mapping | Drafting other rows needs no M3 completion |
| [#358](https://github.com/zryna/zryna/issues/358) / M4 | #357 -> #358 final profile mapping | Pure-core API design can proceed independently |
| [#359](https://github.com/zryna/zryna/issues/359) / M4 | #167 + #357 -> #359 interface acceptance | #358 -> only its corresponding library-facing conversions |
| #358 / M4 resource APIs | #359 + [#364](https://github.com/zryna/zryna/issues/364) -> affected resource decisions only | Reconcile sections, not circular whole-issue prerequisites |
| #360 / M5 compatibility | #357 -> #360 cross-profile acceptance; #168 -> its schema alignment | #360 does not block this design; #364 remains in M7 |
| [#168](https://github.com/zryna/zryna/issues/168), [#360](https://github.com/zryna/zryna/issues/360) / [M5](../../docs/ROADMAP.md#m5--packages-and-reproducible-releases) | Accepted package formats and instance identity -> package-backed composition implementation | No manifest or lockfile format is invented here |
| [#361](https://github.com/zryna/zryna/issues/361), [#362](https://github.com/zryna/zryna/issues/362) / M5 | Accepted build-plan and execution-trust decisions -> build-tool integration only | Build permissions never become target permissions |

[#356](https://github.com/zryna/zryna/issues/356) is a planning index, not a delivery gate. Existing
M3 [#89](https://github.com/zryna/zryna/issues/89) -> [#90](https://github.com/zryna/zryna/issues/90)
closure and historical digest-pinned inventories remain unchanged. #357 does not depend on
completion of #358, #359, M5, native FFI, or native frontend replacement for design acceptance.

## Separate selection axes

An evaluation binds one exact language profile, a nonempty set of output targets, one deployment
host policy per output, a resolved source dependency graph, and explicit host grants. These are
distinct inputs. A compiler process running on Windows or Node does not grant Windows or Node
APIs to its output. A source-file import is not a package instance identity or a host capability.

Current public language selection remains omission of `--profile` for `I32V1`, exact
`--profile control-flow-v1` for `ControlFlowV1`, or exact `--profile data-ownership-v1` for
`DataOwnershipV1`. Public target spelling remains `javascript`, `webassembly`, `native`, and `all`.
`all` requests the three existing backends; it is not a host policy and does not include WASI.
The [CLI](../../docs/CLI.md) and [public M3 contract](../../docs/M3_PUBLIC_PROFILE.md) retain their
implemented boundaries, including current source/module and native-host limits.

The row IDs below are document references, never CLI or package selectors. "Universal source"
means source admitted under the same selected language contract for every requested target;
it is not a fourth language selector. Browser/Node are deployment distinctions, not new Zryna
language profiles. WIT `browser`, `command`, and `server` are #167 registry identities only;
the roadmap's `wasm-web` name does not select a new CLI mode or claim component/browser execution.

## Decision table v1

`D` is a denied capability ceiling. `E` is eligible only after an exact interface is specified,
explicitly required, host-granted, and independently enforced. Neither value grants authority.
Every omitted grant is denied. Columns follow #167's canonical capability order. This table
covers filesystem, network, clock, randomness, and environment access only; additional host
effects (including standard I/O, process launch, DOM access, and dynamic loading) are unsupported.
Internal allocation under the selected language/runtime contract is not a host I/O grant.

| Row | Output target | Language admission | Deployment boundary | clock | environment | filesystem | network | randomness | Enforcement owner | Availability |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| U-JS | javascript | selected universal subset | current pinned Node run harness | D | D | D | D | D | compiler verifier + driver | existing pure source only |
| U-WASM | webassembly | selected universal subset | audited core module; current pinned Node harness | D | D | D | D | D | compiler verifier + binary audit + driver | existing pure source only |
| U-NATIVE | native | selected universal subset | audited Linux x86-64 object; supported Linux link/run | D | D | D | D | D | compiler verifier + object audit + driver | existing pure source only |
| JS-BROWSER | javascript | same selected language; explicit host imports deferred | browser ESM adapter | E | D | D | E | E | compiler profile validation + #359 adapter | specified-only |
| JS-NODE | javascript | same selected language; explicit host imports deferred | Node ESM adapter | E | E | E | E | E | compiler profile validation + #359 adapter | specified-only |
| WIT-BROWSER | webassembly | selected language plus separately verified interface | #167 browser world | D | D | D | D | D | compiler profile validation + #167 world audit + host adapter | specified-only |
| WIT-COMMAND | webassembly | selected language plus separately verified interface | #167 command world | E | E | E | E | E | compiler profile validation + #167 world audit + host adapter | specified-only |
| WIT-SERVER | webassembly | selected language plus separately verified interface | #167 server world | E | D | D | E | E | compiler profile validation + #167 world audit + host adapter | specified-only |
| NATIVE-HOST | native | native extension requires separate language/FFI gate | explicit native host interface | E | E | E | E | E | compiler metadata validation + native adapter + external isolation owner | specified-only |
| UNSUPPORTED | any other target or unmatched host/interface pair | none | no fallback | D | D | D | D | D | driver selection before compilation | unsupported |

The pure rows describe current capability restrictions, not implemented package composition.
Pure source may be a future dependency of any compatible row without gaining that row's eligible
effects. Node availability does not admit arbitrary npm modules, `node:fs`, `process.env`, global
`fetch`, clocks, or randomness into source. Browser availability similarly supplies no ambient
globals. JS eligibility is a ceiling for #359 to refine: exact operations, resource limits,
endpoint/clock/entropy policy and enforcement must be accepted before any nonempty request.
There is no implicit emulation of a Node filesystem API in a browser.

WIT rows reuse `zryna:capability-profiles@0.1.0` and WASI `0.2.12` from #167. Equal capability
names do not imply equal interfaces: command networking and server HTTP imports differ, and a
browser JS network permission does not authorize imports into the empty WIT browser world.
No automatic Node-to-WASI adapter, world subtyping, or interface-version range is admitted.

## Transitive composition

This is an abstract compiler input contract, not manifest syntax. The resolver must supply an
authenticated, finite, canonical graph with stable instance IDs, exact source identities,
dependency edges, per-instance language compatibility, supported target/interface pairs,
capability restrictions, and direct capability requirements. A missing declaration is unknown, not proof of purity. An
explicit empty requirement set claims purity and must be checked against source and interfaces.
Unknown fields/versions/capabilities, duplicate identities/edges, dangling edges, cycles, and
unverifiable native or external inputs reject before composition. Package identity and any later
cycle policy belong to #360; v1 composition does not infer identity from import spelling.

For each selected output and deployment boundary, take the complete reachable dependency closure
`C(root)`, including the root. All resolved runtime dependencies count, even if a call appears
unreachable or a bundler could remove it. A future accepted resolver may exclude an inactive
dependency before producing this authenticated graph; composition cannot invent feature or
target predicates to discard a forbidden dependency. Build tools have a separate graph and
execution policy under #361/#362. A package used in both graphs is checked independently in both.

1. Validate graph shape, identities and structural budgets before traversal. Validate every
   instance's target/interface compatibility and exact language-profile compatibility next.
   Version numbers do not establish language subtyping: `ControlFlowV1` and `DataOwnershipV1`
   are separate verifier contracts. A source package can claim both only if separately verified
   under each; a precompiled authority cannot be relabeled. Unknown compatibility rejects.
2. Let `R(p)` be the direct required capability/interface set of instance `p`. Compute
   `R*(p) = R(p) union union(R*(d))` over its dependencies. Sets deduplicate by exact identity;
   a diamond does not erase a requirement or create a second instance. Claimed summaries must
   equal recomputed closure requirements. Validate every instance's declared restrictions
   against its own `R*(p)`: a pure library cannot launder an effectful child.
3. Require every member of `R*(root)` to fit the selected table row and its exact interface
   authority. A denial rejects before IR construction even if the host offers that capability.
   Unknown operations, adapters, or versions reject; a shared capability label is insufficient.
4. The root must explicitly approve the complete transitive request. An omitted leaf requirement
   fails as forbidden-capability during profile validation, before IR. A dependency declaration
   never approves its own grant. Bind the driver result to graph/source identities, language
   profile, all selected targets, exact host/interface policy versions, and the approved request.
   Changing any bound input invalidates it. Revalidate a cached result; do not trust a stored
   transitive summary or reuse a result across hosts.
5. Before any target execution/instantiation, the host must satisfy every required operation
   within the approved request and current grants. No partial grant, ambient fallback, or silent
   target removal is permitted. Effective authority is the intersection of the verified request,
   profile ceiling, and explicit host grant; extras offered by the host convey no authority.
   Recheck at each invocation/instantiation and mediate each host operation. Revocation or an
   exhausted quota fails at the host boundary without performing the denied effect.

For a future `CommandH1V1` execution using the pinned
`wasi:cli/environment@0.2.12` import, that last host-boundary rule has one concrete
transport: a revoked environment grant traps before reading or copying the value. The
`wasi:cli/run@0.2.12` call then has no return value; the host records a distinct
permission-denied execution outcome with the denied interface and operation. It must
not fabricate a source `Missing` or a returned WIT `err`. This specifies the #357 side
of the H1-only #358 reconciliation; it grants no new operation, changes no other
target's error transport, and does not activate the proposed command profile.

Language/source verification must independently reject undeclared host operations before sealing
IR; declared metadata alone is insufficient. For `all`, all three selections must pass before
any backend dispatch or transaction commit. Existing per-profile transactions stay authoritative;
unsupported native execution on the compiler host still fails through the existing driver gate.

### Resource composition

Permission union is not quota multiplication. Within one component/execution instance all linked
dependencies share its host ceilings. Count a live descriptor/timer/operation once, including
children; sum simultaneously retained resources across dependencies. Deduplicate normalized
endpoint, preopen-authority and environment-key entries; conflicting environment values reject.
For statically declared independent reservations, sum disjoint reservations with checked
arithmetic and reject an unprovable bound. A diamond sharing one resolved instance contributes
its reservation once; two distinct instances contribute twice. A per-call limit is checked on
each call; cumulative limits count all calls across dependencies, including repeated invocations.

#167 remains the authority for each metric and numeric maximum. Host policy may narrow it; neither
transitive metadata nor multiple importing packages may enlarge it. Independent component
instances retain #167's per-instance limits; this document promises no process-wide quota across
instances. Such an isolation claim requires a separate host policy. JS/native numeric host limits
remain deferred to their accepted adapters; unknown bounds cannot admit an effectful request.

## Native restrictions and isolation

The NATIVE-HOST row restricts compiler-checked declarations and accepted link inputs. A manifest
does not sandbox arbitrary native code: in-process foreign code can issue syscalls or access the
process environment outside an adapter. Reject native inputs whose effects cannot be verified
when the request requires denied capabilities or execution isolation. Trusted foreign code may
enter only through separately accepted #364 and #361/#362 boundaries, with its trust assumptions
explicit; it cannot be advertised as capability-isolated based on declarations.

Running untrusted native code with a capability-denial guarantee requires an external process/OS
isolation policy with independent escape and denied-effect tests. No such sandbox is specified or
activated here. The existing audited compiler-produced native route retains its narrower proof;
neither its bounded process controls nor generated manifest establishes arbitrary-code isolation.

## Diagnostics and representative examples

The following labels are required future diagnostic categories, not allocated public codes.
Implementation must assign stable codes in the compiler diagnostic authority before conformance,
preserving #167's `ZRYNA-C4000` through `ZRYNA-C4004` meanings for its own validator.

| Category | Expected phase | Required explanation |
| --- | --- | --- |
| invalid-composition | graph/input validation | offending field, identity, edge, version or bound |
| incompatible-profile | compiler profile validation before IR | selected profile and incompatible instance's exact declared profiles |
| unsupported-target | driver/profile selection before IR | requested output, deployment host and unsupported interface or native host |
| forbidden-capability | compiler profile validation before IR | denied capability/operation, rejecting policy and full dependency witness |
| undeclared-capability | source/interface validation before sealed IR | operation span and instance whose declaration omitted it |
| missing-host-grant | host pre-execution validation | required operation, explicit request and absent/narrower grant |
| resource-limit | input/reservation validation or mediated host operation | metric, limit, observed first-extra value and consuming instance |

Composition diagnostics must retain root and leaf instance IDs, target/host policy identity and an exact
root-to-leaf edge witness. Use the declaration/import source span when authenticated; otherwise
use a workspace location or global diagnostic with no fabricated span. Do not print environment
values, credentials, or host paths outside the authenticated diagnostic input. Shape/selection
errors use only already-authenticated identities and need no fabricated dependency witness.
Canonical phase order is input validation, target/interface selection, profile validation,
source/interface verification, then host validation/operations; within a phase use target order
`javascript`, `webassembly`, `native`, then canonical
instance/edge IDs and capability order. For multiple paths choose the shortest root-to-leaf
witness, breaking ties by bytewise instance-ID sequence. Repeated validation must produce the
same ordered diagnostics; #169's transport remains authoritative.

Examples below are future fixtures. `A -> B -> C` uses exact distinct instances, one selected
language profile and no unmentioned requirements; `pure` means an explicit verified empty set.
An "accept" is a specified decision, not an executed compiler/host result.

| Case | Input / change | Fixed expected outcome |
| --- | --- | --- |
| pure-chain | A -> B -> C, all pure and ControlFlowV1-compatible; all three pure rows | accept; empty transitive set; later source/target oracle returns `i32:42` on all three |
| indirect-forbidden | A -> B -> C; C requires filesystem; JS-BROWSER | forbidden-capability before IR; filesystem witness A -> B -> C, even if A never calls B |
| pure-parent | A -> B -> C; B declares a pure restriction, C requires clock; JS-NODE | forbidden-capability before IR at B; a host clock grant cannot repair B's restriction |
| incompatible-language | A selects ControlFlowV1; B declares only DataOwnershipV1; A -> B | incompatible-profile before IR; no numeric-version promotion |
| unsupported-native | A -> B; B declares a native-only interface; targets javascript + native | unsupported-target before any backend; no native-only partial bundle |
| unsupported-host | pure A; native run requested on a host outside the supported Linux boundary | existing driver unsupported-host rejection; no execution |
| host-is-not-target | compiler runs on Node; A -> B requires environment; WIT-SERVER | forbidden-capability before IR; Node environment access grants nothing |
| explicit-grant | A -> B; B requires command environment; root approves it; #167 command world, exact-limit request and host grant | accept policy validation using #167's command-granted fixture; host execution remains deferred |
| omitted-grant | same approved command request, host grant empty | missing-host-grant before execution; no partial instantiation |
| omitted-request | A -> B; B requires command environment; root approves an empty request | forbidden-capability before IR; transitive declaration cannot approve itself |
| world-mismatch | C requires command socket interfaces; WIT-SERVER | unsupported-target/interface before IR despite network eligibility |
| forged-pure | C claims pure but source uses a host clock operation | undeclared-capability before sealed IR, at C's operation |
| diamond | A -> B, A -> C, B -> D, C -> D; D requires one clock reservation | one requirement/reservation for D; canonical witness A -> B -> D |
| replay | accepted JS-NODE request reused for JS-BROWSER or after dependency source/edge change | invalid-composition; stale authority rejected before backend/host use |
| untrusted-native | native object claims pure but its effects are unverifiable; request requires denial | invalid-composition at native input validation; declarations supply no isolation proof |

### Deterministic resource fixtures for later implementation

Composition v1 proposes a bounded graph input: at most 256 instances including root, 4,096 edges,
32 edges in a root-to-leaf path, 65,536 total UTF-8 identity bytes, and 255 ordinary diagnostics
plus one reserved terminal limit diagnostic (256 total). Validate graph bounds before materializing closure summaries; traversal
must be iterative. These are future composition limits, not changes to current module/IR limits.

| Metric | Exact-limit fixture | First-extra fixture / outcome |
| --- | --- | --- |
| instances | pure star with root + 255 leaves, accept | add one distinct leaf: 257, invalid-composition |
| edges | acyclic layered graph with 4,096 distinct edges within all other bounds, accept | add one valid edge: 4,097, invalid-composition |
| depth | pure chain of 32 edges, accept | append one edge: 33, invalid-composition |
| identity bytes | otherwise valid graph totaling 65,536 UTF-8 bytes, accept | add one valid ASCII byte: 65,537, invalid-composition |
| diagnostics | 255 independent errors emit all 255 without truncation | the 256th candidate emits only the terminal limit diagnostic in slot 256, then halt |
| command environment entries | two dependencies reserve 64 distinct keys each, total 128, accept under #167 | add one distinct key: 129, resource-limit before instantiation |
| command environment bytes | disjoint key/value UTF-8 bytes total 65,536, accept under #167 | add one UTF-8 byte: 65,537, resource-limit before instantiation |
| every other #167 metric | partition its exact profile maximum across dependencies in one instance | one extra metric unit anywhere in the closure: resource-limit; no partial grant |

Independently construct malformed graphs, forged summaries, cycle/dangling/duplicate mutations,
unknown operation/version cases and requests that omit the transitive leaf. Replay valid and
invalid fixtures with permuted discovery order; canonical outputs must match, without using the
producer's transitive summary as the oracle. Include diamond versus distinct-instance quotas,
repeated-call cumulative exhaustion, multibyte byte accounting, host revocation and recovery after
rejection. Reuse #167's single-instance granted/denied tests; these fixtures add composition only.

## Migration and delivery gates

| Decision | Migration | Deferred implementation / conformance gate |
| --- | --- | --- |
| Existing CLI profiles, targets and wasm-web terminology | preserve spelling, omissions and rejection behavior | current M0-M3 regression and public-profile authorities |
| Browser/Node/native host rows | no new selector; unsupported effectful requests remain unsupported | accepted exact #359 or #364 interface, language/ABI prerequisites, enforced host limits |
| WIT world mapping | exact #167 identities only; no core-module-to-component reinterpretation | #167 acceptance, component emission, independent world audit and host enforcement |
| Transitive union, pure restrictions and explicit approval | new versioned composition input only; missing legacy metadata stays unknown | #168/#360 package formats/identity, compiler source/summary verification and malformed-graph tests |
| Exact language and interface compatibility | no implicit profile widening or precompiled relabeling | separately checked source compatibility or accepted adapter boundary |
| Resource and native-isolation policy | no quota increase or new sandbox claim | bounded graph/host tests above; #361/#362/#364 only for their affected inputs |
| Future selector or policy expansion | separate reviewed version and migration decision required | all relevant implementation and conformance gates before explicit public activation |

A bounded implementation should first validate compiler inputs and pure dependency composition,
then integrate one accepted host interface with independent denial/resource enforcement. Public
package resolution waits for its own M5 authorities; a fixed internal graph need not wait for a
registry service. No implementation slice is authorized merely by this decision document.

Track four separate states: **specified** (reviewed decisions and prerequisites), **prototype**
(private implementation with no support claim), **conformance-passed** (independent positive,
negative, deterministic, exact-limit/first-extra and applicable target/host proofs), and
**publicly-supported** (explicit activation and authenticated public documentation). This change
supplies only specification and documentation-consistency tests. Future conformance must assign
stable diagnostic codes and execute the fixed cases above on every claimed host/target; public
activation requires reviewed selectors, migration and support documentation. Current contribution
checks, Linux/Windows M0 proof, and applicable cross-target gates remain mandatory.

## Private fixed-graph implementation #380

The decision table and public availability above remain specified-only. The private driver
`profile_composition` module implements the **prototype** verification boundary for a closed,
fixed input graph. `verify` receives independently supplied claims and recomputes each node's
reachable requirements and reservations using iterative traversal. It validates every node's
restrictions, selected language and exact target/interface pair; permission eligibility never
becomes a grant. Only the verifier constructs the opaque result.

Each graph instance is accompanied by an opaque verified program and its exact `SourceMap`.
Composition checks that pairing at the sealed program boundary and derives language compatibility
from the verified program type; the graph cannot supply a source-identity string or language
declaration. A WIT selection additionally requires the existing authenticated `WitWorldAudit`,
and every approved interface must occur in that exact resolved world. The digest-pinned #167
registry remains the authority for capability classification and numeric quotas. There is no
resolver, dispatch call, host grant, execution, public API or selector in this boundary.
Package-backed integration still requires #360; invocation-time grants remain a separate gate.

The policy adapter reads the existing #167 JSON projection at compile time and authenticates its
exact SHA-256 bytes before use. Numeric limits and capability classification come from that
projection; WIT world/interface admission additionally requires the opaque authenticated audit.
JS/native effectful interfaces remain unsupported. Host policy inputs can narrow WIT ceilings;
they cannot enlarge them. Language admission comes only from the corresponding sealed program.

Input limits are 256 instances, 4,096 edges, longest root-to-leaf depth 32 and 65,536 UTF-8
identity bytes. Identity accounting includes every occurrence of instance IDs, edge/root
references, contract/policy/world strings and required/approved interface strings, excluding JSON
framing. Reservation environment bytes and authority/endpoint entries use their separate #167
bounds. Each normalized endpoint is additionally limited to 259 UTF-8 bytes, the maximum
253-byte DNS host plus `:` and a five-digit port; the 128-entry structural ceiling therefore
bounds one reservation's endpoint identities to 33,152 bytes. Each instance carries at most one
sealed authority for each of the three `Language` variants. Endpoint and graph bounds are checked
before input cloning or closure-summary derivation; the authority cardinality bound is checked
before source/program fingerprinting. All instances must
be reachable. Missing authorities, duplicate instances/edges and cycles reject. Typed private
DTOs deny unknown fields and versions; there is no unbounded public decoding entrypoint.

Canonical input order is target, bytewise instance ID and edge ID. The binding covers the complete
canonical metadata input, sealed program observations, exact source bytes and the WIT audit,
including restrictions, selected outputs, approval and narrowed policy. The opaque result retains
the same canonical input and authority binding and requires equality on revalidation.
Witnesses use shortest paths with bytewise sequence tie-breaking. Restriction diagnostics retain
a root-to-leaf witness through the rejecting node. Diagnostics are global because no authenticated
source span is supplied, and never render reservation values, preopen authorities or endpoints.

Reservation sums count each reachable instance once. Distinct instances add; shared diamonds do
not. Environment keys deduplicate only when values agree; conflicts reject. Preopen authority IDs
and normalized lowercase DNS host/decimal-port endpoint entries deduplicate. This fixed input
requires canonical endpoint spellings and opaque preopen IDs; it performs no host lookup or path
normalization. Retained counters and cumulative randomness add with checked arithmetic; per-call
randomness uses the maximum individual reservation. This is static reservation evidence, not
runtime quota consumption, revocation or process-wide isolation.

Stable producing-phase diagnostics use the existing `zryna_diagnostics::Diagnostic` authority:

| Code | Private composition category |
| --- | --- |
| `ZRYNA-C4010` | invalid-composition: shape, structural budget, stale binding or forged claim |
| `ZRYNA-C4011` | unsupported-target: exact target, world, interface or policy mismatch |
| `ZRYNA-C4012` | incompatible-profile: selected language lacks matching sealed program authority |
| `ZRYNA-C4013` | forbidden-capability: transitive restriction, ceiling or approval denial |
| `ZRYNA-C4014` | resource-limit: reservation bound, conflict, undeclared usage or checked overflow |
| `ZRYNA-C4015` | incomplete composition report: the 256th candidate replaces that slot with a terminal diagnostic |

#167's C4000–C4004 meanings and #169 transport rules remain unchanged. This slice allocates no
undeclared-source-operation or missing-host-grant diagnostic because neither phase executes here.

Focused verification is mapped to driver unit tests rather than a new component or integration
test directory: `cargo test --locked -p zryna-driver profile_composition`, plus driver doc-tests
for external opacity. The `tests` module covers pure multi-output closure, diamond witnesses,
independently malformed graphs, profile/world mismatches, forged summaries/quotas/witnesses,
source/policy/edge replay, all graph and diagnostic exact/first-extra limits, every applicable
command/server quota, deduplication, conflicts, overflow, permutation and rejection recovery.
Registry parity and byte-substitution tests retain the owning authority. These located tests
are not a claim that full conformance or hosted checks have passed. Required frozen installation,
structure/docs/WIT checks, preflight, M0 and applicable complete M2/M3/resource and hosted
Linux/Windows gates remain required before integration. Runtime and public conformance stay open.
