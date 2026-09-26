# Closed generic and standard enum layout v1 proposal

Status: specified candidate for [Issue #415](https://github.com/zryna/zryna/issues/415).
This is a proposed successor to [aggregate layout v1](AGGREGATE_LAYOUT_V1.md),
not an interpretation of its existing tags or SHA-256 fingerprint. Existing
layout records remain byte-for-byte unchanged and reject these new forms.

A closed generic nominal instance is identified by declaration kind,
`(ModuleId, declaration-index)` and ordered complete argument keys. `Option<T>`
and `Result<T,E>` use separate compiler-reserved family tags and ordered complete
argument keys. No source declaration can claim either family identity. These
keys are leaves with respect to nominal fields, as in v1; argument keys are
recursively length-prefixed and must be complete, finite and within the proposed
4,096-byte and depth-64 instantiation limits. The next version must freeze exact
binary tags and lengths with fixed digest fixtures before implementation; v1
tags cannot be silently reused.

After substitution, ordinary generic structs use v1 field layout. Generic enums,
Option and Result use v1's four-byte ordinal and maximum active payload formula,
with the same `Linear32V1` and `LinuxX8664V1` targets, object ceiling, checked
arithmetic, by-value cycle rejection and target-specific fingerprints. `Option`
ordinals are `none=0`, `some=1`; `Result` ordinals are `ok=0`, `err=1`.
There is no null niche, tag folding, field reordering or padding observation.
The exact `Option<i32>` and `Result<i32,bool>` fixtures are each size 8,
alignment 4 and payload offset 4 on both targets. `Option<String>` is size 16,
alignment 4, offset 4 on Linear32V1, and size 32, alignment 8, offset 8 on
LinuxX8664V1. Only the selected payload bytes are initialized or dropped.

The successor sealed record includes a family/nominal instance key, closed
argument IDs, substituted field/variant records, sizes, alignment, offsets,
drop/runtime metadata and StorageTarget. Its fingerprint uses a new domain
separator and new record tags; it cannot reuse `ZRYNA-AGGREGATE-LAYOUT-V1` or
pretend that `Option<T>` is a source nominal `(ModuleId, declaration-index)`.
Consumers require the exact matching successor fingerprint. Invalid target,
unknown argument, cross-target fingerprint, changed ordinal, wrong payload,
overflow, excessive depth and by-value cycle all fail before code generation.

Fixed fixtures must cover primitive, owned and nested arguments, both storage
targets, recursive indirection, direct/indirect by-value cycles, exact/first-extra
type/depth/key/object limits, checked arithmetic with synthetic small targets,
and a one-byte key/fingerprint mutation. A later versioned encoding proposal
must provide complete bytes and digests before any backend consumes it.
