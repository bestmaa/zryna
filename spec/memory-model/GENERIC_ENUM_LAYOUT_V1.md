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
4,096-byte and depth-64 instantiation limits. The candidate successor key
encoding retains v1 primitive/container tags and assigns these unused tags:

```text
12 || u32 ModuleId || u32 declIndex || u32 argCount || childKey[argCount]
   generic struct
13 || u32 ModuleId || u32 declIndex || u32 argCount || childKey[argCount]
   generic enum
14 || u32 argCount=1 || childKey(T)                 Option<T>
15 || u32 argCount=2 || childKey(T) || childKey(E)  Result<T,E>
childKey = u32 byteLength || complete child key bytes
```

All lanes are unsigned little-endian. The count is one or two for user nominal
types, exactly one for Option, exactly two for Result. A mismatch is invalid.
These tags are candidate assignments awaiting maintainer acceptance, not an
extension to the currently implemented v1 key parser. Two fixed key fixtures
are `Option<i32>` = `14010000000100000001` and `Result<i32,bool>` =
`150200000001000000010100000000`. Their respective SHA-256 digests are
`0e0115ef9544b1acda88ad090f8c060ee9e3e90788d6d216d82667753f3944c3`
and `41ddfae250c2303185a0d65e1848e81056ac5fa5ef458e3b50e328f38b3ede33`.
An independent implementation must reproduce those bytes and digests.

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
drop/runtime metadata and StorageTarget. The candidate fingerprint document uses
the v1 record prefix and target/count encoding with domain separator ASCII
`ZRYNA-GENERIC-AGGREGATE-LAYOUT-V1\0`; unchanged v1 records keep their exact
tag-specific payloads. New record tags are `10=generic struct`,
`11=generic enum`, `12=Option` and `13=Result`. A generic struct record payload
is `u32 ModuleId, u32 declIndex, u32 argCount, argCount * u32 argTypeId,
u32 fieldCount`, followed by v1 struct field records. A generic enum record
uses that same identity/argument prefix followed by v1 enum variant count,
payload offset/area and variant records. Option/Result records omit ModuleId and
declaration index; their payload begins `u32 argCount, argCount * u32
argTypeId` and then uses the v1 enum variant count, payload offset/area and
variant records. The family tag is the identity; no source nominal is forged.
The record length includes every prefix and payload byte exactly as in v1.
The complete sorted type universe is fingerprinted with SHA-256. A fixed
Linear32V1 fixture containing only the mandatory `bool=0`, `i32=1`, `String=2`
records and `Option<i32>=3` has four records in that order. Its Option record is
`4c0000000c00000003000000000000000000000008000000000000000400000000000000`
`0100000001000000020000000400000000000000040000000000000000000000ffffffff`
`0100000001000000` (concatenated hex). The full 230-byte fingerprint document
has SHA-256
`1701d9b3c46f81f99b2e1e992528a08dde5956364de423526ddfb8b9e796925e`.
Independent encoders must reproduce the record and digest and reject a mutated
tag, argument, target or ordinal. These proposed bytes still require maintainer
acceptance; current v1 fingerprints cannot be reused.
Consumers require the exact matching successor fingerprint. Invalid target,
unknown argument, cross-target fingerprint, changed ordinal, wrong payload,
overflow, excessive depth and by-value cycle all fail before code generation.

Fixed fixtures must cover primitive, owned and nested arguments, both storage
targets, recursive indirection, direct/indirect by-value cycles, exact/first-extra
type/depth/key/object limits, checked arithmetic with synthetic small targets,
and a one-byte key/fingerprint mutation. Remaining generic struct/enum and
Result full-record fixtures must be independently checked before implementation.
