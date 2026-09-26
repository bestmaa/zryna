# Generic value boundary proposal

Status: specified candidate for [Issue #415](https://github.com/zryna/zryna/issues/415).
This document owns the boundary decision for the initial internal feature; it
does not change [scalar ABI v1](SCALAR_V1.md) or
[ownership runtime ABI v1](OWNERSHIP_RUNTIME_V1.md).

Every closed generic call is internal to the authenticated module graph. A
verified instance ID identifies one complete substituted signature and cleanup
contract. Each target may choose a private calling carrier, but it must preserve
exact argument/result types, left-to-right evaluation, move and borrow legality,
controlled trap identity, and reverse live-owner cleanup. No target-specific
carrier is a source-visible ABI or a cross-version link symbol. A backend cannot
specialize by representation, merge distinct verified instances, or expose a
generic template as an untyped entrypoint.

Only `i32` and `bool` remain admitted public scalar parameters/results. A
generic function, `Option`, `Result` or any aggregate containing them is rejected
at that boundary before emission, even when one specialization could return a
scalar. Generic nominal values and standard enum values remain internal.
Neither internal layout nor private call carriers create a JS object API, core
Wasm export, native C ABI, WIT resource, Component Model binding or serialization
format. A future aggregate boundary must separately define type/version identity,
discriminant and payload validation, ownership transfer, failure cleanup and
cross-target carriers with adversarial tests.

`Result.err` is a normal value, never a host exception or runtime status.
Allocation, capacity and refcount failures remain the exact controlled traps of
the M3 ownership contract, with cleanup before reporting. `upgradeWeak` keeps
its existing branch ABI until a separate change explicitly adapts it.
The #400 WASI host-grant proposal and #364 native FFI specification retain their
own authority; this issue does not choose F1/F2 grants or C representations.

Required tests reject public generic/aggregate signatures and forged private
instance targets at the owning ABI/IR boundaries. Three targets must demonstrate
the same scalar observations, trap identities and logical cleanup traces for
private Option/Result computations before any public activation.
