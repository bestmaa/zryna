# M6 tooling conformance v1

Version: 1. Status: final integration conformance and restricted-playground publication are pending.
This contract defines the evidence required for the bounded tooling implementation. It does not
record a completed gate, grant an executable capability, publish a toolkit or activate debugging.
The single user-facing support matrix is [M6 tooling support and evidence](../../docs/M6_TOOLING.md).

The existing [project contract](../../docs/STANDALONE_PROJECTS.md),
[LSP/formatter/editor contract](../../docs/LANGUAGE_SERVER.md),
[portable setup status](../../docs/PORTABLE_SETUP.md),
[semantic query/snapshot contract](SEMANTIC_QUERIES_V1.md),
[package source-trust contract](../package/SOURCE_TRUST_V0.md) and
[compiler architecture](../../docs/ARCHITECTURE.md) retain their respective acceptance states.
Source, syntax, semantics, IR, ABI, diagnostics and execution authority remain with their existing
owners. Tooling consumers may convert coordinates and render results; they may not duplicate semantics.

## 1. Integration and artifact identity

One final M6 record MUST bind an independently authenticated repository, complete integration
commit, tree, clean source state, exact conformance revision and declared supported platforms.
Working-tree changes, an unrelated hosted run or a binary from another revision cannot establish
that integration's behavior. Any change to source, policy, profile, artifact or documentation after
verification requires rerunning affected gates and the required final integration checks.

Each artifact MUST record product/role, exact version, original source commit/tree, exact path,
byte length, SHA-256, mode/inventory identity where applicable, construction/provenance receipt,
and immutable durable artifact location. Reused published artifacts retain their original source
identity and MUST match the declared integration composition byte-for-byte. They do not transfer
their historical test outcomes to the final integration. Rebuilding different bytes under an
immutable published version is not permitted.

The version composition is compiler 0.2.3, server/VSIX 0.5.0 and setup 0.1.0-candidate.3;
the separate restricted toolkit's proposed product version is 0.1.0. These version numbers are
not a single release number. Published VSIX evidence, signed nested compiler evidence, unsigned
reviewer-delivered setup evidence and proposed outer-toolkit publication MUST remain distinct.
No nested signature authenticates new outer bytes.

## 2. Evidence record v1

The final record MUST identify these fields without replacing missing values with a pass:

| Field group | Required contents |
| --- | --- |
| Record identity | Conformance version 1, repository, integration commit/tree, source cleanliness and UTC execution timestamps |
| Composition | Every compiler/server/VSIX/setup/toolkit/browser/provider/runtime/helper/policy artifact's role, version, source, size, hash and exact inventory |
| Trust | Independently selected publisher identity/issuer, trusted-root and verifier pins, outer/nested signature bundles, provenance and verification receipts; reviewer-handoff identity for an unsigned candidate |
| Environment | Native OS/version, architecture, kernel, runtime versions, editor/browser versions, host capability policy and actual supported/unsupported profile selection |
| Execution | Exact command/argument array, working directory/profile, input/corpus hashes, gate ID, start/end/deadline, exit status and executed/ignored/skipped counts |
| Results | Actual diagnostics/locations, scalar observations, artifact hashes, resource/cancellation/cleanup observations and separately preserved primary/cleanup failures |
| Documentation | Exact source/exported-document hashes, support-matrix revision and executed example identities |
| Retention | Immutable artifact/document/log/receipt locations and their hashes, hosted run/job identities and final status |
| Acceptance | Per-row passed/failed/not-run/unsupported state, outstanding prerequisites and explicit support/publication status |

The versioned [machine registry](../../scripts/m6/registry.mjs) owns complete gate/platform/case
identities. The [closed evidence validator](../../scripts/m6/evidence.mjs) binds its digest and
requires each gate's relevant platform-specific artifact roles. The
[detached receipt validator](../../scripts/m6/receipts.mjs) binds exact observations, material
identities, working directory, profile, corpus and retained input/stdout/stderr bytes. The
[closure consumer](../../scripts/m6/closure.mjs) checks that complete inventory and the independently
selected source certificate claims through the pinned static verifier. Final publication receipts
remain outside the toolkit archive; the publication descriptor equals its complete gate receipt.
These source implementations and synthetic tests do not establish actual signature, download,
gate execution or executable host admission. The evidence validator
requires independently selected integration/artifact pins, complete nonzero results and durable
publication receipts; metadata validation does not execute or prove those gates.
A passed row requires observed nonzero execution of its intended cases with all mandatory
subcases present. A test listing, synthetic frame, adjacent self-authored hash, skipped test,
expired artifact or unavailable environment does not satisfy that row.

Unsigned setup-candidate observations MAY remain historical evidence with their reviewed handoff
identity. They MUST NOT be reported as public outer-signature verification. Final public restricted
toolkit admission requires its own independent outer authentication and finite complete archive.
The proposed immutable `playground-v0.1.0` prerelease is not an existing release or download location.

