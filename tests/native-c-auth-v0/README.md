# Native C source/declaration authentication fixtures

`library-policy.json` is the fixed independent policy capture for the existing
`fixture-c-v0@0` reference contract. Its SHA-256 is
`918edc899c9fff69bdc4b79d0cc8332af01f0e4b6b15f4778bf643809ce0dba8`.
The canonical `{allocators,kinds,operations}` bytes retain every import policy
field and exclude only operation `sourceBinding`, as required by the accepted
source contract. They were materialized from the existing normative fixture
with a separate Node serializer, outside the Rust authority implementation.
Rust tests consume these fixed bytes, the original header and complete source
fixtures independently of the sidecar producer.

The isolated syntax authority parses complete restricted function bodies into
flat expression arenas and authenticates all intrinsic sites against one exact
immutable SourceMap. The declaration authority checks exact captured source,
header and policy bytes, declaration identities, signatures, nominal allocator
relations, output/status promises and source bindings. Neither constructor
type checks private bodies or proves linear ownership, status dominance,
runtime freshness, cleanup, arbitrary C safety, native linkage or execution.
Those require later independent typed-body, IR, MIR and driver authorities.

The grammar covers explicitly typed functions, initialized immutable locals,
returns, canonical terminal status guards, scalar literals/addition and the
fourteen reserved foreign intrinsics. Unknown complete-input occupants reject;
unsupported general language forms do not become a smaller accepted program.
Existing syntax protocols and M3 sealed values retain their existing vocabulary.

The new Rust tests are source-stage fixtures until their exact candidate has
actually compiled and run. Focused Node composition evidence remains separate.
