# Restricted source playground implementation

This implementation is under verification. It does not yet provide a supported launcher,
published executable toolkit, authenticated end-to-end browser session or M6 closure evidence.
The existing [fixed-component example](../README.md) remains independently runnable.

`apps/zryna-playground-compiler` reads one closed request and calls the driver's existing scalar
source compiler, IR verifier, component audit and deterministic binding producer. Source remains
an exact UTF-8 string at `src/main.zry`; it cannot select a target, filename, package or command.
The reply contains actual compiler structured-diagnostic schema 1, including preserved TypeScript
syntax codes, and separately versioned compilation framing. LSP diagnostics v2 is unchanged.

`protocol.mjs` bounds and checks complete input/output carriers, exact source/revision identity,
diagnostic fields, Unicode scalar coordinates, canonical order and artifact byte consistency.
These checks do not establish executable provenance. `executable.mjs` additionally requires an
independently captured fixed compiler/toolkit/policy receipt, the complete child-frame digest,
the authenticated existing binding template, matching interface identity and import-free core
exports before loading any module. The executable trust chain must start with the separately
reviewed outer toolkit signature and finite installed material inventory; response-supplied
hashes and adjacent manifests are insufficient.

`toolkit-schema.mjs` binds canonical metadata to independent source/archive/nested-compiler pins,
fixed signing identity, browser materials and present gate receipts. `signature.mjs` snapshots
an independently pinned, root-owned static verifier through retained no-follow ancestor handles,
checks pinned trusted-root bytes and invokes bounded offline Sigstore verification without
disabling log checks. `toolkit.mjs` verifies the entire canonical archive and exact finite
inventory before issuing a retained capability; plain objects cannot forge that capability.
Before capability issuance, `nested-compiler.mjs` separately verifies the captured 0.2.3 envelope
with its exact release workflow/tag/source certificate claims, reuses the complete distribution
archive verifier and compares every mounted runtime/provider byte. `browser-archive.mjs` checks
all captured ZIP members against the separately pinned ordinary-file inventory. Both nested
archives, the compiler envelope/bundle and browser inventory are mandatory signed-inventory entries.
`wit-closure.mjs` verifies the canonical 34-source WIT inventory and every captured source byte,
then reconstructs the compiler's domain-separated, little-endian length-prefixed closure digest.
`resources/browser.wit` must equal the captured complete local worlds source. Its individual file
hash is not the compiler's WIT identity; capability issuance uses the reconstructed closure digest.
Actual positive signature, nested-material composition, publication and launcher replay are still pending.

`producer.mjs` assembles deterministic unsigned archive/envelope bytes from bounded captured
files and reviewed source/material/gate identities. Its returned candidate policy contains the
newly computed digests for review; it is not independent authority. It performs no tag creation,
signing, upload or installation. `publication/source.mjs` requires the exact protected annotated
toolkit tag, clean isolated source checkout, selected commit/tree and ordinary workflow blob.
`publication/capture.mjs` captures separately pinned finite build inputs through no-follow handles;
`publication/assemble.mjs` binds that capture to the source before and after compression and retains
create-only unsigned outputs, a source receipt and computed candidate policy. Its final assembly
receipt distinguishes a complete unsigned candidate from a preserved partial output directory.
`publication/output.mjs` retains every no-follow source/output ancestor and the created output
directory, writes through its descriptor and checks named reachability around every write. Symlink
aliases into source and renamed/replaced output directories reject without a pathname fallback;
failed partial output bytes remain available for review. This output writer is Linux-only.
It retains each output file handle and rechecks named identity, exact byte count and SHA-256 before
the completion receipt. That receipt records the validated candidate files at completion; later
independent trusted capture and signature verification remain mandatory before any execution.
The protected signing/publishing workflow and independently acquired verifier/root receipts remain
required before this candidate becomes a runnable release. No actual protected assembly or signature
verification is established by these source modules or their filesystem unit tests.

The internal Linux assembly entry point uses the independently authenticated pinned Node runtime:

```text
node examples/playground/restricted/publication/assemble.mjs --materials /absolute/inputs --source /absolute/source --plan build-plan.json --plan-bytes <bytes> --plan-sha256 <reviewed-sha256> --reviewed reviewed-policy.json:<bytes>:<reviewed-sha256> --output /absolute/new-candidate
```

The reviewed policy selects the plan digest, exact source commit/tree, nested compiler pins and
required gate names. It must come from separate review, never from `candidate-policy.json`.
The bounded plan selects ordinary input files by relative path, exact byte count and digest, with
finite materials and actual retained gate receipts. The invocation does not sign, publish or run
the candidate. A protected workflow must supply the exact tag context; local context fabrication
does not establish protected publication evidence.

`controller.mjs` retains one source revision and one operation. Editing or cancellation invalidates
old results. Admission waits for host teardown; teardown failure stops subsequent work. A host
watchdog must arm before creating an evaluation worker. The page's five-second timer is an
additional interruption mechanism, not a host memory or process-tree guarantee.

`watchdog.mjs` is the external Node/CDP lifecycle consumer. It admits a captured dedicated
context, arms before worker creation, closes the exact owned worker and requires a zero-worker
observation before acknowledging cleanup. Host expiry invalidates late observations while
successful teardown permits recovery. Unexpected targets or failed cleanup require termination
of the dedicated browser by its real process-tree supervisor. Mock CDP tests do not establish
that process-tree capability or actual Chrome isolation.

The Linux-only supervisor candidate uses an already delegated, empty cgroup v2 subtree with
cpu, cpuset, memory and pids controllers. It creates only owned job children; it never enables controllers or
changes system security configuration. Compiler admission uses 512 MiB memory, zero swap and
32 tasks (threads count), a quota of two CPUs and an effective two-CPU cpuset. All descendants must join before receiving source, with ten seconds for
compilation and two seconds for complete teardown. Read-only sealed file captures, mandatory
namespaces and the explicit x86-64 seccomp allowlist confine the fixed compiler/provider closure.
The bootstrap permits finite authenticated file-copy setup under larger file/descriptor limits;
the dedicated compiler reduces and reads back those limits before provider capture. `host.py`
is an incomplete receipt consumer; the helper security and independent authentication review
must finish before it can admit a supported session. Missing primitives reject. These numeric and OS claims require positive boundary/isolation
acceptance; unit tests and policy source are not that evidence. Dedicated supervised Chrome
memory/task/deadline acceptance, service authentication and durable publication are still pending.

Light checks, with the pinned Node runtime and Linux Python:

```text
node --test tests/playground-protocol.test.mjs tests/playground-controller.test.mjs tests/playground-executable.test.mjs
PYTHONDONTWRITEBYTECODE=1 python3 tests/playground-host-unit.py
PYTHONDONTWRITEBYTECODE=1 python3 tests/playground-cgroup-unit.py
PYTHONDONTWRITEBYTECODE=1 python3 tests/playground-join-unit.py
```

Synthetic framing/controller fixtures are explicitly transport tests. Full acceptance must use
the actual authenticated compiler, independent CLI receipts and the pinned real Chrome corpus,
alongside Linux/Windows canonical and M6 tooling gates on the final integration revision.

The [M6 support matrix](../../../docs/M6_TOOLING.md) and
[conformance v1](../../../spec/tooling/M6_CONFORMANCE_V1.md) distinguish historical artifacts from
required final integration evidence. Both remain under verification until the complete records,
durable runnable documentation and real hosted gates are accepted.
