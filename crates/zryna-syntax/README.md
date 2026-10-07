# Zryna syntax

Owns the versioned, provider-neutral syntax contract below every replaceable frontend.

Protocol v2 represents executable syntax as bounded flat arenas. Raw wire values remain untrusted;
only source-map-backed verification can construct the opaque verified project consumed by Zryna
semantics. This crate does not parse TypeScript, resolve names, assign semantic types, or construct
IR.

The isolated `v5` candidate defines untrusted bounded-generic DTOs, source-backed
declaration/header checks and a separate complete-source/arena verifier under review.
Its immutable syntax seal is bound to one source map; it grants no semantic or target
authority. It authenticates identifier roles using per-file module context and complete
type/body ownership. The focused complete-source and hostile Rust tests now execute in
the cloud continuation; exact-revision receipts accompany the draft PR. Provider parity,
successor review and public admission remain separate gates. See
[syntax protocol v5](../../docs/SYNTAX_PROTOCOL_V5.md) for the exact boundary and evidence limits.
