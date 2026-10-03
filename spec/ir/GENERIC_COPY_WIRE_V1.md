# Internal generic Copy wire v1

This is the first internal executable Copy slice of #416, not completion or public activation
of the full [generic IR contract](GENERIC_INSTANTIATION_V1.md). Owned values, loan transfers,
mutable control flow, nominal source declarations and runtime fault cleanup remain rejected by
its executable issuer. Existing M1/M2/M3 wire versions and constructors are unchanged.

The domain is the 20 bytes `ZRYNA-GENERIC-IR-V1\0`, followed by little-endian u32 version 1.
All counts, IDs, ordinals, signed i32 bit patterns and source span lanes use four bytes.
Boolean/optional presence values are exactly one byte 0 or 1. Unknown tags, noncanonical bools,
invalid UTF-8 export names, truncation and trailing bytes reject the whole message.
No decoded claim is executable authority.

The original-body pass treats opaque parameters and potentially owned containers as affine
before specialization. Such bindings currently permit at most one whole-body reference,
including unused templates. This conservative restriction also rejects some valid shadowed
or mutually exclusive uses; full path-specific owner/loan/drop replay remains required by #416.

The complete message ceiling is 32 MiB. The decoder checks each vector count before reserving
storage, checks a minimum encoded size against the remaining bytes, and bounds the sum of
vector children to 1,048,576. Keys are at most 4,096 bytes; export names at most 256 bytes.
Inherited raw program budgets are checked after decode and before verification. Byte/count
exhaustion reports `ZRYNA-I3201`; malformed domain/tags/source claims report `ZRYNA-I7001`.
Allocation failure remains a distinct infrastructure result.

Fields occur in this exact order:

| Record | Ordered fields |
| --- | --- |
| Program | module vector; original function declaration vector; type-key blob vector; universe[32]; Linear32 fingerprint[32]; Linux fingerprint[32]; function vector |
| Module | id; original function count |
| Original function declaration | module; function; generic arity; span |
| Span | file; start; end, as UTF-8 byte offsets |
| Function | key blob; span; optional export-name UTF-8 blob; parameter-type vector; result type; block vector |
| Block | id; definition vector; instruction vector; terminator span; terminator |
| Definition | id; value type |
| Instruction | result definition; span; operation |
| Edge | target block; value-ID vector |
| Match arm | ordinal; optional payload definition; ordinary edge, excluding its payload binding |

A blob is u32 byte count followed by exactly those bytes; a vector is u32 element count
followed by records. An optional value has its presence byte followed by the record only if
present. Type tags are Stored=0 followed by ID, Unit=1, Borrow=2 followed by referent ID and
exclusive bool. Borrow can be decoded as an untrusted claim but cannot enter the Copy seal.

| Operation tag | Exact lanes |
| --- | --- |
| 1 BoolLiteral | bool |
| 2 I32Literal | i32 bits |
| 3 Unit | none |
| 4 Copy | value ID |
| 5 I32Add | left ID; right ID |
| 6 ClosedGenericCall | instance ID; argument-ID vector |
| 7 SourceCall | original module; original function; argument-ID vector |
| 8 ClosedEnumConstruct | closed type ID; ordinal; optional payload ID |

| Terminator tag | Exact lanes |
| --- | --- |
| 1 Return | value ID |
| 2 Jump | edge |
| 3 Branch | condition ID; true edge; false edge |
| 4 ClosedEnumMatch | closed type ID; scrutinee ID; mode byte; arm vector |

Match modes are Value=0, SharedBorrow=1, ExclusiveBorrow=2. The latter two remain raw claims;
the Copy executable issuer rejects them. Vector ceilings are modules 4,096, original/closed
functions 16,384, types 65,536, blocks per function 4,096, instructions per block 16,384,
parameters/ordinary edge or call arguments 256, and variants per match 1,024. Complete inherited
aggregate limits still apply, including function-global values and program-global CFG edges.

`tests/m7-generic-copy-fixtures/wire-v1.zir` is an independently packed 277-byte frozen vector,
not output captured from the matching Rust encoder. Its optional-export flag is byte 208 and
literal tag byte 255. The hostile suite checks every truncated prefix, domain/version, count,
opcode, boolean and first-extra message-byte boundary, followed by pristine replay.

`generic_v1::copy_v1::verify` consumes only `wire::DecodedProgram`, exact authenticated protocol-v5
syntax, immutable SourceMap and selected entry FileId, both successor layouts and the separate
generic runtime ABI seal. It independently authenticates signatures/layout members/calls/CFG,
replays every supported original body symbolically (including unused templates), compares
complete closed bodies to their source, checks demanded instance equality, and derives an empty
owned/loan/drop/runtime-operation effect set from every used layout. Only entry-module scalar
exports are passed through unchanged scalar ABI v1. All modules must be in the entry closure.
Typed but fabricated bodies and a foreign equal-text source/runtime/entry brand reject.

The immutable lane admits bool/i32 literals, exact wrapping i32 addition, references,
immutable locals, expression statements, explicit returns, generic/nongeneric direct calls,
and exhaustive by-value Option/Result expression matches. A reference reuses its dominating
value; no implicit clone is introduced. Parameters/results of private generic instances can be
Copy enum values. Original opaque T does not gain scalar capabilities from specialization.
Option.none/some and Result.ok/err ordinals retain the accepted contract; err remains an ordinary
enum branch. Every branch transfers only its active payload through explicit typed CFG parameters.

The internal JavaScript backend consumes only this new opaque program. Its private immutable
records never cross a public host boundary; it uses sealed scalar export names/carriers, exact
arity and parallel edge transfers. The frozen source/DTO test with a Unicode comment and exact LF bytes
executes generic forwarding and both variants of both families under pinned Node 22.22.1.
This is authenticated DTO-to-JavaScript execution, not protocol-v5 parser/provider parity,
Wasm/native conformance, a runtime allocator proof, a driver route or a public profile.
