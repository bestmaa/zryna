# zryna-layout

Compiler-owned verification authority for canonical M3 type identities, aggregate layouts, and
sealed layout fingerprints. Raw type graphs are untrusted. Only this crate can construct the
opaque immutable views consumed by later compiler and runtime layers.

The separate internal `generic_v1` verifier recomputes complete successor keys, dense sorted IDs,
original declaration/argument consistency, by-value cycles and physical records on both storage
targets. It uses a distinct fingerprint domain and publishes read-only source/universe-bound
type IDs. `verify_claim` independently recomputes the complete record document and rejects changed
bytes, fingerprint or target. Original aggregate-v1 records, keys and public entrypoints retain
their format; the shared physical formulas operate on unsealed records and issue no M3 authority.

Authenticated bounded-generic semantics supplies the original declaration inventory and closed
substituted members through its separate source-to-layout producer. Raw graph fixtures exercise
the layout boundary; they do not establish source admission. Fixed #415 records/digests, owned
prefixes, hostile claims and synthetic type/data/depth/key/object boundaries have focused tests.
Executable generic IR, ownership, standard enum operations and target emission remain separate
issue #416 work. This verifier grants no existing `DataOwnershipV1` or backend authority.

This crate does not lower syntax, allocate memory, expose aggregate ABI, emit target code, or
activate the public `DataOwnershipV1` profile.
