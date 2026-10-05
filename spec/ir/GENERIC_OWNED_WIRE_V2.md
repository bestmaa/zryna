# Internal generic owned wire v2

This additive #416 successor proves whole String ownership inside Option/Result and exact
shared/exclusive loans for its admitted source forms. It does not complete the full
[generic contract](GENERIC_INSTANTIATION_V1.md) or activate a public profile. The
[Copy wire v1](GENERIC_COPY_WIRE_V1.md), its decoder and Copy issuer remain separate.

## Encoding

The exact domain is `ZRYNA-GENERIC-OWNED-IR-V2\0`, followed by little-endian u32 version 2.
All counts, value IDs, block IDs and positions are unsigned four-byte little-endian lanes.
Booleans are exactly one byte 0 or 1. A vector is its u32 count followed by its elements.
A blob is its u32 byte count followed by those bytes. The complete fields are:

| Record | Ordered fields |
| --- | --- |
| Program | core blob; function-extension vector; function-plan vector |
| Core blob | Complete frozen v1 message, including its domain/version and exact existing graph encoding |
| Function extensions | Extension vector, ordered by result ID |
| Extension | result ID; opcode byte; opcode operands |
| Function plan | Step vector, in canonical block/position order |
| Step | block; position; failure bool; end-loan ID vector; cleanup ID vector |

The embedded graph retains source spans, complete instance/type inventories and both layout
fingerprints. Each extension replaces precisely one graph `Unit` instruction with its own
declared result type. It cannot replace another core operation, repeat an ID or be omitted from
complete source replay. The embedded message alone grants no owned execution authority.

| Opcode | Ordered operands |
| --- | --- |
| 1 StringLiteral | UTF-8 blob |
| 2 Move | value ID |
| 3 Borrow | value ID; exclusive bool |
| 4 EndLoan | loan value ID |
| 5 Drop | owner value ID |
| 6 CloneString | exact String owner ID |

Unknown domains, versions, opcodes, noncanonical booleans, invalid UTF-8, truncation and trailing
bytes reject the complete message. Encoding checks its result with the independent decoder.
`DecodedProgram` exposes only immutable untrusted claims; only independent verification can
construct the opaque `VerifiedOwnedProgram` accepted by the three emitters.

## Authority and ownership

The seal retains the exact source-bound dual layouts, ownership runtime declarations and
unchanged scalar export ABI. Verification checks original function/import identity and call
acyclicity, every original symbolic body including unused templates, closed substitution,
complete function/type demand, exact source instruction/binding/order replay, typed CFG and
dominance, then an independently derived owner/loan/drop plan. Altering source operations,
cleanup IDs, loan endings, failure placement or active-payload transfers cannot authenticate.

Opaque `T` is affine even when specialized to i32. Whole moves consume a live owner once.
Construction transfers the selected payload; by-value match consumes the enum and transfers
only its active payload. Shared loans exclude consumption and exclusive loans. Exclusive
parents freeze while payload subloans exist. Arm subloans end before their parent or join.
Loans cannot return or be transferred as ordinary CFG edge arguments. Retained loans require
identical incoming state. Joined owner state must match exactly.

Structured source branches use existing bool Branch and empty Jump edges. No wire opcode or
type carrier changes. Continuing arms close their lexical locals; incoming binding identities
and complete availability/loan states must agree without implicit repair. Both returning arms
return directly, with no join parameter or unreachable join. Branch-dependent replacements,
loops and loan-carrying edge arguments remain outside this source slice.

Steps occur before a fallible instruction or at successful return. `position` equals the
instruction index, or the instruction count for return. A failure step ends current loans and
drops live owners in reverse creation/transfer order. Call arguments transfer to the callee
before the caller's failure plan; the callee cleans those arguments on failure, and the caller
does not release them a second time. An allocation or clone result becomes an owner only on
success. Successful return transfers the result before cleaning remaining owners. Explicit
scope and arm cleanup operations are checked independently of these plan claims.

## Resource ceilings

The complete message ceiling is 32 MiB. Before allocation, each decoded vector checks its
count, minimum remaining encoded bytes and total amplification. The sum of vector children
in the extension/plan envelope is at most 1,048,576; the embedded frozen graph separately
retains its own inherited ceiling. No combined larger single-budget claim is made.

| Envelope member | Maximum |
| --- | ---: |
| Function extension lists or function plans | 65,536 |
| Extensions per function | 16,384 |
| Plan steps per function | 20,480 |
| End-loan or cleanup IDs per step | 16,384 |
| UTF-8 bytes in one String literal | 65,536 |
| Aggregate planner state-copy/cleanup credits | 1,048,576 |
| Source branch snapshot/restore credits, per original / complete closed production | 1,048,576 |

Inherited graph, source, layout and instantiation ceilings apply independently. Checked
overflow and the first extra member fail atomically; allocation failure remains an
infrastructure result. Structural/forged claims report I7001 and IR resource exhaustion I3201.
Authenticated invalid consuming or borrow operations report M7007 with the exact original
UTF-8 byte span. The semantic producer maps its unsupported source lane to M3008 and its
resource failure to M7201. General canonical multi-error ownership selection remains work
for full acceptance; this issuer returns its first independently witnessed ownership error.

See [owned execution evidence and boundaries](../../docs/M7_GENERIC_OWNED_EXECUTION.md).
