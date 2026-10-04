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

## Full-feature acceptance still open

The normative acceptance owner remains
[`NATIVE_C_INTEROP_V0_ACCEPTANCE.md`](../../../../../../spec/abi/NATIVE_C_INTEROP_V0_ACCEPTANCE.md).
The preserved #515/#520/#521/#524/#525/#527 draft stack carries source,
typed-flow/IR/MIR, scalar, handle, byte-copy, private runtime and shared process
work. This boundary adds acquired-object structure and separate-object fixture
evidence; it does not turn those drafts into an integrated complete feature.

| Acceptance boundary | Current evidence | Remaining work |
| --- | --- | --- |
| Source and ABI identity | Genuine retained authority; equal-byte independently recaptured source issuer rejects. Reviewed header and policy bytes compare exactly. | Preserve these authorities when real artifact acquisition is connected. A compatible C prototype does not authenticate body promises. |
| Foreign object identity/inventory | Immutable actual ET_REL bytes, full strong definition/dependency inventory, raw tables and relocation audit; independent hostile producers and exact limit cases. | Ordered real-library/runtime/sysroot/tool acquisition, authenticated library review and plan binding. A supplied dependency list is no execution permission. |
| Executable fixture calls | Separate C object scalar and handle calls, allocation-prefix/reverse-release trace, actual linker failure and SIGABRT. Earlier byte/runtime tests remain separate evidence. | Reconcile the complete normative matrix on the final integrated revision; extend separate-object byte/private-runtime and sanitizer evidence. The helper's existing executable audit only checks its limited required-symbol/ELF rules, not a complete linked dependency inventory. |
| Recipe and host authorization (#405) | No grant introduced. Existing unconditional recipe admission guards remain authoritative. | Actual independent supervisor/OS enforcement proof, approved ordered build inputs, retained tool identities, complete linked-output/runtime dependency audit, create-only publication and cache/provenance binding. Hashes and caller flags cannot replace these. |
| Real libraries | Tiny reviewed fixture with explicit malloc/free dependency inventory. | Separate SQLite and Rust C-ABI shim proofs, each with exact acquired artifacts, operation policies, cleanup/failure matrix and provenance. |
| Platforms and release | Linux x86-64 fixture tests; portable structural tests. No CLI/profile activation. | Windows validation runs separately; no Windows native-C or sanitizer result is implied. Public activation, release and deployment require separate decisions. |

Plain cloud descendant cleanup and a scoped init/subreaper run must remain
distinct evidence. Passing the latter cannot be reported as a clean plain
baseline. #417 stays open until its complete acceptance requirements are met.
