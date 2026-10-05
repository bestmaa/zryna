# Internal generic owned wire v3

This finite #416 continuation transports the compiler-generated String payload leaf used by
structural clone of stored `Option<String>` and `Result<String,String>`. It does not activate a
public profile or expand source clone to borrowed Strings. The exact domain is
`ZRYNA-GENERIC-OWNED-IR-V3\0`, followed by little-endian u32 version 3.

All core, extension, plan, boolean, vector and blob encodings are exactly those documented for
[owned v2](GENERIC_OWNED_WIRE_V2.md). Opcodes 1 through 6 retain their meanings. The sole added
opcode is 7, followed by a four-byte little-endian operand ID. Independent typing requires
exact shared `Borrow<String>` and exact fresh stored String result. Independent owner replay
also requires an actual live shared loan; an exclusive loan with a forged shared type fails.
Ended, foreign and nonloan operands reject. The compiler source producer alone creates the
selected-payload operation after borrowing and matching the named enum owner once.

The leaf borrows its operand and retains the original owner on success and failure. Before
runtime allocation, its independently derived failure step snapshots current owners in reverse
creation/transfer order and loans in reverse ID order. The unwritten result is absent from that
snapshot and becomes owned only on success. Rebuild transfers the cloned payload once, ends
the child loan before its join, then ends the outer temporary loan. Original and surrounding
owner/loan states remain unchanged. None and inactive payloads do not execute a clone call.

V2 encoding and decoding both reject opcode 7. V2 and v3 reject each other's domains and
versions. Unknown opcodes, malformed boolean/UTF8, truncation, hostile lengths and trailing
bytes reject before exposing a partial decoded carrier. Both domains retain the 32 MiB complete
message ceiling, 1,048,576 envelope child ceiling and each v2 member ceiling; the embedded
frozen v1 graph independently retains its own limits. No larger combined-budget claim is made.
The shared accounting primitives check exact/first-extra transport and child credit, integer
overflow, and the 65,536/65,537 String literal boundary before allocation.

V3 decode returns the same opaque untrusted `DecodedProgram`. The existing owned verifier
still checks source, original opaque bodies, complete demand, dual layout/runtime identities,
typed CFG, source replay and independently derived cleanup before issuing the same
`VerifiedOwnedProgram`. Native lowering still requires the existing mandatory MIR seal.
No direct v3 executable issuer, MIR operation, runtime symbol, ABI or layout is added.
All three emitters lower opcode 7 through the same retained String clone contract as opcode 6;
native relocation auditing and operand normalization include the new leaf explicitly.

Full #416 acceptance remains incomplete. Nominal/container and nested structural clone,
borrowed source clone, partial state, public activation, full runtime/status qualification and
supported-platform Windows generic execution are outside this finite proof.
