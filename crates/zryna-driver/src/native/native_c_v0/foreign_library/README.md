# Retained foreign object boundary (#417)

This module captures one static ELF64 little-endian x86-64 relocatable object
against the genuine original source/IR/MIR/object authority. It preserves the
exact versioned library, header, operation policy and selected program object.
The independently supplied expected object size/hash and dependency inventory
are checked against actual immutable bytes. They are caller-supplied identity
prerequisites, **not authenticated approval or recipe execution authority**.
Changing both bytes and their expected hash can capture different data. It
cannot grant a host capability, cache write, publication or public native route.

The closed inventory checks every declared imported function in the library,
including functions unselected by the current program. It rejects extra global
definitions, weak/common/IFUNC symbols, reserved private runtime names,
constructor/TLS/dynamic sections and unsupported relocation fields or targets.
Raw header/table validation precedes the generic object iterators so malformed
links or truncated relocation arrays cannot silently disappear from the audit.
The accepted subset has at most 16 non-null sections, 4,096 non-null symbols,
32,768 relocations, 64 dependencies, and the existing 8 MiB object/storage limit.
Dependencies are exact sorted unique C identifiers of at most 128 bytes.
This is a deliberately narrow GCC non-PIC object subset, not an archive, shared
library, loader-search or general ELF interface.

The Linux x86-64 tests compile an independently authored seven-operation C
fixture **separately** from the compiler-generated object and observation client.
They use retained capability-relative staging and bounded existing process
helpers. They prove snapshot use after acquired-path replacement, scalar wrap,
reverse cleanup of every accepted two-handle allocation prefix, actual symbol
substitution rejection, and cleanup/retry after an actual unresolved-symbol
link failure. A structurally accepted fixture that calls `abort()` produces
SIGABRT: structural capture cannot establish the safety of a C body or promise
recoverable status, in-process containment or cleanup after a crash.
The test-only foreign object recipe explicitly selects `-fcf-protection=none`
for this closed fixture subset. An independent `-fcf-protection=full` control
emits GNU property metadata and remains rejected. These fixture arguments do
not select or authorize a production recipe's compiler or security policy.

The byte proof additionally compiles the original issuer's private runtime in
a separate translation unit, includes its retained ABI header, and links it
with the generated object, captured foreign object and independent client.
Malloc/free observations verify distinct private copies at lengths 0, 3 and
4096, each allocation-failure point, a mixed handle/byte reverse cleanup prefix,
and controlled malformed metadata. An actual missing-release C mutant fails
the physical owner oracle; the pristine implementation then passes again.
The C library, runtime and client also run with ASan/UBSan. Instrumented objects
remain rejected by production capture; the generated machine code is not
instrumented, and these observations do not establish arbitrary C body safety.

`tests/execution/bytes/reference-provenance.json` records the exact existing
fixture body, normative header/policy and private-runtime template at preserved
checkpoint `42f8e80`. Local proof logs record the actual compiled object and
rendered source digests. These are reviewed fixture prerequisites with no
production approval. Controlled hostile derivatives are identified by their
actual source digest; they are not approved library bodies. No exact operator-
approved SQLite or Rust-shim source/policy or authentic #405 host-admission
capability is available here. The normative stage order requires the audited
raw boundary and reviewed library-specific ownership policy before those proofs.

## Full-feature acceptance still open

`linked_output/` is a Linux x86-64 test-only compile/link observation producer. Its two
fixture consumers retain genuine original requirements and captured-library capabilities,
actual foreign/client/private-runtime objects and sources, the originating GCC/linker
capability for every invocation, exact arguments and outputs, final ELF bytes, and the
linker's observed input order including repeated archive occurrences. The private runtime
producer preserves the original rendered-source/header binding. Client objects are compiled
separately using the existing fixture client warning policy; foreign/runtime compilation
retains strict warnings. No shared staging allowlist or production recipe guard changes.
The observer's uninstrumented fixture profile explicitly selects `-fno-stack-protector`
so distribution GCC defaults cannot add an undeclared runtime dependency. An actual
`-fstack-protector-all` compiler control still rejects `__stack_chk_fail`; the audit's
exact four-import inventory is unchanged. This selects no production security policy.

