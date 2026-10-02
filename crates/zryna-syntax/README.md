# Zryna syntax

Owns the versioned, provider-neutral syntax contract below every replaceable frontend.

Protocol v2 represents executable syntax as bounded flat arenas. Raw wire values remain untrusted;
only source-map-backed verification can construct the opaque verified project consumed by Zryna
semantics. This crate does not parse TypeScript, resolve names, assign semantic types, or construct
IR.

The isolated `native_c_v0` module decodes the specified source-bound C sidecar into
public **untrusted** records. It checks bounded canonical wire, required nullable
fields, the closed vocabulary and collection/range limits, and independently
supplied target selection. It preserves distinct `c-i32`, `c-int` and Boolean
shim spellings. It does not authenticate header/policy/source identities, parse
foreign source nodes, verify ownership policies or construct sealed compiler
authority. Existing protocols v2/v3/v4 do not accept foreign forms through this
decoder. See the [native C contract](../../spec/abi/NATIVE_C_INTEROP_V0_SOURCE.md)
and [isolated prototype](../../tests/native-c-prototype/README.md).

The separate `native_c_source_v0::authenticate_sources` constructor parses every
complete file of a restricted foreign-function grammar from one immutable
`SourceMap`. Its opaque result retains the exact map identity, complete flat
expression arenas, original byte identities and independently collected
intrinsic spans. No provider AST, sidecar site inventory or digest claim can
construct this result. Unknown complete-input occupants reject. This distinct
authority establishes syntax correspondence only; names, body types, linear
tokens, status dominance, cleanup, totality and execution remain unverified.
Its source/arena budgets are local to this extension and do not enlarge any
existing protocol. See the [independent captures](../../tests/native-c-auth-v0/README.md).
