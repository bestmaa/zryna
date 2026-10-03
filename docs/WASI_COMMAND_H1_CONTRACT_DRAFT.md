# Proposed bounded WASI command H1 contract

Status: **draft for review, unaccepted**. This document proposes one concrete resolution of
[#400](https://github.com/zryna/zryna/issues/400). It activates nothing, assigns no current
CLI selector or ABI, and does not revise the pinned
[#167 WIT registry](../spec/wit/CAPABILITY_PROFILES_V1.md). Its source and host decisions must
be accepted before implementation. The earlier [readiness assessment](WASI_COMMAND_ACTIVATION_PROPOSAL.md)
records the alternatives.

The selected denial transport is **A: retain the pinned WASI host trap**. This resolves only
denial transport. The source gate, F1 allocation/conversion, grant capture, run mapping and
manifest below remain one complete review candidate; selection of A does not accept them or
establish execution evidence. The matching narrow #357/#358 reconciliation already exists.

## Fixed inputs and proposed compatibility boundary

The command world is exactly `zryna:capability-profiles/command@0.1.0`; its WASI interfaces are
exactly `0.2.12`. The [#357 profile contract](../spec/language/CROSS_TARGET_PROFILES_V1.md)
keeps language admission, output target, WIT world, verified request, and host grant separate.
The [private self-check](WASI_COMMAND_SELF_CHECK_V1.md) binds empty grants and compares an
embedded `i32` expectation. Its bridge, zero-memory budget, and policy are not this proposal's
public interface. The existing browser `component` build and its manifest remain separate.
The proposed WIT input is the complete 34-file set returned by
`zryna_backend_webassembly::pinned_wit_sources()`: the local
`spec/wit/capability-profiles-v1/worlds.wit` plus all 33 pinned WASI `0.2.12` sources, across
the exact `wasi:cli`, `clocks`, `filesystem`, `http`, `io`, `random`, and `sockets` packages.
The backend's `wit_world_audit/pins.rs` owns each logical path and SHA-256; the fresh
`audit_pinned_wit_worlds` result must authenticate all 34 bytes/paths and the eight resolved
packages before command component construction. The selected command world has 13 explicit
imports, 16 resolved imports including `wasi:io`, and only `wasi:cli/run@0.2.12` exported.
An audit of `worlds.wit` alone is insufficient. No runtime path opens a substitute WIT file.

**Proposed version identities:** a new `command-h1-v1` language gate, an explicit
`wasi-command` target, `zryna.wasi-command-request.v1` for the private grant file, and
`zryna.command-h1.host.v1` for the exact host policy, plus
`zryna.wasi-command-manifest.v1` in a distinct
`zryna-wasi-command-manifest-v1.json` create-only bundle. These spellings and the CLI grammar
below are review candidates, not supported arguments. Do not reinterpret M1–M3, the browser
component manifest, the #167 request schema, or historical registry bytes. A change to source
outcomes, import set, grant interpretation, or result mapping requires a reviewed new command
profile/manifest version; a change to pinned WIT imports requires its own new WIT identity.

## One admitted source operation and entry

The proposed `CommandH1V1` gate uses the exact protocol-v4 source grammar, spans, and budgets
from [the v4 syntax contract](SYNTAX_PROTOCOL_V4.md) and the accepted pure
[DataOwnershipV1](../spec/language/DATA_OWNERSHIP_V1.md) evaluation, ownership, and scalar
entry rules. It adds one semantic intrinsic represented by v4's existing ordinary call and
string-literal syntax: `environmentLookup("MODE")`. Its sole argument must be one unescaped
source-faithful UTF-8 literal of 1–64 bytes; no computed String or additional arguments are
admitted. The name is compiler-reserved in this gate, never imported from a library or shadowed
by a declaration. The provider remains syntax-only. The semantic lowerer resolves this one
name, seals its literal key and exact `environment` /
`wasi:cli/environment@0.2.12` requirement in a distinct verified effect operation, and
the mandatory verifier rejects forged or missing effects. This is a new verifier authority:
an existing DataOwnershipV1 program cannot be relabeled as `CommandH1V1`, and the new call is
unavailable in existing profiles or targets. No dynamic key, foreign call, environment
enumeration, or dependency-supplied effect is admitted. The first gate accepts one source file
and one H1 call site; it rejects package/module dependencies rather than treating them as
pure. A later dependency route must perform #357's complete transitive check.

`EnvLookupV1` is a dedicated closed source outcome, not generic `Option` or `Result` from
[#415](https://github.com/zryna/zryna/issues/415). Proposed ordinary variants are
`Found(String)` and `Missing`. `Found` owns one validated 0–1024-byte UTF-8 String;
`Missing` owns none. The tag follows the existing closed-enum zero-based `u32` discriminant
rule: `Found = 0`, `Missing = 1`; only the active payload can be read or dropped. The
result must be consumed by an exhaustive match inside the command and cannot cross the public
entry boundary. Its target layout stays sealed behind the accepted language/layout verifier;
these tags do not publish the private String record or a host ABI. This is a new source gate,
not an inference from current internal M3 String or enum support.
An unused helper's match does not consume the lookup's actual owned result. Forwarding that
result into a private helper that exhaustively matches it is allowed. The command verifier checks
the existing sealed ownership state at successful cleanup; failure/trap cleanup remains available
before matching, and ordinary M3 ownership rules are unchanged.
The built-in name `EnvLookupV1` is reserved, not a user-defined enum. The existing v4 match
expression shape can name its two closed arms as `"EnvLookupV1.Found"` and
`"EnvLookupV1.Missing"`; the new semantic and IR verifiers must authenticate the arm
names, active owned payload and reverse cleanup. Other v4 match syntax remains unchanged.
The proposed source example is intentionally unavailable to the current compiler:

```ts
export function main(): bool {
  const result: EnvLookupV1 = environmentLookup("MODE");
  return match(result, {
    "EnvLookupV1.Found": (value) => true,
    "EnvLookupV1.Missing": () => false,
  });
}
```

The unused `value` still has one owned cleanup obligation before returning. A separate
adapter-level fixture must compare its exact UTF-8 bytes; this source program only proves
the found/missing branch and public WIT result mapping.

The proposed sole entry is `main(): bool`, with no parameters and no public owned value.
The generated bridge validates the exact scalar carrier and maps `true` to WIT `run: ok(())`
and `false` to `run: err(())`. The only public command result is the pinned
`wasi:cli/run@0.2.12` `result<(), ()>`; the CLI and manifest may report that typed
success/error, never a raw `bool` or `i32`. Process exit, stdout, and the private self-check's
expected `i32` are not result channels. An unexpected scalar carrier, unhandled trap, or denied
host import is a separate failed execution observation, not `Missing` or an ordinary `err`.

**Selected denial transport.** The pinned `get-environment` returns only a list and cannot return a
typed permission error. If a grant is revoked at a mediated call, the host must perform no
read, record the denial, trap, invalidate the instance, and report a command-boundary denial.
Returning an empty list would falsely turn denial into `Missing`. The call cannot return a
typed WIT `err` after that trap either: `run` produced no result. Record `runReturn: absent`,
`execution: host-denial`, and the exact denied interface/operation in a separate host-origin
manifest observation. Never report a successful `run: err(())` for that event.

The selected A rule retains `Found/Missing` in source and makes revocation a fatal instance
denial with no `run` return. It uses the narrow H1-specific
[#358 E3 transport](../spec/libraries/MINIMAL_CORE_HOST_V0.md) and #357 revocation
reconciliation: "fail at the host boundary without performing the effect" is a no-effect
trap; permission-denied is a distinct host-origin execution outcome, never a source enum
case or returned WIT `err`. This changes no pinned import, registry ceiling, or other host
operation. A future typed source denial would require a separately reviewed error-bearing
WIT identity and compatibility contract; it is outside this candidate.

## Explicit request, grant, and host behavior

Proposed CLI shape (for review only):

```text
zryna run <ENTRYPOINT> --target wasi-command --profile command-h1-v1
  --export main --node <PINNED_FRONTEND_NODE> [--grant-file <ABSOLUTE_PRIVATE_FILE>]
```

Omitting `--grant-file` means an empty request and grant set. Pure commands can run under that
set. A source with the H1 requirement and no matching grant rejects before instantiation.
`--node` serves only the existing authenticated frontend; it grants no Node process authority
to the command host. A grant file is an explicit host input, never a path to ambient host
environment data. The driver captures a bounded, regular, no-follow, owner-private file from
the stated absolute path, authenticates one immutable bounded byte snapshot, and seals that
snapshot as run authority. It never reopens or rereads a mutable pathname to select values.
A later path change cannot change this run; replacement of the sealed snapshot rejects. It
does not source values from process environment, argv, cwd, or inherited descriptors. The
caller retains ownership of the file: the driver neither modifies nor deletes it. The driver
keeps no temporary disk copy. It retains the original file and directory handles for
same-handle privacy, identity and snapshot revalidation through the run and manifest commit,
then closes them and discards the in-memory value with the sealed policy. No claim is made that
ordinary memory disposal prevents operating-system swap or same-user process inspection.

Unix capture verifies the caller owns a regular single-link file with no group/other
permissions, and opens every ancestor and the file without following links. Windows capture
uses retained no-reparse ancestor/file identities and denies write/delete sharing while
reading. A narrowly scoped Windows filesystem authority must inspect the owner and DACL
through that exact retained file handle and reject a file whose confidentiality cannot be
proved; inherited privacy or a caller assertion is insufficient. It neither repairs ACLs
nor grants the guest any filesystem authority. Identity, privacy and bounded immutable
capture are required on both systems before request admission.

The proposed request JSON has exactly one world, one environment grant and one literal key;
the value is explicitly present or absent. For a present value:

```json
{"schema":"zryna.wasi-command-request.v1","world":"zryna:capability-profiles/command@0.1.0","grant":{"capability":"environment","key":"MODE"},"input":{"present":true,"value":"on"}}
```

For an authorized missing value, `input` is exactly `{"present":false}`. This is different
from omitting the grant. The parser rejects duplicate or unknown fields, additional records,
malformed UTF-8, invalid Unicode, noncanonical types, or trailing data. JSON field order and
whitespace do not change authority. The proposed semantic encoding for binding is fixed field
order with length-prefixed UTF-8 bytes and an explicit presence bit. File size is at most 4,096 bytes, key 1–64 UTF-8
bytes, and present value 0–1,024 UTF-8 bytes. One grant/key and one value are the first-slice
ceilings. The #167 command ceiling of 128 environment entries and 65,536 total key/value
bytes remains an independent upper bound; neither may be raised. Test exact 1/first 2 first-
slice entries and exact/first-extra byte limits. Existing #167 registry tests retain their
128/129 and 65,536/65,537 checks.

The driver validates these static H1 limits against the captured file before creating a
store. Repeated reads of the admitted value do not consume a per-call environment quota.

Without a grant file, the driver constructs one canonical in-memory empty request for the
same exact command world and host policy: no approved host operation, no requested or effective
capability/interface/key, no value, and zero host quotas. The fixed fuel/deadline/memory
execution envelope still applies. No empty file, implicit host context, or default environment
entry substitutes for that request. A source that requires H1 cannot use it.

For the first gate, a pure source with a nonempty H1 grant also rejects before engine/store
creation: that grant has no matching verified operation. Thus the only admitted pairs are
pure/empty and one exact H1 requirement/one matching grant. The supplied grant record is the
root's explicit request/approval and current host input; the source requirement is derived
independently and cannot be inferred from the record. Multiple approvals or host maps do not
exist in this first one-node route.

The compiler seals the source literal and exact interface requirement independently of runtime
grant data. Before creating a store, the driver verifies the root's explicit approval,
compares that sealed literal with the key in the immutable captured grant, and intersects the
approved request, command-world ceiling, and host grant. The compiler cannot infer or approve
the host payload. Extra entries convey no authority; for this slice they reject. A conflicting
key, absent grant, substituted captured snapshot, changed source, or stale
component binding rejects before a store is created. Within a live invocation the host checks
the sealed grant and revocation state at every mediated call. This policy covers only H1:
filesystem, clock, randomness, sockets/network, process, stdio, and native FFI remain denied.

The pinned `wasi:cli/environment@0.2.12` interface contains three functions. The host's
`get-environment` returns exactly `[(key, value)]` if `present` and `[]` if absent, with the
same result on every successful call. `get-arguments` returns `[]`; `initial-cwd` returns
`none`. The compiler-generated wrapper may scan only that one explicit pair and expose only
`environment_lookup` to source. Independent component audit must prove no source path can
enumerate the list or call the other two functions. If that confinement cannot be proved, H1
needs a new reviewed WIT interface instead of a weakened no-enumeration claim. All other
resolved command-world imports receive the denied linker, including resource drops.

## F1 conversion, ownership, and runtime envelope

This slice proposes the [#359 synchronous Canonical ABI rules](../spec/interop/JS_WASM_ADAPTERS_V1.md):
one memory32, unshared memory, UTF-8, no async or memory64, with memory/realloc options tied
to the exact component instance. The current M3 WebAssembly owned backend uses one fixed
256-page (16 MiB) memory; a command component that reuses it must audit that exact memory,
set a matching 16 MiB store ceiling, and add a separately audited canonical realloc path.
The private zero-memory command audit cannot be reused as proof. Retain the command's
100,000-fuel and five-second deadline as proposed maxima, subject to exact/first-extra
execution evidence; no output stream or unbounded guest allocation is admitted.

The host owns the captured request bytes. On a successful `get-environment`, Canonical ABI
lowers the one pair into transfer storage in the receiving instance. The generated wrapper
validates lengths/ranges and UTF-8, copies the selected value into a distinct language-owned
String, and releases the transfer storage through the same instance's allocator after the
copy. `Found` takes that one language owner; exhaustive match and reverse lexical cleanup
release it exactly once. `Missing` performs no String allocation. Source key and host input
stay live until the copy commits; no linear-memory view or borrowed host value escapes.

Preallocation, multiplication, range and 1,024-byte checks precede copying. An ordinary
language allocation failure retains the existing typed trap and drops initialized language
prefixes without publishing a result. Invalid canonical data, realloc, lifting, or cleanup
failure is fatal: invalidate the store, reclaim independently held host resources, and do
not retry guest cleanup or relabel the failure as missing/denied.

### F1 memory and allocator acceptance candidate

The command-specific core uses exactly one fixed 256-page memory32 (16,777,216 bytes). It
reserves `[0, 65,536)` for null/static state, bounds the **command-only** private language
arena to `[65,536, 15,728,640)`, and reserves `[15,728,640, 16,777,216)` as a 1,048,576-byte
Canonical ABI transfer arena. All intervals are half-open. The current M3 backend instead
lets its private bump arena reach byte 16,777,216; therefore its allocator, scalar exports,
and effective capacity cannot simply be reused unchanged. The distinct `CommandH1V1`
backend/verifier must seal and independently audit the lower 15,728,640-byte private limit.
Existing M3 artifacts and verifier limits remain untouched. The store allows one 16 MiB
memory, with no growth, additional memory, shared memory, or memory64.

The command verifier and independent backend audit must reject any language allocation
or pointer-producing instruction able to enter the transfer interval.

The language path cannot produce a pointer into either canonical storage or allocator
metadata. Static state contains only bounded compiler-owned constants, result scratch and
the private transfer ledger; its exact offsets and helper instruction templates are sealed
and independently audited before runtime. At most 16 nonempty ledger entries may be live;
the seventeenth rejects before writing. This count is a command-specific bound, not an
increase to any #167 quota. Resizing counts both old and prospective new allocations until
copy and release commit. Retained padding and holes count against the arena cursor until
all live entries drain; a release cannot prematurely recover untracked arena capacity.

Only the command component's audited internal core exposes this memory and a canonical
`realloc(oldPointer:i32, oldByteLength:i32, alignment:i32, newByteLength:i32) -> i32`
to the component's selected canonical options. Neither is a public source or M3 scalar
export. The generated receiving bridge uses the same instance's private transfer release
operation after it copies a host result; it never uses the M3 language arena as the
Canonical ABI allocator. A checked ledger owns each nonempty transfer allocation. New
allocations start in the transfer interval; resize allocates a new aligned block, copies
`min(oldByteLength, newByteLength)` bytes, and releases the old ledger entry only after
success. Before copying or changing the ledger, validate the requested alignment and
checked new range. For a nonzero old pointer, require an exact live ledger entry and
`oldByteLength` equal to its recorded allocation size.
`oldPointer=0,oldByteLength=0` creates a new allocation when the new size is
positive and returns zero when it is zero. `newByteLength=0` releases an existing
allocation and returns zero. Any other nonzero old pointer must name
an exact live ledger entry; `oldPointer=0` with nonzero old length rejects. The allocator
accepts only alignment 1 or 4 for this audited result shape. Release requires the exact
live pointer and size; duplicate, foreign, overlapping, misaligned, out-of-range, and
wrapped ranges trap. Zero-length
strings use `(0,0)` and create no owned buffer. The transfer cursor resets to 15,728,640
only when every transfer entry for the callback has been released. An arena reset alone
is not a per-buffer release.

The H1 bridge admits only the pinned environment result: an empty list for `Missing`, or
one `(key,value)` pair with the authorized 1–64-byte key and a 0–1,024-byte UTF-8 value.
It checks list and string lengths, canonical offsets/alignment, source key identity, and
all ranges before copying into a distinct language String. A single transfer allocation
is capped at 4,096 bytes and total live plus unreclaimed transfer bytes at 1,048,576;
exceeding either traps before writing. The 4,096-byte bound accommodates the pinned
UTF-8 Canonical ABI's at-most-four-times temporary string allocation for this 1,024-byte
value. Only alignment 1 for string bytes and alignment 4 for list/tuple records is needed;
the independent component audit rejects other result shapes. The command-only language
allocation path must return a checked status to the bridge before emitting an E1 trap.
The current M3 helper already reports allocation failure through checked status, but it
has no command transfer ledger or release path. The command bridge must drain outstanding
transfer buffers before turning that status into E1. It releases each transfer entry exactly once before a `Found`
value becomes visible. After a failed language allocation status, it releases those
entries before emitting the existing E1 trap and publishes no `Found`. A trap during
canonical allocation, lowering, lifting, or release is fatal:
discard the store and independently tracked host state without a guest
cleanup retry. The immutable host snapshot remains outside guest memory until the mediated
call. This candidate needs an independent implementation and audit before F1 is accepted.

Acceptance fixtures must distinguish bytes 15,728,639/15,728,640 at the private boundary
and 16,777,215/16,777,216 at the transfer boundary; test empty, present-empty, multibyte,
and 1,024/1,025-byte values; first-extra list item and key byte; repeated calls with
complete ledger drain; resize-copy-release and forged/double release; wrong old allocation
length before copy; canonical allocation failure and subsequent fresh-instance recovery;
and absence of a borrowed guest-memory view after the callback. These are future tests,
not executed evidence.

## Manifest and one-run authority

The grant is permission to use the exact environment interface for one authorized literal
key under bounded quotas. The supplied present/missing value is host input data, not another
capability. This follows #167's separation of capability eligibility from host-owned values
and #357's binding of verified requirements and current grants. The driver captures the file
once into an immutable private snapshot, validates it, and seals that snapshot with the
prepared run. Only those captured bytes supply `get-environment`; no path reread, process
environment, or inherited host descriptor can change the value during execution. A changed
runtime policy object or grant identity rejects before a store or mediated effect.

The prepared run must retain and revalidate **all** of these opaque authorities together:

1. The authenticated source map's exact relative path and UTF-8 bytes, its source identity,
   the `CommandH1V1` verifier-sealed program and program fingerprint, its verified effect
   requirement and `main(): bool` scalar ABI. A source digest alone cannot substitute for the
   verified program or profile seal.
2. The #357 composition result for the one-node graph: selected language/profile and
   `WitCommand` row, exact command world, source/program authority, empty or exact H1
   requirement, root approval, policy version, resource ceilings, and composition binding.
   The current private empty-only composition result does not authorize H1; the new result
   needs the same independent derivation and stale-authority rejection.
3. All 34 authenticated pinned WIT sources and the fresh resolved audit of eight packages,
   three worlds, exact command imports and run export; the sealed component's retained core,
   final bytes, independently audited topology, and both artifact identities. Neither a WIT
   label nor a component digest alone grants a host operation.
4. The driver-validated requested and effective grant set, exact literal key, approved
   interface, fixed quotas and runtime envelope, plus the private captured present/value
   bytes and current revocation state. The host receives that sealed policy rather than a
   new filesystem read.

Before engine/store creation, revalidate the source/program/profile, composition, full WIT
closure/component, and grant intersection. At each environment callback, check the retained
policy identity, literal key, validated static limits and revocation state before returning
the captured value.
After execution, derive the manifest only from those retained observations and the actual
run/denial/teardown record. A create-only bundle commits the complete manifest or nothing.

The proposed `zryna.wasi-command-manifest.v1` execution record contains:

| Field group | Recorded value |
| --- | --- |
| Source and program | Source path/identity; `CommandH1V1` profile and verifier revision; sealed program fingerprint; verified H1 requirement; scalar `main` identity |
| Composition | One-node graph/selection identity, root approval, exact `WitCommand` row, policy version and independently derived composition binding |
| WIT and component | Command world/WASI version; all eight resolved package IDs and canonical import/export sets; complete 34-file closure identity; retained core and audited component identities |
| Grants and limits | Canonical requested and effective capability/interface/key sets (both empty for omission), #167 ceiling and narrower one-key quota, fuel, deadline and memory limits |
| Host input summary | `none`, `missing`, or `present`, with bounded UTF-8 byte count; no value, input path or unkeyed value hash |
| Execution | `run-returned` with `runReturn: ok` or `err`; `host-denial` with `runReturn: absent`; or `runtime-trap` with `runReturn: absent`, `trapCategory`, and conditional `trapIdentity` |
| Denial and teardown | First denied interface/operation, `permission-denied` reason and host policy revision only for authentic host denial; otherwise no denial fields; `confirmed` or `unconfirmed` teardown, never success inferred from a failed cleanup |

The sealed host denial state, set before its deliberate trap, is the only source of a
`host-denial` record. On `runtime-trap`, `trapCategory` is exactly `controlled-language`,
`interface-violation`, or `host-process-failure`. `trapIdentity` is the exact verified
`zryna.trap.*` identity for a controlled language trap, or a fixed audited failure
identity for an interface violation. An unrelated raw Wasm trap or host/process exception
is `host-process-failure`, with `trapIdentity` absent and no arbitrary exception text.
All three categories have no denial fields and no WIT run return.

The candidate fixed interface identity is `zryna.command.interface-violation.v1`.
It applies only to an exact `unreachable` location retained from the complete independent
storage-core audit, translated into the authenticated storage-first component. Runtime
classification also checks the originating compiled component and storage core. An
exception string, trap opcode alone or unauthenticated byte offset cannot assign it.
This identity includes fatal canonical allocation, live/count/byte-budget and transfer
contract guard failures. They consume the store, publish no run return or denial fields,
and do not become a language E1 identity. No guest cleanup retry follows such a failure.

For a compact complete WIT closure identity, the candidate `witClosureDigest` is SHA-256 over
all 34 audited files sorted by logical path. For each file, hash an unsigned 64-bit
little-endian path-byte length, its UTF-8 path bytes, an unsigned 64-bit little-endian
source-byte length, and the exact pinned source bytes. Record count `34` and that digest;
the retained authenticated audit, not the manifest digest, is execution authority. The
manifest's source, program, composition and artifact identities are likewise observations
of retained sealed authorities, not permission reconstructed from JSON.

The candidate program fingerprint is a stable content binding, not a serialization of
raw IR. Compute SHA-256 over the following closed byte sequence, in this order:

1. ASCII `zryna.command-program-binding.v1` followed by one NUL byte.
2. The UTF-8 profile `command-h1-v1`, prefixed by its unsigned 32-bit little-endian
   byte length.
3. Six raw 32-byte digests: source UTF-8 SHA-256, sealed language core SHA-256,
   sealed storage core SHA-256, verified Linear32 layout fingerprint, verified Linux
   x86-64 layout fingerprint, and the authenticated command WIT binding below.
4. Seven unsigned 32-bit little-endian integers: `256`, `0`, `65536`, `65536`,
   `15728640`, `15728640`, `16777216`. These encode memory pages and the static,
   L and C half-open bounds.
5. One optional-key tag: `0` for no environment requirement; otherwise `1`, followed
   by the unsigned 32-bit little-endian byte length and exact verified key UTF-8 bytes.

The command WIT binding is SHA-256 over ASCII `zryna.command-wit-binding.v1` followed
by NUL; the length-prefixed world identity; the 32-bit little-endian file count; then,
in logical-path order, each length-prefixed path and raw 32-byte SHA-256 of its exact
pinned source bytes. Finish with the explicit import, resolved import and export name
lists, in that order. Each list has a 32-bit little-endian count and each name has a
32-bit little-endian UTF-8 byte length. Use the canonical order from the retained
authenticated world audit. This binding is distinct from `witClosureDigest` above.

The source, core, component and WIT observations remain separate manifest fields.
The program binding excludes request values, request paths, unkeyed value hashes and
process-local issuer IDs. Equal content from fresh verification may have the same
fingerprint, while execution still requires the retained opaque program issuer,
source-map binding, both layout witnesses and paired ownership runtime ABI. Neither
fingerprint can reconstruct those authorities from a manifest.

Driver execution must obtain its opaque command semantic result through authenticated
protocol-v4 source admission and real semantic lowering. The independent IR verifier
checks structural, type, ownership, entry, effect and match obligations; it is not a
general proof that arbitrary supplied IR implements every source expression. The content
fingerprint likewise makes no source semantic attestation claim.

This durable JSON is an **execution record** of the grant set, limits, result and denial.
It neither contains nor proves exact secret value bytes after the private snapshot is
discarded, and it is not a third-party attestation or replay receipt. Exact-value
commitments, caller-supplied verification keys and HMAC/key storage are separate optional
scope, not a #400 baseline gate. A reader may verify documented identities against the
matching compiler artifacts and pins, but must not treat an editable manifest alone as an
authenticated proof of the value that was supplied.

## Complete implementation ownership and review boundary

The syntax provider continues to emit ordinary authenticated v4 call/type/match records.
Only the new semantic gate may recognize the reserved intrinsic/type. The IR authority must
seal a distinct command program with an exact environment operation, literal source span,
owned result layout and cleanup proof. It must independently reject missing, duplicated,
forged or non-source-faithful operations and wrong result arms; an M3 program or externally
supplied effect list is insufficient. Existing M3 constructors, verifier acceptance, runtime
ABI identities, allocator capacity and scalar manifests remain unchanged.

The backend accepts only that command authority. It seals a command-only language allocator
with a checked allocation status, the transfer ledger and exact environment receiving bridge,
then audits the complete emitted core instructions and component topology independently of
the producer. The full command remains capped at the existing 1 MiB component envelope;
exceeding it rejects rather than widening the private self-check limit. Source code reaches
the literal-key wrapper only, never a raw memory/realloc symbol or a general host import.

The driver owns architecture-first request validation, source/provider capture, independent
one-node composition, immutable grant capture, pre-store revalidation, exact host callbacks,
fresh store/deadline ownership and create-only publication. The CLI parses the explicit route
and renders the sealed run observation. A build-only command artifact cannot carry execution
authority and is outside this initial run-only CLI. Every unsupported profile/target/grant
combination rejects before backend dispatch or runtime construction.

Before implementation acceptance, review this document together with the existing #357/#358
H1 reconciliation and #167 first-extra-byte test. Acceptance would authorize only this bounded
one-file H1 command and its necessary pure/negative controls. It does not authorize H2-H5,
servers, package dependencies, native FFI, public String/layout exports or general Result/Option.
No runtime/public support state changes until the full acceptance matrix below is executed
and the required exact-revision Linux/Windows gates and code review are complete.

## Fixed design and later execution fixtures

| Case | Input | Required decision or observation |
| --- | --- | --- |
| pure | No grant file; source `main` returns true without H1 | Empty effective set; WIT `ok(())`; zero host entries |
| found | Literal `MODE`; exact file above; source matches `Found` | One environment requirement/grant; copied owned value `on`; WIT `ok(())` for a true branch |
| missing | Same grant key with `present:false` | `Missing` (distinct from `Found("")`); WIT outcome follows source bool |
| empty value | `present:true,value:""` | `Found("")`; one owned empty String; not `Missing` |
| omitted grant | H1 source, no file | Reject before engine/store, no callback |
| changed authority | Substitute captured policy, source, verified program/profile, composition, any of 34 WIT pins, or component; separately replace pathname after capture | Reject sealed substitution before instantiation/publication; pathname replacement cannot change the captured run |
| malformed | Duplicate/conflicting key, unknown field/capability/world, bad UTF-8, wrong type, extra record | Reject in deterministic input phase |
| bounds | One/two entries; 64/65-byte key; 1,024/1,025-byte value; 4,096/4,097-byte file; #167 128/129 entries | Exact accepted where applicable, first extra rejected at owning layer |
| revoked | Revoke after sealing but before host call | No value read; trap; absent `run` return and distinct host-denial record; no false `Missing` or typed WIT `err` |
| static limit | 1,025-byte captured value or second key before instantiation | Reject the input before a store; repeated reads of one admitted value do not deplete a runtime quota |
| fatal categories | Controlled E1 language trap, forged canonical pointer, unrelated raw Wasm trap or host exception | Absent `run` return; correct category and conditional identity; no denial fields without a host denial |
| manifest | Empty grant, granted-missing and present-empty value; changed requirement, world, limit, result or denial | Distinct grant/input summaries and complete retained identities; manifest never exposes or claims proof of value bytes |
| denied probes | Filesystem, clock, randomness, sockets/network under empty or H1 grant; process import attempt | Deterministic host denial for admitted-world imports; process import rejected by world/topology audit, with no ambient effect |
| malformed component | Changed import/function type, canonical option, realloc, memory, run result, or excess bytes | Reject before instantiation; next valid request recovers |
| cleanup | Found, missing, source `err`, denied call, canonical failure, fuel/deadline trap | Exact ownership ledger, store invalidation, joined watchdog, fresh recovery |

These rows are design fixtures, not tests that have run. Implementation acceptance also needs
the WIT contract, source/IR and independent component audits, focused driver/CLI tests,
manifest inventory/execution-record checks, fixed examples, and required Linux and Windows gates on
the reviewed revision. #400 stays open and unsupported until those proofs and the required
repository contract decisions above are accepted.