## 3. Complete acceptance matrix

Every advertised row is required. Unsupported methods/platforms additionally need fail-closed
negative evidence; a declared unsupported result is not a positive host-admission pass.

| Gate family | Required nonzero acceptance | Platforms/authority |
| --- | --- | --- |
| Project creation/resolution | Exact generated manifests/lock/source, portable unsafe-path rejection, create-only collision handling, frozen inventory, retained-root identity and cleanup | Linux and Windows; actual compiler/project authority |
| Project build/run | Supported default i32-v1 JavaScript/core WebAssembly observations and bundle identity; forbidden profile/target/runtime/root override rejection | Linux and Windows; installed and source-checkout modes as advertised |
| LSP lifecycle/diagnostics | Real stdio framing, full-text open/change/close, exact negotiated Unicode/CRLF coordinates, actual complete compiler diagnostics, cancellation/stale recovery and unsupported method rejection | Linux and Windows; exact matching server/source |
| LSP definition | Scalar declaration/use authority and valid absent results; foreign/stale coordinates reject; M2/M3 definition remains unavailable | Linux and Windows; scalar semantic index |
| Formatter | Scalar, explicit M2 and explicit M3 document/range formatting, idempotence, exact token/comment preservation, unchanged outside-range bytes, saved-import revalidation, no edits for rejected/stale/cancelled/over-budget requests | Linux and Windows; verified selected compiler snapshot |
| Installed editor | Exact published/reviewed VSIX, verified matching server capability/source before source transmission, trust/settings isolation, profiles, diagnostics/definition/formatting and explicit scalar/M2 saved-file Run/output identity | Actual supported editor hosts; every advertised platform requires its own final receipt |
| Setup composition | Exact complete installed candidate inventory, relocation/tamper rejection, compiler/server/VSIX compatibility, fresh project/editor exercise and independent reproduction where claimed | Linux and Windows packaging/portable acceptance; historical candidate status remains explicit |
| Source-playground corpus | Actual edited source compiled with actual CLI diagnostic/scalar parity, multiple exports/arity, literals and rejected examples; generated audited component/bindings and real browser evaluation | Authenticated compiler and selected Chrome; candidate execution host only |
| Playground transport/lifecycle | Closed request/reply shapes, exact bounds, malformed/oversized/expired/unsupported rejection, one operation/no queue, edits/Cancel, stale response suppression, authority substitution and recovery | Linux/Windows transport negatives plus actual admitted host lifecycle |
| Host containment | Required helpers/kernel policy, finite memory/tasks/CPU/output/storage/deadlines, membership before source, namespace/seccomp/capability denials, containment-wide cancellation and proved teardown | Non-root authenticated Ubuntu 24.04 x86-64 host; unsupported hosts reject before execution |
| Browser containment | Dedicated complete process tree, resource baseline, independent evaluation watchdog, malicious/stalled worker/session cleanup, no escaped descendants or residual observations | Exact pinned Chrome with accepted finite browser policy |
| Accessibility | Keyboard/focus/labels/live announcements, editable source, canonical input errors, actual diagnostic navigation and inert hostile text in real browser | Actual browser accessibility-tree and keyboard exercise |
| Documentation/publication | One consistent matrix, runnable verified examples, exact exported documentation, immutable artifacts and independent signature/provenance verification | Final integration documents/artifacts and public admission policy |
| Canonical/hosted checks | Required architecture, formatting/lint, workspace/protocol/adapter tests, M0 and applicable M2/M3/tooling/packaging gates, complete required hosted jobs | Final integration on both required CI operating systems; real host lanes separately recorded |

Installed-editor rows MUST distinguish a native graphical host, WSL transport, portable command
acceptance and an extension development-host fixture. Earlier Windows installed-marketplace
observations do not establish Linux graphical-host acceptance. A platform claim cannot be broadened
by labeling transport tests as an editor-host run.

## 4. Compiler/browser corpus parity

The evidence MUST preserve exact UTF-8 source, its digest and monotonic revision for every case.
Preset labels and expected values are comparison oracles only. Results MUST come from actual
compiler diagnostics and generated artifacts, never a source catalogue, handwritten evaluator,
cached answer or substituted component.

The admitted corpus MUST include i32 addition, edited literal, nested addition, zero-argument
and multiple-export sources, inclusive signed-i32 boundaries and wrapping observations. Rejected
cases MUST include explicit `any`, gated bool output, parse failure, unresolved names and
unsupported statements. Their code/severity/path/byte/line/column/message/guidance records MUST
be compared with the actual CLI structured-schema-1 reports for the same source/compiler inputs.
Provider syntax codes remain actual provider codes; host/configuration errors cannot be converted
to fabricated source diagnostics. LSP structured diagnostics v2 remains unchanged.

Scalar comparisons MUST identify actual export, arity/inputs, component/core/interface/binding/WIT
identities and observed signed-i32 output. Each browser component must descend from the existing
source compiler, mandatory IR verifier, ABI validation, component audit and deterministic binding
producer. Exact frame/artifact hashes alone do not establish executable provenance.

