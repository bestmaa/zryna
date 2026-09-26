# Proposed bounded WASI command H1 contract

Status: **draft for review, unaccepted**. This document proposes one concrete resolution of
[#400](https://github.com/zryna/zryna/issues/400). It activates nothing, assigns no current
CLI selector or ABI, and does not revise the pinned
[#167 WIT registry](../spec/wit/CAPABILITY_PROFILES_V1.md). Its source and host decisions must
be accepted before implementation. The earlier [readiness assessment](WASI_COMMAND_ACTIVATION_PROPOSAL.md)
records the alternatives.

## Fixed inputs and proposed compatibility boundary

The command world is exactly `zryna:capability-profiles/command@0.1.0`; its WASI interfaces are
exactly `0.2.12`. The [#357 profile contract](../spec/language/CROSS_TARGET_PROFILES_V1.md)
keeps language admission, output target, WIT world, verified request, and host grant separate.
The [private self-check](WASI_COMMAND_SELF_CHECK_V1.md) binds empty grants and compares an
embedded `i32` expectation. Its bridge, zero-memory budget, and policy are not this proposal's
public interface. The existing browser `component` build and its manifest remain separate.

**Proposed version identities:** a new `command-h1-v1` language gate, an explicit
`wasi-command` target, `zryna.wasi-command-request.v1` for the private grant file, and
`zryna.wasi-command-manifest.v1` in a distinct
`zryna-wasi-command-manifest-v1.json` create-only bundle. These spellings and the CLI grammar
below are review candidates, not supported arguments. Do not reinterpret M1–M3, the browser
component manifest, the #167 request schema, or historical registry bytes. A change to source
outcomes, import set, grant interpretation, or result mapping requires a reviewed new command
profile/manifest version; a change to pinned WIT imports requires its own new WIT identity.

## One admitted source operation and entry

The candidate source gate admits one compiler-known synchronous call with the conceptual
signature `environment_lookup(<literal key>) -> EnvLookupV1`. The key is one immutable UTF-8
string literal of 1–64 bytes. It is checked during source admission and sealed with the exact
`environment` / `wasi:cli/environment@0.2.12` requirement in verified IR. No dynamic key,
foreign call, environment enumeration, library-provided declaration, or dependency-supplied
effect is admitted. The first gate accepts one source instance and one H1 call site; it rejects
package/module dependencies rather than treating them as pure. A later dependency route must
perform #357's complete transitive check.

`EnvLookupV1` is a dedicated closed source outcome, not generic `Option` or `Result` from
[#415](https://github.com/zryna/zryna/issues/415). Proposed ordinary variants are
`Found(String)` and `Missing`. `Found` owns one validated 0–1024-byte UTF-8 String;
`Missing` owns none. The tag follows the existing closed-enum zero-based `u32` discriminant
rule: `Found = 0`, `Missing = 1`; only the active payload can be read or dropped. The
result must be consumed by an exhaustive match inside the command and cannot cross the public
entry boundary. Its target layout stays sealed behind the accepted language/layout verifier;
these tags do not publish the private String record or a host ABI. This is a new source gate,
not an inference from current internal M3 String or enum support.

The proposed sole entry is `main(): bool`, with no parameters and no public owned value.
The generated bridge validates the exact scalar carrier and maps `true` to WIT `run: ok(())`
and `false` to `run: err(())`. The only public command result is the pinned
`wasi:cli/run@0.2.12` `result<(), ()>`; the CLI and manifest may report that typed
success/error, never a raw `bool` or `i32`. Process exit, stdout, and the private self-check's
expected `i32` are not result channels. An unexpected scalar carrier, unhandled trap, or denied
host import is a separate failed execution observation, not `Missing` or an ordinary `err`.

**Denial transport gap.** The pinned `get-environment` returns only a list and cannot return a
typed permission error. If a grant is revoked at a mediated call, the host must perform no
read, record the denial, trap, invalidate the instance, and report a command-boundary denial.
Returning an empty list would falsely turn denial into `Missing`. The call cannot return a
typed WIT `err` after that trap either: `run` produced no result. Record `runReturn: absent`,
`execution: host-denial`, and the exact denied interface/operation in a separate host-origin
manifest observation. Never report a successful `run: err(())` for that event.

There are two coherent decisions. **A (recommended for the smallest pinned-world slice):**
retain `Found/Missing` in source and make revocation a fatal instance denial with no `run`
return. This needs an explicit, reviewed H1-specific reconciliation of
[#358's provisional E3 permission-denied outcome](../spec/libraries/MINIMAL_CORE_HOST_V0.md)
and #357's revocation rule; it must not silently waive either. **B:** add a reviewed
error-bearing host interface so a source-level `Denied` case can be transported normally.
B needs a new WIT package/world identity, registry/request revisions, component audit, and
compatibility decision; it cannot be grafted onto `wasi:cli/environment@0.2.12` or the
existing world. If reviewers require typed source denial, B is a prerequisite and A cannot
activate #400. Neither choice is accepted by this draft.

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
the stated absolute path, retains its identity through validation/execution, and revalidates
it before instantiation. It does not source values from process environment, argv, cwd, or
inherited descriptors. The caller retains ownership of the file: the driver neither modifies
nor deletes it. The driver keeps no temporary disk copy, closes its retained handle after the
run/receipt, and discards the in-memory value with the sealed policy. No claim is made that
ordinary memory disposal prevents operating-system swap or same-user process inspection.

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

The compiler proves the call's literal key equals the grant key and that the root explicitly
approves the exact interface requirement. The driver intersects that approved request, the
command-world ceiling, and the captured host grant. Extra entries convey no authority; for
this slice they reject. A conflicting key, absent grant, altered file, changed source, or stale
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
not retry guest cleanup or relabel the failure as missing/denied. This proposes the F1
copy/ownership postcondition; exact allocator entrypoints and verifier rules remain subject
to review before any code is written.

## Manifest and confidential value authority

The proposed create-only manifest records schema/version; exact source, verified profile,
component, WIT world/dependency and host-policy identities; requested and effective
capability/interface/key; first-slice and registry limits; typed `run` outcome or trap;
first denial and teardown disposition. It contains no plaintext host value, raw guest scalar,
environment dump, process path, or unkeyed value hash. The exact value and presence bit remain
in a sealed per-invocation policy paired with the captured file. Revalidation before
instantiation and at each call compares that sealed policy with the source/component/request
authority and a pre-run commitment; substitution invalidates execution. Manifest publication
occurs only after a complete run and teardown record, through a create-only bundle.

For a durable exact-value binding, the **proposed** pre-run commitment is
`authorityTag = HMAC-SHA256(K, authority-domain || canonical_request || source_identity ||
component_identity || world_identity || limits)`. The sealed policy retains `authorityTag`
and is checked before instantiation and at each mediated call. After teardown, calculate
`receiptTag = HMAC-SHA256(K, result-domain || authorityTag || run_outcome || denial || cleanup)`.
The manifest records a versioned algorithm, key identifier, `authorityTag`, and `receiptTag`,
never `K` or the value. The two domains are distinct fixed versioned byte strings, and every
field has a length-prefixed canonical encoding. `K` is an independent random 256-bit host
key. It lives in owner-private compiler host state outside the bundle and source
project; it is generated from a trusted operating-system entropy source, never inherited
from the command's own randomness grant. Verification needs both `K` and the original
private request bytes; missing or rotated-away keys fail verification. Rotation retains
old keys for the declared manifest retention period or explicitly marks older receipts
unverifiable. A plain or salted hash would expose low-entropy values to offline guessing.
The host-key storage path, retention period, and cross-platform ACL proof are external
security decisions before this durable scheme can be accepted. If #400 requires a
standalone publicly verifiable exact-value manifest, this host-keyed scheme is insufficient
without an additional reviewed attestation design; that requirement remains a blocker.

## Fixed design and later execution fixtures

| Case | Input | Required decision or observation |
| --- | --- | --- |
| pure | No grant file; source `main` returns true without H1 | Empty effective set; WIT `ok(())`; zero host entries |
| found | Literal `MODE`; exact file above; source matches `Found` | One environment requirement/grant; copied owned value `on`; WIT `ok(())` for a true branch |
| missing | Same grant key with `present:false` | `Missing` (distinct from `Found("")`); WIT outcome follows source bool |
| empty value | `present:true,value:""` | `Found("")`; one owned empty String; not `Missing` |
| omitted grant | H1 source, no file | Reject before engine/store, no callback |
| changed authority | Replace source, file value/presence, WIT pin, component, or HMAC key | Reject stale binding/verification before instantiation or publication |
| malformed | Duplicate/conflicting key, unknown field/capability/world, bad UTF-8, wrong type, extra record | Reject in deterministic input phase |
| bounds | One/two entries; 1,024/1,025-byte value; 4,096/4,097-byte file; #167 128/129 entries | Exact accepted where applicable, first extra rejected at owning layer |
| revoked | Revoke after sealing but before host call | No value read; trap; absent `run` return and distinct host-denial record; no false `Missing` or typed WIT `err` |
| denied probes | Filesystem, clock, randomness, sockets/network under empty or H1 grant; process import attempt | Deterministic host denial for admitted-world imports; process import rejected by world/topology audit, with no ambient effect |
| malformed component | Changed import/function type, canonical option, realloc, memory, run result, or excess bytes | Reject before instantiation; next valid request recovers |
| cleanup | Found, missing, source `err`, denied call, canonical failure, fuel/deadline trap | Exact ownership ledger, store invalidation, joined watchdog, fresh recovery |

These rows are design fixtures, not tests that have run. Implementation acceptance also needs
the WIT contract, source/IR and independent component audits, focused driver/CLI tests,
exact-byte manifest/receipt checks, fixed examples, and required Linux and Windows gates on
the reviewed revision. #400 stays open and unsupported until those proofs and the external
decisions above are accepted.
