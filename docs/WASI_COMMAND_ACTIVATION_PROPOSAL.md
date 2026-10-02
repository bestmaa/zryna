# WASI command activation: prerequisite decision proposal

Status: **proposed, unaccepted**. This is a review input for
[#400](https://github.com/zryna/zryna/issues/400), not an executable profile, CLI contract,
manifest version, or support claim. It changes none of the accepted WIT sources, registry,
schemas, or existing public M1–M3 behavior.
The [bounded H1 contract draft](WASI_COMMAND_H1_CONTRACT_DRAFT.md) develops the recommended
choice with selected pinned-interface host-trap transport A. That selection does not accept
the remaining source, conversion or public runtime contract. Its one-run grant/manifest
decision supersedes the value-commitment question below; durable secret-value attestation is
separate optional scope.

## Established boundary

The [#167 capability contract](../spec/wit/CAPABILITY_PROFILES_V1.md) pins
`zryna:capability-profiles/command@0.1.0`, WASI `0.2.12`, five eligible capability categories,
and per-instance ceilings. Eligibility is not a host grant. An omitted capability is denied;
host-owned values must be explicit; callbacks that always reject do not demonstrate a grant.
The [#357 composition contract](../spec/language/CROSS_TARGET_PROFILES_V1.md) requires verified
source requirements, root approval, exact host grants, and a checked intersection before
instantiation. Its current implementation is a private fixed graph with no grant dispatch.

The [private command self-check](WASI_COMMAND_SELF_CHECK_V1.md) compiles verified `I32V1`
source and compares one export's result with an embedded expected `i32`. It has an empty
request/grant set, a denied host linker, and a sealed `result<_, _>` run export. That is a useful
pure execution proof, but it is not a general command result or a granted host operation.
The [public `component` build](CLI.md) is bound to the empty browser world and has no host run.

The [#359 adapter contract](../spec/interop/JS_WASM_ADAPTERS_V1.md) and its
[conformance plan](../spec/interop/JS_WASM_ADAPTER_CONFORMANCE_V1.md) keep effectful WIT adapters,
the WASI consumer proof, and public activation separate. In the
[#358 library contract](../spec/libraries/MINIMAL_CORE_HOST_V0.md), F1 value conversion and
cleanup and F2 concrete host outcomes must be accepted before an H1–H5 host operation is
implemented. These missing decisions block a nonempty public #400 grant.

## Smallest proposed useful slice

| Candidate | Additional decision burden | Recommendation |
| --- | --- | --- |
| Pure command with empty grants | Needs reviewed public run/result and manifest semantics, but exercises no grant | Retain as a conformance stage; it does not complete #400's nonempty-grant goal |
| H1 command environment lookup | Needs public String conversion, a distinct found/missing outcome, explicit values, and one WASI interface; no descriptors, sockets, entropy, or timestamp carrier | **Propose first** after F1/F2 acceptance |
| H2 filesystem read | Adds preopen identity, path containment, descriptor lifetime, and filesystem race/cleanup proof | Defer |
| H5 secure randomness | Adds owned `Vec<i32>`, entropy provenance, and cumulative byte accounting | Defer |
| H4 clock or command networking | Clock needs an accepted unsigned 64-bit carrier (F3); sockets need endpoint/DNS/operation lifetime and do not supply H3 HTTP | Defer |

H1 in [#358](../spec/libraries/MINIMAL_CORE_HOST_V0.md) proposes an authorized UTF-8 key of
1–64 bytes and a found String of 0–1024 bytes or a distinct missing result. It forbids source
enumeration and ambient process access. This selects an existing candidate for review; the
source spelling, public result type, and concrete ABI remain **unaccepted**.

## Decisions required before implementation

1. **Compiler and language owners — source authority.** Accept one exact H1 declaration and
   call form, with a verified `environment` requirement on its exact
   `wasi:cli/environment@0.2.12` interface. Decide the source result form for found versus
   missing and the closed F2 error outcomes. Source verification must reject undeclared or
   incompatible calls before sealing IR, including calls hidden in dependencies. An effect
   declaration alone cannot authorize arbitrary imports. The root must explicitly approve the
   transitive request. No general FFI, host import syntax, public owned ABI, or extra source
   operation is implied.
2. **WIT adapter and runtime owners — F1/F2 mapping.** Admit a checked UTF-8 copy from the
   explicit host value into an independently owned language String, with exact allocator,
   canonical lifting, post-return, normal cleanup, and partial-failure rules. Keep key/input
   owners live through the call; transfer a complete result once; reclaim temporaries and
   initialized prefixes on every failure. A malformed canonical value or failed cleanup is a
   fatal interface failure, not a missing key. Specify operation-specific outcomes before
   encoding; no generic `Result`, `Option`, internal M3 layout, raw host error text, or exit
   status may stand in for the accepted source result.
3. **Driver and host owners — grant payload and mediation.** Define a deterministic CLI/driver
   request grammar that names the exact command world and an explicit environment entry map.
   Omission selects an empty request and grant set. Reject unknown, duplicate, conflicting,
   unsupported, noncanonical, or over-limit entries before creating an engine/store. The host
   must construct the guest-visible map solely from those supplied bytes, never inherit
   `process.env` or an operating-system environment. Count UTF-8 key plus value bytes against
   the command ceiling of 128 entries and 65,536 bytes; enforce narrower H1 key/value bounds
   and any accepted per-call limits without truncation. Recheck grant identity and remaining
   quota at every call. A revoked grant yields the accepted permission-denied outcome before
   reading a value. All other categories retain zero effective grant until separately accepted.
4. **WIT adapter owner — whole-interface behavior.** The pinned
   `wasi:cli/environment@0.2.12` interface also contains `get-arguments` and `initial-cwd`.
   Review whether an environment grant returns exactly the explicit entry map from
   `get-environment`, an empty argument list, and no initial cwd, and prove that generated
   source can reach only admitted H1 behavior. The source-level no-enumeration promise cannot
   be inferred from WIT alone. Reject any component whose audited topology can call a broader
   operation or import an undeclared interface. The other command-world imports stay bound to
   denying callbacks; interface presence conveys no permission.
5. **CLI, manifest, and security owners — public observation.** Decide whether public command
   execution uses a reviewed version of the private `run` comparison or a different verified
   command entry contract, including how found/missing/errors map to the pinned
   `wasi:cli/run@0.2.12` `result<_, _>` and the CLI's result observation. The private embedded
   expected value must not silently become a public output ABI. Allocate a distinct create-only
   manifest version/name. Bind exact WIT world and dependency digests, source/verified program,
   requested and effective grants, limits, component identity, run result, denial and cleanup
   outcome. The bounded H1 draft records presence and byte count, keeps the value only in the
   captured one-run input, and makes no durable exact-value authenticity claim. An unkeyed hash
   of a low-entropy secret permits offline guessing. Record no ambient values. Preserve old
   manifest versions and the #399 browser bundle independently.

These are owner decisions, not names or field shapes already approved by #400. The first
implementation should stay confined to the accepted H1 subset; registry eligibility for
filesystem, network, clock, or randomness does not make those operations available.

## Proposed conformance before a support claim

- Positive: an empty-grant pure command and one H1 command with only explicit entries; the
  latter distinguishes found from missing and binds the same sealed source, world, grant, and
  result in its manifest. Repeated runs with the same explicit entries see the same map.
- Input rejection before instantiation: omitted required grant, duplicate/conflicting keys,
  unknown capability/interface/world/version, malformed UTF-8, overlong H1 key/value,
  exact registry limit then first extra entry/byte, stale source or grant binding, and a
  component with an undeclared or broadened import.
- Runtime denial: filesystem, clock, randomness, socket/network and process probes remain
  denied under an environment-only grant. A revoked or exhausted environment grant performs
  no read and reports its accepted outcome. Test the entire denied grant matrix for empty grants.
- Resource and lifecycle: independent malformed component/value probes, exact/first-extra
  allocation and quota boundaries, fatal canonical-lifting and post-return cleanup,
  store invalidation after success/error/trap, fresh recovery, and no inherited descriptors,
  environment entries, or process authority.
- Integration: fixed command examples, artifact/manifest identity checks, WIT contract and
  documentation checks, focused backend/driver/CLI tests, then the repository-required
  Linux and Windows gates on the exact reviewed revision. Record executed counts and actual
  host results; a source fixture or test listing alone is not execution evidence.

Until those decisions and tests are accepted, #400 remains open and the public WASI command
profile remains unavailable.