## 5. Restricted host and resource proof

Candidate source is one fixed `src/main.zry` with 4096 UTF-8 bytes inclusive. Requests are capped
at 32768 bytes, combined output frames at 1277956 bytes and stderr at 4096 bytes. Components are
capped at 1048576 bytes; each generated binding at 65536 bytes. Diagnostic report limits are
65536 bytes, 256 records and 4096 bytes for each message/guidance field. Invalid UTF-8, duplicate
JSON keys, noncanonical numeric carriers, unknown fields and mismatched source/revision reject.

Compilation uses one operation without a queue: 10 seconds execution plus 2 seconds complete
teardown, 512 MiB memory, zero swap, 32 tasks including threads, two CPUs by quota and effective
cpuset. Evaluation allows 5 seconds plus 2 seconds teardown with an independently armed host
watchdog. The proposed dedicated-browser baseline is 1 GiB, 256 tasks and two CPUs; it is not yet
an accepted numeric policy. Final admission MUST bind and positively verify the accepted browser
baseline rather than silently inheriting compiler limits or treating a worker timer as confinement.

Every numeric boundary needs exact-limit and first-extra evidence at the owning authority, plus
process-tree and recovery proof. Deadlines cover provider startup, stalled request/output pipes,
descendants and complete cleanup. Kill alone, process exit alone or population zero alone cannot
prove teardown: adopted children must be reaped and owned resources removed or retained with
explicit cleanup failure. Both handles/descriptors and primary/cleanup failures remain accounted
for. An unproved cleanup MUST stop further session admission.

The host requires non-root Ubuntu 24.04 x86-64, actual authenticated kernel/helper capabilities
and an operator-provided empty delegated cgroup v2 domain with cpu/cpuset/memory/pids controllers.
Tests MUST demonstrate filesystem, network, process, credential/environment, writable-cache,
cross-session and user-selected executable/argument/target denial. No controller enabling,
privilege relaxation or system-security change is an admission fallback. The fixed official
bubblewrap 0.12 acquisition route and exact helper receipt remain under review. An OS label or
WebAssembly label does not prove isolation.

Host authentication MUST precede helper/provider execution. Signature verifier/trusted-root,
kernel/helper and nested compiler/browser material identities are independently selected, bounded
and revalidated; a response or adjacent manifest cannot supply its own authority. The executable
closure and complete archive inventory must be authenticated before loading generated bindings.
Selected browser acceptance pins are Chrome 153.0.8010.12 and playwright-core 1.63.0.

Public sessions MUST reject packages/imports, native execution, caller-selected paths, targets,
commands, generic argv, dependency fetching and ambient capabilities. No supported launcher or
actual authenticated end-to-end host/browser receipt exists in this conformance record yet.

## 6. Revision, user interaction and documentation proof

Every source mutation MUST invalidate compiled exports, diagnostics, identity and observations,
cancel pending work and increment the admitted source revision. Same-length edits and edit/undo
cannot revive previous authority. Late compile/worker replies MUST be discarded. Export input
fields derive from returned arity; values must be canonical decimal signed i32, including both
inclusive endpoints and rejecting negative zero, whitespace, leading zeros, fractions and exponents.

One host job remains reserved through teardown acknowledgement, including overlapping finish,
cancelled upload, provider startup and late host acknowledgement races. Cancelling cannot release
an older request that later dispatches another provider or arms another watchdog. Independent
compiler/toolkit/policy/frame/component identities MUST remain bound through observation.

Source, diagnostics, availability failures and identities MUST be rendered as inert text. Source
is edited as textarea text; diagnostic location selection maps exact UTF-8 boundaries to the
editor's Unicode units. Accessibility receipts must cover keyboard-only source/example/export/input
operation, labels/descriptions, invalid-input focus and messages, visible focus, live status,
contrast and actual diagnostic navigation. DOM doubles alone do not establish these outcomes.

Published documentation MUST retain the same supported profiles, versions, limits and pending
states as the tested artifacts. Runnable examples require actual nonzero exercise of their exact
source/profile/commands; hypothetical launcher commands or nonexistent release URLs are forbidden.
Documentation export must preserve the recorded source and exported-document identities and
make durable evidence reachable without treating expiring CI artifacts as permanent publication.

## 7. Acceptance decision

The final record MUST enumerate all required registered gates and platform rows with exact
receipts. A final pass requires every required row, independently authenticated immutable
artifacts, complete hosted verification and consistent exported support documentation. Failed,
not-run, skipped or unsupported rows cannot be silently removed to obtain closure.

Historical setup/extension receipts remain valid only for their recorded original claims.
Transport/schema/unit successes remain subordinate evidence. Missing final compiler/browser/OS
acceptance, accepted browser policy, supported launcher, protected outer publication or durable
receipts keeps this integration under verification. No public activation or M6 completion is
claimed by this specification.
