# Native C Universal IR extension

Internal, independently verified native-requirement IR for the accepted
[native C v0 contract](../../spec/abi/NATIVE_C_INTEROP_V0.md). This component is a
compiler member downstream of source-bound semantic/private authorities. Its
separate registration preserves the base IR/semantics graph without a reverse edge.

`lower` produces untrusted closed claims and invokes mandatory `verify`. Direct
`verify` calls admit hostile candidates against the original `SourceMap` and genuine
`VerifiedPrivateBoundaries`. The private seal retains the actual bodies, captured
declaration/header/policy materials, dual layouts and ownership-runtime issuer.
It never reconstructs an issuer from a digest or a legacy protocol-v4/M3 program.
The independent verifier does not call the lowering producer.

The production modules separate records/views/production from source, value,
call/status/output, private storage and terminal cleanup replay. The complete source
and declaration inventories include unused operations. Value definitions retain
original spans, dense lexical identities, exact arguments, private moves and status
provenance. Explicit effects preserve all fourteen primitives, safe/raw markers,
conditional execution-instance reservations, initialization before output read,
packing/copy stages, genuine private faults and exact reverse cleanup obligations.
Foreign errors, private traps, host/ABI failures and process failures remain distinct.
C4100–C4107 preserve their existing producing categories; this single-error API never
uses terminal-report C4108 as a catch-all. Runtime live64 remains a shared conditional
reservation requirement, not a static limit on lifetime acquisitions.

Read-only views expose sealed functions, operations, values, storage requirements and
outcomes. They provide no native MIR, runtime allocation, process execution, object,
link/publication or public profile capability. Initial public C entries remain total
scalar exports only. Foreign pointer validity and malformed-result release still
depend on the exact retained reviewed library promises.

The `contract` module reexports closed record types already returned by these views for downstream
machine planning. Copies cannot construct any source, layout, runtime or program issuer. The
separately admitted `zryna-native-mir::native_c_v0` retains this actual immutable IR authority;
its new normal dependency points downstream without a base-IR or semantics backedge.

Focused proposed verification is `cargo test --locked -p zryna-native-c-ir`, with
capture, admission, hostile and limits integration targets, plus the architecture
registration fixtures in `zryna-architecture/tests/native_c_ir_graph.rs`. These test
sources are not a claim of execution. Linux/Windows M0 and the full #417 native
header/MIR/ELF/link/runtime/fault matrix remain required before integration or support.
