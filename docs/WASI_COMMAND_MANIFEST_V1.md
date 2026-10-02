# Bounded command execution manifest v1

Status: implementation review candidate; complete acceptance and independent final review
remain pending. This record belongs only to `command-h1-v1` / `wasi-command` and does not
change scalar or browser manifest versions. The [walkthrough](WASI_COMMAND_GETTING_STARTED.md)
explains invocation and private inputs; the [H1 contract](WASI_COMMAND_H1_CONTRACT_DRAFT.md)
specifies the compiler, grant and conversion authorities.

The create-only bundle `<name>.wasi-command-run` contains
`wasi-command/<name>.wasm` and `zryna-wasi-command-manifest-v1.json`. Its schema identity is
`zryna.wasi-command-manifest.v1`. The driver constructs the record only after consuming the
actual execution, retaining the source, verified semantic result, composition, audited artifact,
original private file and policy through publication. It commits the complete bundle or nothing.

## Closed inventory

The UTF-8 JSON document is bounded to 16,384 bytes. Every object is closed: duplicates, unknown
fields, trailing data, malformed UTF-8, alternate array-shaped objects and explicit null optional
fields reject. Digests are lowercase 64-character SHA-256 values. The strict decoder verifies
the exact pinned world inventory, program-binding consistency, all fixed limits and permitted
grant/input/outcome combinations. It produces observations, never executable authority.

| Group | Required fields and interpretation |
| --- | --- |
| Top level | `schema`, `source`, `composition`, `component`, `grants`, `input`, `limits`, `execution`, `teardown` |
| `source` | `path` (normalized workspace-relative source), `sha256`, `profile` (`command-h1-v1`), `verifierRevision` (1), `programBinding`, `requirements`, `scalarEntry` |
| `source.scalarEntry` | `name` (`main`), `abiVersion` (1), `abiIndex` (0), `parameters` (empty), `result` (`bool`) |
| Each grant | Exactly `capability` (`environment`), `interface` (`wasi:cli/environment@0.2.12`), `key` (the source literal). Grant arrays are empty or contain this one exact triple |
| `composition` | `binding`, `root` (`command-source`), `language` (`CommandH1V1`), `row` (`WitCommand`), `world`, `policyVersion`, `approved`, `staticQuota` |
| `component` | `path` (`wasi-command/<name>.wasm`), `kind` (`wasi-command-component-v1`), `sha256`, `languageSha256`, `storageSha256`, `linear32Sha256`, `linuxX8664Sha256`, `worldSha256`, `witClosureDigest`, `witFileCount` (34), `world`, `wasiVersion` (`0.2.12`), `packages`, `explicitImports`, `resolvedImports`, `exports` |
| `grants` | `requested`, `effective`, `registryCeilings`, `staticQuota`, `hostPolicy` (`zryna.command-h1.host.v1`). Requested, effective, source requirements and root approval must agree |
| `input` | `kind`: `none`, `missing` or `present`; `utf8ByteCount`: actual value bytes, zero for none/missing. Present-empty remains `present` |
| `execution` | `kind`, `runReturn`, and only the conditional fields described below |
| `teardown` | `confirmed` only after Store consumption and owned deadline-worker join; otherwise `unconfirmed` |

The world is exactly `zryna:capability-profiles/command@0.1.0`; its eight resolved packages,
13 explicit and 16 resolved imports, and sole `wasi:cli/run@0.2.12` export come from the complete
pinned WIT closure. `policyVersion` is the existing composition policy identity. The
[binding byte encodings](WASI_COMMAND_H1_CONTRACT_DRAFT.md#manifest-and-one-run-authority)
separately frame source, cores, layout witnesses, world and key; the complete WIT closure digest
uses its established path/source framing. Digests do not replace retained issuing authorities.

Quota arrays contain ten unsigned integers in this order: clock subscriptions/timers,
environment entries/total bytes, filesystem preopens/descriptors, network endpoints/concurrent
operations, randomness bytes per call/instance. `registryCeilings` is exactly
`[64,64,128,65536,16,256,64,128,65536,8388608]`. Both `staticQuota` fields are all zero for
pure commands or `[0,0,1,1088,0,0,0,0,0,0]` for H1, regardless of the actual value length.

## Fixed limits

| `limits` fields | Exact values |
| --- | --- |
| `fuel`, `deadlineMillis`, `maxWasmStackBytes`, `backtraceMaxFrames` | 100000, 5000, 65536, 1 |
| `instances`, `memories`, `tables` | 2, 1, 0 |
| `memoryPages`, `memoryBytes`, `memoryGrowth`, `sharedMemory`, `memory64` | 256, 16777216, false, false, false |
| `staticStart`, `staticEnd`, `languageStart`, `languageEnd`, `canonicalStart`, `canonicalEnd` | 0, 65536, 65536, 15728640, 15728640, 16777216 |
| `maxLiveTransferEntries`, `maxTransferAllocations`, `maxTransferAllocationBytes`, `maxTransferBytes` | 16, 4096, 4096, 1048576 |
| `maxRequestBytes`, `maxKeyBytes`, `maxValueBytes`, `maxComponentBytes`, `maxManifestBytes` | 4096, 64, 1024, 1048576, 16384 |

## Actual execution observation

| `execution.kind` | `runReturn` | Conditional fields |
| --- | --- | --- |
| `run-returned` | `ok` or `err` | No denial or trap fields |
| `host-denial` | `absent` | `denial`: exactly `interface`, `operation`, `reason` (`permission-denied`), `policyRevision` (the fixed host-policy identity) |
| `runtime-trap` | `absent` | `trapCategory`; conditional `trapIdentity`; no denial field |

Production source confinement permits only the approved environment callback. Its revocation or
original-file revalidation failure seals `get-environment` denial before value access. The first
authenticated denial takes precedence over the deliberate host trap. No Missing value or WIT
`err` is fabricated for that event.

`trapCategory` is `controlled-language`, `interface-violation` or `host-process-failure`.
The first two require the exact compiled component image, audited core role, function and
instruction coordinates. Their identities are the fixed audited language identities or
`zryna.command.interface-violation.v1`. Unrelated traps, fuel/deadline exhaustion and host
exceptions use `host-process-failure` with no identity. Fatal canonical failure consumes the
Store without guest cleanup, draining or retry.

The document excludes the private value, input pathname, value hash and process-local issuer.
It records that execution's observed presence, byte count and authority metadata. Editing a
well-formed `ok` record to `err` can remain a valid observation document; parsing cannot prove
that edited outcome happened or reconstruct the consumed execution. Execute through retained
compiler/runtime authority to obtain an authentic fresh observation.
