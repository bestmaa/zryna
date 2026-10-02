# WIT capability profiles v1

The separate [bounded command implementation candidate](../../docs/WASI_COMMAND_GETTING_STARTED.md)
uses this exact pinned command world and a narrower one-key environment grant; its
[initial grant table](../../docs/WASI_COMMAND_GETTING_STARTED.md#one-explicit-environment-value)
keeps every other category denied. It changes no registry bytes, WIT identity or eligible ceiling.
Complete acceptance and final review remain pending before its public support state changes.

This document specifies the contract-only M4 foundation tracked by Issue #167. It assigns stable
WIT identities and bounded host-capability policies. It does not emit a component, generate a
binding, instantiate a runtime, add a CLI selector, or publish a supported profile.

## Normative artifacts and sources

The local package is `zryna:capability-profiles@0.1.0`; its source is
[`capability-profiles-v1/worlds.wit`](capability-profiles-v1/worlds.wit). The authoritative
machine-readable projection is
[`tests/wit-capability-profiles-v1.json`](../../tests/wit-capability-profiles-v1.json), validated
against [`schemas/zryna-wit-capability-profiles-v1.schema.json`](../../schemas/zryna-wit-capability-profiles-v1.schema.json).
Capability requests use
[`schemas/zryna-capability-request-v1.schema.json`](../../schemas/zryna-capability-request-v1.schema.json).

The WIT grammar is pinned to WebAssembly/component-model commit
[`2bed77e4228841c1d2721996d3ecc169ff96b158`](https://github.com/WebAssembly/component-model/blob/2bed77e4228841c1d2721996d3ecc169ff96b158/design/mvp/WIT.md).
That specification makes a world the complete set of component imports and exports and gives
packages full semantic-version identities. WASI is pinned independently to release `v0.2.12`,
commit [`281ba75fafcd50961ef55f9e52747afcc9b71ede`](https://github.com/WebAssembly/WASI/tree/281ba75fafcd50961ef55f9e52747afcc9b71ede).
No unversioned or version-ranged WASI interface is admitted.

The repository registry validator recognizes and fail-closed parses the exact WIT subset used
here: one package declaration followed by flat worlds containing fully qualified interface
imports and exports. It is not a replacement for a general Component Model toolchain. The
separate backend dependency audit described below parses the complete authenticated source set
with the pinned upstream parser, resolves its dependency graph, and independently compares the
resolved packages and world interfaces with exact pins. The audit reports the accepted explicit
imports separately from the parser-elaborated dependency closure; it does not treat synthesized
type dependencies as additional host-capability grants.

## World identities

| Profile | Exact world | Host imports | Guest exports | Contract state |
| --- | --- | ---: | ---: | --- |
| browser | `zryna:capability-profiles/browser@0.1.0` | 0 | 0 | specified only |
| command | `zryna:capability-profiles/command@0.1.0` | 13 | `wasi:cli/run@0.2.12` | specified only |
| server | `zryna:capability-profiles/server@0.1.0` | 4 | `wasi:http/incoming-handler@0.2.12` | specified only |

The host-import counts above are the explicit declarations authenticated by the accepted source
and registry. WIT resolution elaborates interfaces needed by imported or exported interface types,
so the exact resolved observations contain 0 imports for browser, 16 for command, and 8 for server.
Command adds `wasi:io/error`, `wasi:io/poll`, and `wasi:io/streams`; server adds those interfaces
plus `wasi:http/types`, all at `0.2.12`. These are transitive type dependencies already within the
declared interface contract, not permission widening, new registry rows, or ambient host grants.

The empty browser world is deliberate. It records the capability-free identity without claiming
that today's core `wasm-web` artifact is a component. Omitting M4 profile selection preserves the
current import-free core-WebAssembly behavior exactly.

The command world lists each imported interface instead of including `wasi:cli/command`; this
makes every authority visible in the checked registry. The server world similarly selects the
HTTP handler interfaces and omits filesystem and environment interfaces. Standard I/O, terminal,
exit, and insecure-random interfaces are not in this contract.

## Denied-by-default selection

A profile is an upper bound, not an ambient grant. Before any future instantiation, a request must
name the exact profile and each requested capability. An omitted request list is empty. An omitted
capability is denied even if the selected profile permits it. An unknown profile, capability,
field, interface, or version fails before instantiation. There is no fallback to a broader world.

| Capability | Browser | Command | Server |
| --- | --- | --- | --- |
| clock | denied | granted | granted |
| environment | denied | granted | denied |
| filesystem | denied | granted | denied |
| network | denied | granted | granted |
| randomness | denied | granted | granted |

`granted` means only that the profile may satisfy the exact registry interfaces after the request
also names the capability. It does not select paths, endpoints, environment entries, entropy
sources, or clock policy. Those host-owned values must be supplied explicitly within the limits
below. Supplying an imported function that always rejects is not evidence that a forbidden
capability was granted.

The registry projection is exhaustive: each profile contains all five capability rows in the
canonical order `clock`, `environment`, `filesystem`, `network`, `randomness`. A denied row has no
interface and both limits are zero. A granted row has at least one exact interface and two
positive limits. The ordered union of granted interfaces must equal the world's WIT imports.

## Resource limits

Limits apply per component instance. A future host must reject a request before instantiation if
any declared maximum is exceeded; it must not truncate, wrap, partially grant, or silently replace
the request. Counts include resources retained by imported interfaces and their children.

| Capability | Metric | Browser | Command | Server |
| --- | --- | ---: | ---: | ---: |
| clock | simultaneous subscriptions / timers | 0 / 0 | 64 / 64 | 1,024 / 1,024 |
| environment | entries / total UTF-8 bytes | 0 / 0 | 128 / 65,536 | 0 / 0 |
| filesystem | preopens / open descriptors | 0 / 0 | 16 / 256 | 0 / 0 |
| network | allowed endpoints / concurrent operations | 0 / 0 | 64 / 128 | 128 / 1,024 |
| randomness | bytes per call / bytes per instance | 0 / 0 | 65,536 / 8,388,608 | 65,536 / 8,388,608 |

An endpoint is one normalized host-and-port policy entry. A network operation includes an open
socket, DNS lookup, or HTTP request retained by the host. Environment byte accounting is the sum
of UTF-8 key and value bytes; duplicate keys are invalid rather than counted twice. A preopen is
one host-selected directory authority, not a guest path pattern. A descriptor or child resource
must be released before its slot can be reused. Clock and randomness values remain
nondeterministic host inputs; deterministic compilation and registry validation do not promise
deterministic runtime values.

The WIT source itself is limited to 16,384 UTF-8 bytes, three worlds, five capability rows per
profile, seven interfaces per capability, and thirteen imports per world. The validator accepts
the exact boundary and rejects the first extra unit.

## Versioning and compatibility

There are three independent identities:

1. registry schema `zryna.wit-capability-profiles.v1` controls JSON shape and validation;
2. WIT package `zryna:capability-profiles@0.1.0` controls world/interface identity; and
3. WASI `0.2.12` controls every external interface identity.

Consumers must match all three exactly. A comment or documentation-only clarification may leave
them unchanged. Any world import/export change, new capability, weakened denial, larger resource
limit, different WASI release, or changed request outcome requires a reviewed registry/schema
revision and a new WIT package version. Removing or changing an existing world requires a new
incompatible identity; no subtyping claim is inferred from WIT `include`. Because the local
package is pre-1.0 and specified-only, no public compatibility or support promise exists yet.

A later implementation may add a new registry version while retaining this file as historical
evidence. It must not rewrite a released registry or reinterpret a v1 denial as a grant.

## Validation and diagnostics

[`scripts/wit-capabilities/validate.mjs`](../../scripts/wit-capabilities/validate.mjs) verifies the
schemas, source digest, exact package/world identities, WIT import/export projection, exhaustive
capability rows, default denials, limits, and deterministic request outcomes.

| Code | Meaning |
| --- | --- |
| `ZRYNA-C4000` | malformed schema, WIT, identity, version, ordering, or registry projection |
| `ZRYNA-C4001` | requested or encoded capability is denied |
| `ZRYNA-C4002` | usage does not belong to the named requested capability |
| `ZRYNA-C4003` | request exceeds a profile resource limit |
| `ZRYNA-C4004` | WIT or registry structural bound is exceeded |

Committed fixtures cover a capability-free browser request, exact-limit command and server grants,
browser filesystem denial, server environment denial, an unknown capability, unrequested usage,
and the first environment entry beyond the command limit. Tests also synthesize every one of the
fifteen profile/capability decisions, malformed WIT and registry mutations, deterministic replay,
and exact/first-extra WIT byte boundaries.

## Executable dependency audit

Issue #383 adds an executable prerequisite in the existing WebAssembly backend without changing
the worlds above. `wit-parser` is exactly pinned to `0.258.0` with default features disabled and
only `std` enabled. Its crates.io archive SHA-256 is
`ff4daaa3cd97ae49ecd0a99dc009d453f93e0f083dd3be38c0f24a83a93e37ac`, published from
`bytecodealliance/wasm-tools` commit `5c6d31c78f8bd503f558441ef9c732950a141d1a`. This matches the
repository's existing `wasmparser` and `wasm-encoder` release line rather than creating a second
Component Model toolchain.

[`wit_world_audit`](../../crates/zryna-backend-webassembly/src/wit_world_audit.rs) owns a typed,
effect-free audit entrypoint. It first authenticates the local source and 33 vendored WASI source
files by exact SHA-256, then creates a fresh resolver, resolves all eight exact package identities,
and independently compares each resolved browser, command, and server import/export set. It does
not expose parser state as authority. The browser observation must remain empty.

The parser boundary accepts at most 34 files, 32 KiB per file, 16 KiB for the local root, 256 KiB
in total, and 96 bytes per logical path. Resolved output is capped at eight packages, 32
interfaces, 16 worlds, and 4,096 types. Input paths are sorted before authentication, source
maps sort their files, returned identities are canonical, and no resolver survives a call. The
accepted closure is 141,709 bytes. Exact source provenance, package-tree identities, upstream
package hashes, license notice and hostile fixtures are recorded with the
[backend evidence](../../crates/zryna-backend-webassembly/tests/wit-world-audit-v1/README.md).

| Code | Meaning |
| --- | --- |
| `ZRYNA-W4000` | UTF-8, WIT parsing or dependency resolution failed |
| `ZRYNA-W4001` | a source is missing, unknown, duplicated or does not match its reviewed hash |
| `ZRYNA-W4002` | a resolved package, version, world or interface differs from the accepted contract |
| `ZRYNA-W4003` | a pre-parse or resolved-graph resource bound is exceeded |

Successful audit is evidence about interface identity only. It does not emit or instantiate a
component, generate bindings, select a public profile, or grant host permissions.

## Dependency and rollout boundaries

Issue #357 owns cross-target and transitive dependency capability composition. Issue #359 owns
JS/WASM value conversion and resource adapters. Issue #168 owns package and release formats.
This contract supplies identities and host-policy inputs to those tasks but implements none of
their behavior.

Component emission, host bindings, runtime enforcement, CLI activation, examples, publication,
and a public profile require separate implementation, conformance, and activation gates. Existing
M0–M3 source, IR, backend, manifest, CI, and authenticated inventory contracts are unchanged.
