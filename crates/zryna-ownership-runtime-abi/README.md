# zryna-ownership-runtime-abi

Compiler-private declarations and verification for `zryna-ownership-runtime-v1`.

This crate seals logical operations, status/result contracts, target mappings, checked native
header bytes, and layout-derived runtime metadata. It contains no allocator, runtime helper,
backend, object parser, driver integration, or public aggregate host ABI.

The sealed authority exposes immutable status declarations for the exact seven-row contract. A
consumer may inspect the authenticated status, closed success/trap/branch/host disposition, and
closed trap identity without recovering raw declarations. Forged disposition or trap metadata is
rejected before that view exists.

Pure transition validation is authority-sensitive. Atomic failure evidence names the exact logical
operation before its status can be accepted. Vec allocation and reserve evidence consumes an opaque
verified element-layout view and checks both the element-count maximum and checked
`capacity * stride` amplification against the dynamic-allocation byte maximum. The legacy
context-free transition entry point remains source-compatible but rejects those three contextual
claim forms; callers must use the operation-bound or sealed-layout-bound validators.

`validate_vec_transition` remains the source-compatible layout-bound API, but its raw
`TransitionClaim` does not retain an intrinsic target/element identity or the old reserve storage
pointer. New consumers that need replay-resistant evidence construct an opaque
`BoundVecTransitionClaim` from the exact `VerifiedElementLayout` and pass it to
`validate_bound_vec_transition`. That validator rejects another target or element view and requires
a successful no-growth reserve to return the exact old storage pointer. It also preserves exact
old storage, length, and capacity on failure and retains the existing checked element-count and
`capacity * stride` byte-amplification rules. Neither API executes a runtime or authenticates an
allocator implementation.


The additive `generic_v1::{raw_v1,verify_v1}` issuer binds both complete successor layouts in a
separate `zryna-generic-ownership-runtime-v1` domain. It independently derives Vec strides and
Shared/Weak control records and retains compilation-branded operation/element/payload identities.
The exact fixed helper/status/header inventory is shared through a private declaration builder;
root `verify_v1`, old IDs and old transition semantics remain unchanged. The successor seal is
not convertible to an older ABI seal and is a declaration proof, not a runtime implementation.
