# Package source trust and build execution policy v0

Status: proposed specification for [#362](https://github.com/zryna/zryna/issues/362) in
[M5 — Packages and Reproducible Releases](https://github.com/zryna/zryna/milestone/6),
indexed by [#356](https://github.com/zryna/zryna/issues/356). This is a repository-only policy
draft and documentation contract test, with no resolver, executor, sandbox, registry or public
selector. It does not change M0–M3 gates, workspace registration, output manifests or support.
Normative requirements below describe later implementations, not security properties executed here.

## Authority and acceptance dependencies

| Authority | Owned decision | Interface required before acceptance |
| --- | --- | --- |
| [#168 / PR #367](https://github.com/zryna/zryna/pull/367) ([issue](https://github.com/zryna/zryna/issues/168)) | Canonical manifest, lock, checksum inventory, SBOM and release/provenance serialization | Exact source tuple, manifest/source/lock digests, authenticated external expectations and provenance material binding |
| [#360](https://github.com/zryna/zryna/issues/360) | Package-instance identity, aliases, compatibility and host/target graph roles | Exact validated `(id, sourceSha256)` pair; aliases and graph roles do not change that pair |
| [#361](https://github.com/zryna/zryna/issues/361) | Source-only resolved plan and cache identity; optional native input appendix | Branded projection from a fully validated #168 envelope, target/runtime-only package graph, exact source inventory, policy-bound cache projection and driver-owned execution/publication |
| This policy | Source authorization, execution restrictions, isolation responsibilities and exceptions | Caller-selected trust mode and verified enforcement evidence bound to those authorities |
| [#364](https://github.com/zryna/zryna/issues/364) | Relevant native ABI and foreign-resource contracts | Target, ABI and runtime compatibility for the optional native appendix only |

Source policy consumes the accepted [#168 package/release contract](PACKAGE_RELEASE_V1.md) and
#360's dependency-aligned identity vocabulary. Execution policy acceptance depends on the
source-only boundary in #361. Its handoff is a branded projection from a fully validated #168
canonical envelope and binds its exact lock digest, compatibility compiler/profile/target
coverage, root, `(id, sourceSha256)` package pairs, aliases and edges to a package-qualified
path/size/SHA-256 inventory. This policy declares no
second serialized plan.
Acquisition may return verified bytes and metadata under this policy. The driver alone validates
tools, compiles, links, verifies cache outputs and performs create-only atomic publication.
Do not add fields to #168's closed v1 records to carry this draft. Any policy evidence extension
needs its own reviewed version under that serialization authority. Native recipes remain
unavailable until the optional #361 appendix and relevant #364 decisions are accepted; they do
not block source-only acceptance. That appendix is provisional-pending-364: only its relevant
ABI, calling-convention, carrier, ownership and runtime decisions plus this policy are required
before native acquisition/execution, not completion of all M7 work. M6 playground consumers refer
to this policy without moving playground implementation into M5.

The subsequently specified #364 tuple does not retroactively accept #361's native appendix.
The [proposed D2 mapping](RESOLVED_BUILD_PLAN_V0.md#proposed-d2-native-c-v0-alignment) keeps
`proposed-specified-native-c-interop-v0` explicitly proposed and returns `denied-proposed` admission.
Independent exact-revision review is still required for `specified-native-c-interop-v0`; that
unprefixed status currently rejects. Neither status text, matching runtime tuple nor the synthetic
fixture's runtime digest authenticates artifact bytes or grants execution/cache reuse. Runtime
contract compatibility and retained artifact bytes require separate authentic evidence; exact
operator approval, #417 conformance, isolation and #168 evidence remain additional requirements.
Default recipe execution stays denied. Source-only identity and cache behavior are unchanged.

### Identity mapping

The following semantic mapping consumes #360 and the accepted #168 records without defining
another wire format:

| Identity | Required binding |
| --- | --- |
| Source tuple | #168 source kind, canonical locator and exact revision |
| Manifest identity | Lock-package `id`: domain-separated digest of canonical manifest bytes |
| Source identity | Lock-package `sourceSha256`: `source-files` digest of the complete ordered inventory |
| Package instance | Exact ordered pair `(id, sourceSha256)` from one validated lock-package record |
| Dependency selection | Lock edge `package` selects record `id`; authenticate `sourceSha256` from that record |

There is no `packageInstance` or `sourceIdentity` wire field in #168 v1. An alias identifies an
importer's dependency edge, not a new instance or independent source approval. #360 defines
separate `host/build` and `target/runtime` semantic domains, but #361 source-only v0 admits only the
authenticated `target/runtime` root and complete lock graph. A `host/build` package, source or edge
rejects until a later version supplies its own authenticated root and edge authority. The accepted
#168 lock has no build-edge kind and must not be reinterpreted as a mixed-role graph. A host tool
in #361's separate declared tool inventory is not a host/build package and receives no source or
execution authorization from its presence.

#361 validates wire bytes and collection budgets before schema and semantic admission. Every
source path is at most 96 lowercase portable ASCII bytes, and its material map must contain exactly
the declared source keys: missing and extra entries both reject. This policy receives only that
successfully validated branded plan. It preserves #361's earlier rejection and does not relabel a
plan, source or cache-integrity failure as a trust decision. The plan-closure `cacheKey` remains
distinct from each package's `sourceSha256`; release serialization, digest rules and provenance
encoding remain #168-owned. The opaque `executionPolicy` id, version and configuration digest is
a cache input, not acquisition, execution, isolation or trust authority.

## Actors, assets and trust assumptions

Protect the authorized source/lock graph, compiler inputs, host secrets, other tenants, caches,
artifact integrity and truthful provenance. An adversary may control dependency source and
metadata, an acquisition response, a writable cache, a recipe, or a playground submission.
Checksums supplied by that same adversary do not establish trust. A correctly hashed malicious
package remains malicious; signatures authenticate an authorized statement, not safe behavior.

The trusted computing base comprises the caller's approval channel, independently authenticated
compiler/tool distribution, resolver and driver verifiers, isolation supervisor and host OS.
Compromise of those authorities is outside this policy's proof. Parser exploits, resource abuse,
native tool behavior and concurrent filesystem mutation require the controls below even for
source-only compilation. Current [strict workspace](../../docs/STRICT_WORKSPACE.md) checks do
not prove safety against a hostile process replacing ancestor directories concurrently.

| Actor / component | Responsibility | Must not delegate to package content |
| --- | --- | --- |
| Workspace owner or release operator | Approve exact dependency source/lock expectations and trust mode through an authenticated local or reviewed release policy | Source changes, approval of recipes, namespace remapping or mode escalation |
| Package resolver and acquisition layer | Validate #168/#360 identity, complete graph, provenance of expected digests, safe materialization and bounded source bytes before handing off | Namespace selection, credential forwarding or acceptance of colocated replacement checksums |
| Cache reader/writer | Revalidate bytes and metadata against external expectations; isolate writers; stage complete entries before visibility | Cache directory name, mtime, cached success bit or cached trust mode as authority |
| Driver | Revalidate exact plan, source snapshot and effective policy; admit pinned tools; own compilation, linking, auditing and create-only publication | Shell commands, tool lookup, target selection, execution or publication authority |
| Isolation supervisor / deployment operator | Enforce network, process, filesystem, environment and resource limits for the complete process tree; attest effective policy | Claims that a manifest permission, sanitized environment or subprocess timeout is an OS sandbox |
| Provenance producer and consumer | Record actual inputs/actions/isolation outcome under #168; independently verify bindings and authenticated builder expectations | Treating self-reported or fixture-only evidence as executed or authenticated proof |
| Playground host (M6 consumer) | Supply approved preverified inputs, isolate compilation and evaluation separately, enforce tenant separation and bounded outputs | User-supplied tools, native targets, credentials or trusted-native opt-in |

The driver remains the orchestrator under [Architecture](../../docs/ARCHITECTURE.md); the package
layer never invokes compilation or linking itself. Manifest permissions are not an OS sandbox.
An absent or unverifiable enforcement capability rejects before any affected process starts.

## Source identity, namespace and acquisition

1. Accept only the exact source forms and canonical spellings admitted by #168/#360. In the
   accepted #168 v1 these are local locators rooted in the declared reproduction root and
   canonical HTTPS Git locators with exact commits. Moving the reproduction root is portable;
   changing the relative locator changes identity. Branches, tags, implicit submodule fetches,
   Git LFS fetches and checkout filters are not source authority. Acquisition must materialize
   the declared regular-file inventory without executing repository hooks, filters or helpers
   selected by dependency content. An undeclared external material requirement rejects.
2. Bind each dependency alias to the full approved name/version/source tuple and #360 instance,
   then to #168's complete manifest and source inventory digests. A version or commit identifies
   a requested source, but never replaces source-byte verification. Replacing bytes and their
   adjacent checksum together must still fail against the independently approved lock/manifest.
3. A namespace means an explicit source authority binding, not a search order. Distinct sources
   may carry the same name only as distinct explicitly declared instances. Aliases do not grant
   permission to switch sources. An unavailable private source never falls back to a public
   service, workspace directory, mirror or higher version. Reject ambiguous selection tuples,
   canonicalization collisions and unsupported case/Unicode spellings before acquisition.
   Do not silently normalize them into another identity; #168 owns the exact accepted grammar.
4. Network acquisition is a separate caller-authorized step, restricted to approved endpoints
   and exact requested materials. The acquisition broker controls credentials, redirect and DNS/IP
   policy, response/expansion limits and timeouts. Cross-authority redirects or alternate mirrors
   require independent approval retaining the original logical source and expected content.
   Repository configuration cannot override the approved origin, helper or transport policy.
   Playground acquisition additionally denies loopback, link-local, private service and metadata
   destinations, including redirect/DNS changes; it uses an operator-approved catalog, not
   arbitrary submission URLs. Credentials never enter a compiler or recipe environment.
5. Consume #361's exact material map and validate complete bounded inventories and exact raw bytes,
   without newline normalization, before compiler input construction. Each declared source path is
   at most 96 lowercase portable ASCII bytes. Reject missing/extra material entries or files, path
   traversal, prefix/case collisions, links, reparse points, hard-link aliases and special files. Any later archive
   transport must prove the same containment and expansion limits; archives are not introduced
   by v0. Materialize a private immutable snapshot, or retain equivalently protected verified
   handles; the bytes compiled must be the bytes verified. Mere check-then-reopen is insufficient.
6. An explicit source update requires approval of the new identity, manifest and complete graph
   before creating a new lock. Preserve the old lock and usable outputs on any rejection.
   No mode has an integrity-bypass switch, including trusted-native.

## Cache, offline and frozen behavior

Source storage may deduplicate equal bytes, but lookup and admission must retain the approved
source/instance/manifest association. A cache hit does not establish that association. Rehash
every consumed material against independently retained expected size/hash and complete inventory;
do not trust cached metadata or a previous validation result. Protect the verified snapshot from
mutation between validation and use. Shared writable caches are untrusted inputs, never shared
execution roots. A mismatched entry is rejected and not executed; it is not silently repaired
during the failing request. A separate explicit acquisition may stage a replacement from an
approved source, verify it fully and publish it atomically under a new complete entry.

### Cache projection

Artifact cache admission uses #361's exact plan identity and cache projection. The following
bindings describe the received interface, not new field spellings or digest rules:

| Binding | Required content |
| --- | --- |
| Execution policy | Policy id, version and configuration digest |
| Environment | Relevant declared environment |
| Host | Host triple |
| Compiler and tools | Compiler/tool versions and hashes |
| Target execution | Target, runtime and profile |
| Input graph | Validated #168 lock projection and target/runtime source closure |
| Result inventory | Outputs |
| Native extension | Optional native inputs |

An absent artifact cache key is a miss: with all verified inputs available, the driver may build
under the current policy. This is distinct from a missing required source/tool in offline/frozen
mode, which rejects. An entry found at the requested key that is incompatible, stale, incomplete
or has wrong target/output bytes retains #361's `P361-CACHE` rejection; it must not be relabeled as
a policy decision or transparent miss. After that structural and cryptographic validation succeeds,
the driver verifies that authenticated acquisition, execution-policy and isolation evidence
authorizes the current request. Missing, stale or incompatible trust evidence rejects as
`TRUST-PLAN`. Effective restrictions and isolation evidence must meet the request even on a cache hit. A
permissive native result cannot enter a pure-source or playground build through reuse. Content
deduplication must not equate approval records; authenticated prior evidence or fresh verification
is required. Interrupted entries never become hits, and no failure publishes a final bundle.

### Resolution requests

| Resolution request | Network acquisition | Lock mutation | Missing or altered material |
| --- | --- | --- | --- |
| Explicit online acquire/update | Only separately approved broker operations before build execution | New verified lock only after explicit update approval | Reject mismatch; a separately requested verified acquisition may populate a missing entry |
| Offline | Denied, including redirects, tool downloads and cache repair | Denied; exact validated lock and already available declared materials only | Reject; never infer another source or version |
| Frozen | Denied, even when the caller has connectivity | Denied; exact approved lock required | Reject missing/stale lock, missing input or altered bytes; never retry online |
| Offline and frozen | Denied | Denied | Same frozen rejection; flags do not weaken each other |

These are policy terms, not new CLI flags. Offline/frozen requests cannot update or repair the
lock under #168/#360; any explicit update is a separate operation, never an offline exception.
Frozen includes the network prohibition; a future lock-preserving online fetch must have a
distinct explicit operation. A clean frozen build first
receives the authenticated complete input closure in a fresh private root through a separate
preparation step, then starts with empty artifact cache and network disabled. Tool/runtime inputs
must also be preprovisioned. Offline operation proves integrity against available approved records,
not freshness of remote revocation/yank state. If caller policy requires fresh authenticated
status that is unavailable offline, reject; do not invent freshness or silently contact a service.

## Execution modes and allowed operations

The caller selects a mode; a dependency cannot select or upgrade it. Unknown modes reject.
`allow` still requires verified inputs and containment. `conditional` means the corresponding
condition below must be proved before dispatch; lack of proof means deny. Denial is transitive
through dependencies and descendants; approving one recipe never approves its descendants.

### Operation matrix

| Operation | pure-source | trusted-native | untrusted-playground |
| --- | --- | --- | --- |
| read-verified-source | allow | allow | allow |
| acquire-source-before-build | conditional | conditional | conditional |
| compiler-tool-process | conditional | conditional | conditional |
| driver-owned-link | conditional | conditional | deny |
| dependency-install-hook | deny | deny | deny |
| native-recipe | deny | conditional | deny |
| load-native-dependency | deny | conditional | deny |
| arbitrary-shell-or-path-tool | deny | deny | deny |
| network-during-build | deny | deny | deny |
| read-ambient-secrets | deny | deny | deny |
| write-source-or-shared-cache | deny | deny | deny |
| write-private-staging | allow | allow | allow |
| execute-built-program | deny | deny | conditional |
| publish-build-artifact | conditional | conditional | deny |
| reuse-build-cache | conditional | conditional | conditional |
| escalate-mode | deny | deny | deny |

Conditions and isolation assumptions:

- **Acquisition:** only the approved broker before execution, never offline/frozen. Playground
  inputs come from its approved catalog. Compiler workers receive no acquisition credentials or
  general network channel; acquisition grants are not inherited by the build.
- **Compiler tools and driver linking:** only independently trusted, version/digest-pinned tools
  admitted by #361 and the existing driver, with closed versioned typed invocation-adapter records,
  an explicit private working directory and a minimal declared environment. Resolve executables
  independently of source-controlled `PATH`; no shell interpolation or raw source-selected argument
  array is admitted. Typed source, response-file, plugin, library and output references must resolve
  to exact declared plan identities. The linker output must match an exact target-qualified plan
  output. Pure-source may use a driver-owned validated toolchain for already-admitted source output,
  but never dependency-provided recipes or native libraries. Playground permits only compiler
  tools inside its enforced isolation, and no native linker or native result execution.
- **Trusted native (conditional extension):** per invocation, the operator explicitly approves
  the exact recipe digest, host executable digests, typed invocation-adapter records/environment,
  transitive declared tools and materials, target/ABI/runtime and output inventory. A changed digest or descendant
  tool requires new approval. Check #361's native appendix and relevant #364 admissibility first.
  Recipes and dynamic/native libraries can execute arbitrary machine code; review is not a
  sandbox. The supervisor must enforce offline execution, read-only verified inputs, private
  scratch/output roots, no ambient secrets and bounded descendant processes. If these controls
  cannot be established on the host, reject this mode; a future unrestricted local escape hatch
  is outside v0. Acquisition of target libraries precedes compilation; the driver independently
  verifies host/target roles and owns linking. Do not execute a target library as a host tool.
- **Pure source:** no install/postinstall hooks, source-supplied generators or executable macros.
  Source files are data for the trusted compiler. Use a private workspace with no untrusted
  concurrent writer; protect retained source bytes and prevent network/ambient inputs for any
  compiler subprocess. A local trusted operator supplies that environment; calling a build
  pure-source alone is not an assertion that hostile input is safely sandboxed.
- **Playground:** operator-enforced deny-by-default network/process/filesystem/environment
  isolation must cover compilation and a separate bounded evaluation worker. No user recipes,
  native modules, host executables or cross-tenant writable roots. Evaluation is a later M6
  action using only an admitted JS/WASM profile with its own capability restrictions, never a
  build hook or new profile activation here. A browser worker, Wasm module or timeout alone is
  not evidence of full host isolation. An unavailable isolation boundary rejects before start.
- **Staging, outputs and reuse:** restrict writes to job-private scratch and bounded outputs.
  The supervisor applies reviewed CPU/wall-time/memory/process/output budgets to the whole
  process tree; unsupported controls, over-limit execution or unconfirmed teardown fail closed.
  Numeric budgets belong to the later platform execution contract and require exact/first-extra
  tests before activation. The driver alone may commit fully audited build artifacts create-only;
  this does not authorize package or registry publication. Playground returns bounded results
  through its host and cannot publish a build bundle. Cache reuse follows the preceding section;
  isolation, source and native admission checks cannot be skipped on hits.

## Threat and enforcement matrix

Rows below use #361's retained fixture category where it owns rejection and otherwise use proposed
policy case identifiers; none is a public compiler diagnostic code.
When implemented, check stages in this order: `request`, `identity`, `source`, `plan`,
`execution`, `publication`. Stop before any effect forbidden by the failing stage. Within a
stage use canonical instance/path order and then the rejection identifier, never network arrival
or directory order. A malformed #168 envelope or invalid #361 projection retains that authority's
earlier rejection; this table begins with a validated plan. In particular, #361 owns artifact-cache
identity and byte integrity through `P361-CACHE`; this policy owns only later reuse authorization.
Return no accepted source/plan/result capability on failure.
Include the stable reason and non-secret logical identity; redact credential-bearing URLs and
host paths. Cleanup failures must be reported, and private failed-job state must not be reused.

### Threat matrix

| Threat | Responsible enforcer | Stage | Rejection | Later gate |
| --- | --- | --- | --- | --- |
| unknown-policy | Driver / caller policy verifier | request | TRUST-MODE | policy-matrix |
| namespace-confusion | Resolver | identity | TRUST-NAMESPACE | source-adversarial |
| source-substitution | Resolver / acquisition verifier | identity | TRUST-SOURCE | source-adversarial |
| frozen-lock-drift | Resolver | identity | TRUST-FROZEN | clean-offline |
| missing-offline-input | Acquisition / cache reader | source | TRUST-OFFLINE | clean-offline |
| checksum-mismatch | Acquisition verifier | source | TRUST-CHECKSUM | source-adversarial |
| tampered-cache | Cache reader / retained-source verifier | source | TRUST-CACHE | cache-adversarial |
| unsafe-materialization | Source materializer / isolation supervisor | source | TRUST-PATH | source-adversarial |
| unauthorized-fetch | Acquisition broker | source | TRUST-NETWORK | broker-isolation |
| artifact-cache-integrity | Driver / #361 cache verifier | plan | P361-CACHE | cache-adversarial |
| unauthorized-cache-reuse | Driver / #362 policy verifier | plan | TRUST-PLAN | cache-adversarial |
| undeclared-tool | Driver tool verifier | plan | TRUST-TOOL | native-adversarial |
| unapproved-native-recipe | Driver / operator approval verifier | plan | TRUST-NATIVE | native-adversarial |
| forbidden-operation | Driver / isolation supervisor | plan | TRUST-OPERATION | policy-matrix |
| missing-isolation | Isolation supervisor | plan | TRUST-ISOLATION | process-isolation |
| execution-escape | Isolation supervisor | execution | TRUST-ESCAPE | process-isolation |
| execution-budget | Isolation supervisor | execution | TRUST-BUDGET | process-isolation |
| incomplete-provenance | Driver / provenance consumer | publication | TRUST-PROVENANCE | provenance-binding |
| incomplete-publication | Driver / cache writer | publication | TRUST-PUBLICATION | publication-atomicity |

## Deterministic examples and later negative fixtures

Use a synthetic clean catalog with `math` version `1.0.0` from a declared private Git origin at
exact commit A and approved digest H; alias `calc` binds that instance. A public origin may offer
the same display name and version, even identical bytes, but is not that authorized source.
Letters A/H denote fixed fixture values to instantiate under #168, not legal wire hashes.
Each row changes only its stated precondition unless it explicitly names multiple failures.
`accept` means permitted to continue to the next verified boundary, never public support.
`miss` permits a build from the complete verified closure; it grants no artifact reuse or fetch.

### Case matrix

| Case | Changed input / request | Outcome | Stage | Later gate |
| --- | --- | --- | --- | --- |
| clean-frozen | Complete approved local source/tool closure; exact lock; network disabled; empty artifact cache | accept | source | clean-offline |
| offline-missing | Same frozen request with one required source absent | TRUST-OFFLINE | source | clean-offline |
| frozen-tool-missing | Frozen source closure complete but a pinned compiler tool is not preprovisioned | TRUST-OFFLINE | source | clean-offline |
| frozen-lock-changed | Lock has a new source revision although all cached bytes are present | TRUST-FROZEN | identity | clean-offline |
| private-public-confusion | Private origin unavailable; resolver is offered public math under alias calc | TRUST-NAMESPACE | identity | source-adversarial |
| substituted-origin | Well-formed replacement origin with same name/version and identical bytes H | TRUST-SOURCE | identity | source-adversarial |
| replaced-digest-pair | Acquisition response changes both content and its adjacent checksum; approved digest remains H | TRUST-CHECKSUM | source | source-adversarial |
| tampered-cache-byte | Cache key and inventory still claim H but one consumed byte changed | TRUST-CACHE | source | cache-adversarial |
| tampered-cache-metadata | Attacker rewrites cached content and cached digest together; approved manifest still names H | TRUST-CACHE | source | cache-adversarial |
| source-race | Cached path is replaced after checking but before attempted snapshot consumption | TRUST-CACHE | source | cache-adversarial |
| path-escape | Materialization contains a parent path, link, reparse point or hard-link alias | TRUST-PATH | source | source-adversarial |
| redirect-substitution | Approved acquisition redirects to an unapproved authority or private service address | TRUST-NETWORK | source | broker-isolation |
| artifact-cache-miss | Exact requested artifact cache key is absent; complete verified source/tool closure remains available | miss | plan | cache-adversarial |
| wrong-target-cache | Valid artifact bytes and source digest, but another target/ABI or plan binding | P361-CACHE | plan | cache-adversarial |
| policy-cache-mismatch | Structurally valid #361 result has matching bytes, but its authenticated execution-policy or isolation evidence does not authorize this request | TRUST-PLAN | plan | cache-adversarial |
| permissive-cache | Pure-source request offered a trusted-native result without compatible policy evidence | TRUST-PLAN | plan | cache-adversarial |
| path-tool-injection | Valid native approval, but tool path now resolves to different bytes via PATH | TRUST-TOOL | plan | native-adversarial |
| native-without-opt-in | Trusted-native request has no approval for its exact recipe or changed descendant tool | TRUST-NATIVE | plan | native-adversarial |
| pure-source-hook | Pure-source dependency requests a postinstall hook despite correct source checksums | TRUST-OPERATION | plan | policy-matrix |
| playground-native | Playground dependency requests a native recipe or native module | TRUST-OPERATION | plan | policy-matrix |
| sandbox-unavailable | Playground host cannot enforce network and descendant-process isolation | TRUST-ISOLATION | plan | process-isolation |
| network-child | Approved native recipe starts a child that attempts a network connection | TRUST-ESCAPE | execution | process-isolation |
| budget-extra | Execution attempts one more process/output byte than its reviewed limit | TRUST-BUDGET | execution | process-isolation |
| missing-policy-evidence | Artifact checksums match, but execution policy/isolation evidence is absent | TRUST-PROVENANCE | publication | provenance-binding |
| interrupted-write | Stop the writer after staging some bytes and before complete entry/bundle commit | TRUST-PUBLICATION | publication | publication-atomicity |
| unknown-mode | Caller supplies an unsupported trust mode with otherwise valid inputs | TRUST-MODE | request | policy-matrix |
| earliest-rejection | Unknown mode together with a stale lock and tampered cache | TRUST-MODE | request | policy-matrix |

For every rejected pre-execution case, later tests must independently observe no compiler/recipe
start, no prohibited acquisition and no published result. Execution cases must observe blocked
effects, descendant teardown and no accepted result; publication cases retain prior outputs.
Repeat each case with reversed input enumeration and in a fresh root: reason, logical identity
and preserved state must match. Cache corruption must not be recast as a transparent miss/fetch.
The `clean-frozen` gate requires two clean builds in different roots, exact artifact/metadata
comparison, zero network attempts and an unchanged lock. It is a required later test, not a build
performed by the documentation checks. Native positives require all appendix/ABI prerequisites
and exact approval; the same invocation with missing approval or a changed tool must reject.

## Provenance and trust exceptions

Under #168's serialization authority, provenance must bind actual source/instance/lock and plan
identities; compiler/tool/runtime inputs; effective mode, policy version and explicit recipe
approval; acquisition origin and authenticated expected digests; actual isolation provider and
enforced restrictions; and audited output hashes. Record which steps ran versus reused evidence.
Never record credentials, private environment values or an unevaluated sandbox claim as proof.
The consumer authenticates the builder and evidence against independent policy. Missing, stale
or mismatched binding rejects; a signed statement alone cannot grant more authority than its
signer was approved to exercise. The precise evidence encoding remains a reviewed #168 extension.

The operation conditions and threat gates enumerate all v0 exceptions: explicit acquisition,
driver-owned tools/linking, native recipes/libraries, playground evaluation, cache reuse and
create-only build publication. Each has denial when approval/evidence is absent, containment and
a later negative gate. There is no implicit transitive approval, arbitrary hook exception,
integrity exception or offline network exception. A new operation requires a reviewed policy
version, explicit owner and independent negative tests before it can be admitted.

## Optional later registry v0 outline

Compiler distribution through the existing planned npm, crates.io, Docker and GitHub release
channels is distinct from a Zryna-language dependency registry. This policy neither publishes
compiler artifacts nor operates a language registry. Registry UI, deployment, accounts, signing
keys and hosted service operations remain deferred and cannot block source-only package work.

A separately reviewed minimal protocol could have these authenticated operations; these names
describe semantics, not endpoints or added source forms in #168 v1:

| Operation | Required immutable binding / authority | Deterministic rejection |
| --- | --- | --- |
| Publish version | Authenticated publisher authorized for exact origin/namespace/name; bind version to manifest and complete material digests atomically | Unauthenticated/unauthorized publisher, namespace mismatch or any existing version; never overwrite, including identical retry unless a separately specified idempotency token proves the same transaction |
| Resolve/fetch | Explicit origin and namespace; authenticated version metadata and exact manifest/material digests; verify bytes independently | Alternate source fallback, conflicting version binding, invalid authentication, incomplete material or checksum mismatch |
| Yank version | Authorized namespace owner; append authenticated status for the existing immutable version without changing its bytes or freeing its name | Unknown version, unauthorized or conflicting status update; unyank semantics require a later explicit decision |

Yank excludes a version from new selection. An existing approved frozen lock may continue using
the exact cached yanked version when local policy permits: yanking is not substitution, deletion
or automatic security revocation. A future explicitly online fetch for that locked version may
retrieve the same immutable bytes with authenticated yank status; a frozen operation never
fetches. Missing locked materials still reject offline. Separate authenticated security revocation
may forbid even cached execution; offline consumers cannot claim knowledge beyond their retained
status. A freshness requirement without available evidence rejects. Version listing and yank
metadata need authenticated sequence/rollback protection anchored in previously trusted state;
transport encryption or a checksum served beside content alone is insufficient. Trust-root
bootstrap, rotation/revocation, idempotency and wire limits must be reviewed and negatively tested
before protocol acceptance; no cryptographic service is claimed here.

## Bounded rollout and evidence

| State / slice | Prerequisites | Measurable exit gate |
| --- | --- | --- |
| Proposed specification (this change) | Scope #362; preserve #168/#360/#361 authority | Reviewed threats, complete mode matrix, deterministic cases, documentation contract tests and required repository verification; unresolved interfaces remain explicit |
| Specified source policy and execution boundary | Accepted #168/#360 and source-only #361 handoff with final evidence bindings | Review every source, mode and cache admission edge; independently authored malformed/negative fixture plan; no public activation |
| Source-only prototype (later approved work) | Specified source policy; registered implementation ownership and bounded materialization design | Implement identity/cache verification without network execution hooks; pass source-adversarial, cache-adversarial and clean-offline cases |
| Source-only conformance-passed | Prototype plus platform isolation/retained-source and provenance design | Linux/Windows clean-root replay, broker-isolation, policy-matrix, process-isolation, provenance-binding and publication-atomicity evidence including exact/first-extra limits |
| Optional native prototype/conformance | Only relevant accepted #361 native appendix and #364 ABI decisions, explicit tool approval and host isolation | Native-adversarial cases, host/target mismatch, denied child network/tool escape, missing library, teardown and exact approval positives on each admitted host |
| Optional registry protocol review | Source policy; reviewed authentication/immutable status model | Independent namespace takeover, unauthorized publish/yank, immutable overwrite, metadata rollback, tampering and offline/yank negative cases; hosting separately approved |
| Publicly-supported activation | Applicable conformance plus explicit CLI/support/security review | Publish exact supported modes/platforms and limitations under separate acceptance; M6 playground proves its own isolation before consuming this policy |

The focused [documentation tests](../../tests/package-source-trust.test.mjs) pin operation,
responsibility, rejection and example tables and reject mutations that weaken them. They parse
policy text only; they do not validate real packages, run adversarial programs, enforce isolation,
authenticate provenance or execute the later gates. The additive workflow runs those checks on
Linux and Windows without editing historical conformance inventories or the website whitelist.

Run the focused check with pinned Node `22.22.1`:

```bash
node --test tests/package-source-trust.test.mjs
pnpm docs:check
pnpm structure:check
```

Repository-required installation/verification, preflight, M0 and hosted portability evidence
remain required by [CONTRIBUTING.md](../../CONTRIBUTING.md) before submission/integration.
Focused success is not a completed heavy gate, sandbox proof or authorization to publish.