The observer returns before driver executable permissions or target execution and grants no
execution, cache or publication capability. GCC itself may create executable file modes.
After stage cleanup, a create-only private evidence export can retain the actual artifacts;
`ZRYNA_LINKED_OUTPUT_EVIDENCE_DIR` selects an existing external parent for the scalar/byte
test exports. Without it, each test uses its disposable fixture root. Failure records retain
actual invocation status/stdout/stderr; existing execution and sanitizer paths remain separate.

Observed PT_INTERP, DT_NEEDED, RPATH/RUNPATH, all dynamic entries and raw GNU version sections
are requirements, never an approved provider inventory. Trace output is not complete input-use
attestation. The unchanged limited executable audit still applies, but no full dependency or
loader audit is claimed. Independently issued ordered tool/sysroot/startup/runtime expectations,
authoritative loader/provider/version/load-search policy and production recipe/host admission
remain explicitly missing. Source or output hashes cannot issue those authorities. No other
PR's toolchain or sanitizer runtime policy is borrowed.

The normative acceptance owner remains
[`NATIVE_C_INTEROP_V0_ACCEPTANCE.md`](../../../../../../spec/abi/NATIVE_C_INTEROP_V0_ACCEPTANCE.md).
Draft #534 is the cumulative review route toward main for the source,
typed-flow/IR/MIR, scalar, handle, byte-copy, private runtime and shared process
work, plus this retained-object boundary and separate-object fixture evidence.
Its #515/#520/#521/#524/#525/#527/#533 predecessor commits, branches and evidence
remain preserved; those snapshots contain no unique commits outside this route.
This consolidates review without declaring the complete feature accepted or
granting recipe execution, host permission or public activation.

| Acceptance boundary | Current evidence | Remaining work |
| --- | --- | --- |
| Source and ABI identity | Genuine retained authority; equal-byte independently recaptured source issuer rejects. Reviewed header and policy bytes compare exactly. | Preserve these authorities when real artifact acquisition is connected. A compatible C prototype does not authenticate body promises. |
| Foreign object identity/inventory | Immutable actual ET_REL bytes, full strong definition/dependency inventory, raw tables and relocation audit; independent hostile producers and exact limit cases. | Ordered real-library/runtime/sysroot/tool acquisition, authenticated library review and plan binding. A supplied dependency list is no execution permission. |
| Executable fixture calls | Separate C scalar/handle and byte/private-runtime calls, allocation failures and mixed reverse-release trace, malformed metadata, physical missing-release mutant, actual linker failure and SIGABRT. Independent C library/runtime/client ASan/UBSan observations. | Reconcile the complete normative matrix on the final integrated revision, including remaining byte modes and fault cases. The helper's existing executable audit only checks its limited required-symbol/ELF rules, not a complete linked dependency inventory. |
| Recipe and host authorization (#405) | No grant introduced. Existing unconditional recipe admission guards remain authoritative. | Actual independent supervisor/OS enforcement proof, approved ordered build inputs, retained tool identities, complete linked-output/runtime dependency audit, create-only publication and cache/provenance binding. Hashes and caller flags cannot replace these. |
| Real libraries | Tiny reviewed fixture with explicit malloc/free dependency inventory. | Separate SQLite and Rust C-ABI shim proofs, each with exact acquired artifacts, operation policies, cleanup/failure matrix and provenance. |
| Platforms and release | Linux x86-64 fixture tests; portable structural tests. No CLI/profile activation. | Windows validation runs separately; no Windows native-C or sanitizer result is implied. Public activation, release and deployment require separate decisions. |

Plain cloud descendant cleanup and a scoped init/subreaper run must remain
distinct evidence. Passing the latter cannot be reported as a clean plain
baseline. #417 stays open until its complete acceptance requirements are met.
